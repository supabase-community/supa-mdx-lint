use anyhow::Result;
use rules::RuleFilter;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::{fs, io::Read};
use tsify::Tsify;
use wasm_bindgen::prelude::*;

#[cfg(not(target_arch = "wasm32"))]
use ctor::ctor;

#[cfg(target_arch = "wasm32")]
use web_sys::console;

pub mod config;
mod document;
mod errors;
mod parser;
mod rules;
mod utils;

use crate::config::Config;
use crate::errors::LintError;
use crate::parser::parse;
use crate::rules::RuleContext;
use crate::utils::set_panic_hook;

#[cfg(target_arch = "wasm32")]
use crate::errors::JsLintError;

#[cfg(not(target_arch = "wasm32"))]
#[cfg(not(test))]
#[ctor]
fn init_logger() {
    env_logger::init();
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg(test)]
#[ctor]
fn init_test_logger() {
    env_logger::builder().is_test(true).try_init().unwrap();
}

#[wasm_bindgen]
pub struct Linter {
    config: Config,
}

#[derive(Debug, Serialize, Deserialize, Tsify)]
#[serde(tag = "_type", content = "content")]
#[tsify(from_wasm_abi)]
pub enum LintTarget {
    FileOrDirectory(PathBuf),
    String(String),
}

#[cfg(target_arch = "wasm32")]
#[derive(Debug, Serialize, Deserialize, Tsify)]
#[tsify(from_wasm_abi)]
pub struct JsLintTarget {
    #[serde(rename = "_type")]
    type_: String,
    path: Option<String>,
    text: Option<String>,
}

#[cfg(target_arch = "wasm32")]
impl JsLintTarget {
    fn from_js_value(input: JsValue) -> Result<Self, JsValue> {
        match serde_wasm_bindgen::from_value::<serde_json::Value>(input) {
            Ok(json_value) => match serde_json::from_value::<JsLintTarget>(json_value) {
                Ok(js_target) => Ok(js_target),
                Err(err) => {
                    console::log_1(&JsValue::from_str(&format!(
                        "failed to parse JsLintTarget: {:?}",
                        err
                    )));
                    Err(JsValue::from_str(&format!(
                        "Failed to parse JsLintTarget: {:?}",
                        err
                    )))
                }
            },
            Err(err) => {
                console::log_1(&JsValue::from_str(&format!(
                    "Failed to convert input to serde_json::Value {:?}",
                    err
                )));
                Err(JsValue::from_str(&format!(
                    "Failed to convert input to serde_json::Value {:?}",
                    err
                )))
            }
        }
    }

    fn to_lint_target(self) -> Result<LintTarget, JsValue> {
        match self.type_.as_str() {
            "fileOrDirectory" => {
                match self
                    .path
                    .ok_or_else(|| JsValue::from_str("A file target must have a path"))
                {
                    Ok(path) => Ok(LintTarget::FileOrDirectory(PathBuf::from(&path))),
                    Err(err) => Err(err),
                }
            }
            "string" => {
                match self
                    .text
                    .ok_or_else(|| JsValue::from_str("A string target must have a text"))
                {
                    Ok(text) => Ok(LintTarget::String(text)),
                    Err(err) => Err(err),
                }
            }
            _ => Err(JsValue::from_str(
                "Invalid lint target type. Only 'fileOrDirectory' and 'string' are supported.",
            )),
        }
    }
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
impl Linter {
    #[wasm_bindgen]
    pub fn lint(&self, input: JsValue) -> Result<JsValue, JsValue> {
        let js_target = JsLintTarget::from_js_value(input)?;
        match js_target.to_lint_target() {
            Ok(lint_target) => match self.lint_internal(lint_target, None) {
                Ok(errors) => serde_wasm_bindgen::to_value(
                    &errors
                        .into_iter()
                        .map(|e| Into::<JsLintError>::into(e))
                        .collect::<Vec<_>>(),
                )
                .map_err(|e| JsValue::from_str(&e.to_string())),
                Err(err) => Err(JsValue::from_str(&err.to_string())),
            },
            Err(err) => Err(err),
        }
    }

    #[wasm_bindgen]
    pub fn lint_only_rule(&self, rule_id: JsValue, input: JsValue) -> Result<JsValue, JsValue> {
        let js_target = JsLintTarget::from_js_value(input)?;
        match (js_target.to_lint_target(), rule_id.as_string()) {
            (Ok(lint_target), Some(rule_id)) => {
                match self.lint_internal(lint_target, Some(&[rule_id.as_str()])) {
                    Ok(errors) => serde_wasm_bindgen::to_value(
                        &errors
                            .into_iter()
                            .map(|e| Into::<JsLintError>::into(e))
                            .collect::<Vec<_>>(),
                    )
                    .map_err(|e| JsValue::from_str(&e.to_string())),
                    Err(err) => Err(JsValue::from_str(&err.to_string())),
                }
            }
            (Err(err), _) => Err(err),
            (_, None) => Err(JsValue::from_str(
                "A rule ID must be provided when linting only a single rule",
            )),
        }
    }
}

impl Linter {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn lint(&self, input: LintTarget) -> Result<Vec<LintError>> {
        self.lint_internal(input, None)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn lint_only_rule(&self, rule_id: &str, input: LintTarget) -> Result<Vec<LintError>> {
        self.lint_internal(input, Some(&[rule_id]))
    }

    fn lint_internal(
        &self,
        input: LintTarget,
        check_only_rules: RuleFilter,
    ) -> Result<Vec<LintError>> {
        match input {
            LintTarget::FileOrDirectory(path) => {
                self.lint_file_or_directory(path, check_only_rules)
            }
            LintTarget::String(string) => self.lint_string(&string, check_only_rules),
        }
    }

    fn lint_file_or_directory(
        &self,
        path: PathBuf,
        check_only_rules: RuleFilter,
    ) -> Result<Vec<LintError>> {
        if path.is_file() {
            let mut file = fs::File::open(path)?;
            let mut contents = String::new();
            file.read_to_string(&mut contents)?;
            self.lint_string(&contents, check_only_rules)
        } else if path.is_dir() {
            let collected_vec = fs::read_dir(path)?
                .filter_map(Result::ok)
                .flat_map(|entry| {
                    self.lint_file_or_directory(entry.path(), check_only_rules)
                        .unwrap_or_default()
                })
                .collect::<Vec<_>>();
            Ok(collected_vec)
        } else {
            Err(anyhow::anyhow!(
                "Path is neither a file nor a directory: {:?}",
                path
            ))
        }
    }

    fn lint_string(&self, string: &str, check_only_rules: RuleFilter) -> Result<Vec<LintError>> {
        let parse_result = parse(string)?;
        let rule_context = RuleContext::new(parse_result, check_only_rules);
        self.config.rule_registry.run(&rule_context)
    }
}

#[wasm_bindgen]
pub struct LinterBuilder;
#[wasm_bindgen]
pub struct LinterBuilderWithConfig {
    config: Config,
}

impl Default for LinterBuilder {
    fn default() -> Self {
        LinterBuilder::new()
    }
}

#[wasm_bindgen]
impl LinterBuilder {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        set_panic_hook();
        LinterBuilder
    }

    #[wasm_bindgen]
    #[cfg(target_arch = "wasm32")]
    pub fn configure(self, config: JsValue) -> LinterBuilderWithConfig {
        let settings: toml::Value = serde_wasm_bindgen::from_value(config).unwrap();
        let config = Config::from_serializable(settings).unwrap();
        LinterBuilderWithConfig { config }
    }
}

impl LinterBuilder {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn configure(self, config: Config) -> LinterBuilderWithConfig {
        LinterBuilderWithConfig { config }
    }
}

#[wasm_bindgen]
impl LinterBuilderWithConfig {
    #[wasm_bindgen]
    #[cfg(target_arch = "wasm32")]
    pub fn build(mut self) -> Result<Linter, JsValue> {
        self.config
            .rule_registry
            .setup(&self.config.rule_specific_settings)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;

        Ok(Linter {
            config: self.config,
        })
    }
}

impl LinterBuilderWithConfig {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn build(mut self) -> Result<Linter> {
        self.config
            .rule_registry
            .setup(&self.config.rule_specific_settings)?;

        Ok(Linter {
            config: self.config,
        })
    }
}

#[cfg(test)]
mod tests {
    #[cfg(target_arch = "wasm32")]
    use js_sys::Array;
    #[cfg(target_arch = "wasm32")]
    use serde_json::json;
    #[cfg(target_arch = "wasm32")]
    use serde_wasm_bindgen::to_value;
    #[cfg(target_arch = "wasm32")]
    use wasm_bindgen_test::*;

    use super::*;

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn test_lint_valid_string() -> Result<()> {
        let config = Config::default();
        let linter = LinterBuilder::new().configure(config).build()?;

        let valid_mdx = "# Hello, world!\n\nThis is valid MDX document.";
        let result = linter.lint(LintTarget::String(valid_mdx.to_string()))?;

        assert!(
            result.is_empty(),
            "Expected no lint errors for valid MDX, got {:?}",
            result
        );
        Ok(())
    }

    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen_test]
    fn test_lint_valid_string_from_js() {
        let config = json!({});
        let linter = match LinterBuilder::new()
            .configure(to_value(&config).unwrap())
            .build()
        {
            Ok(linter) => linter,
            Err(err) => panic!("Failed to build linter: {:?}", err),
        };

        let valid_mdx = "# Hello, world!\n\nThis is valid MDX document.";
        let lint_target = json!({"_type": "string", "text": valid_mdx});
        let lint_target_js_value =
            to_value(&lint_target).expect("Failed to convert lint target to JsValue");

        let result = match linter.lint(lint_target_js_value) {
            Ok(result) => result,
            Err(err) => panic!("Failed to lint string: {:?}", err),
        };
        let result = Array::from(&result);

        assert!(
            result.length() == 0,
            "Expected no lint errors for valid MDX, got {:?}",
            result
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn test_lint_invalid_string() -> Result<()> {
        let config = Config::default();
        let linter = LinterBuilder::new().configure(config).build()?;

        let invalid_mdx = "# Incorrect Heading\n\nThis is an invalid MDX document.";
        let result = linter.lint(LintTarget::String(invalid_mdx.to_string()))?;

        assert!(!result.is_empty(), "Expected lint errors for invalid MDX");
        Ok(())
    }

    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen_test]
    fn test_lint_invalid_string_from_js() {
        let config = json!({});
        let linter = match LinterBuilder::new()
            .configure(to_value(&config).unwrap())
            .build()
        {
            Ok(linter) => linter,
            Err(err) => panic!("Failed to build linter: {:?}", err),
        };

        let invalid_mdx = "# Incorrect Heading\n\nThis is an invalid MDX document.";
        let lint_target = json!({"_type": "string", "text": invalid_mdx});
        let lint_target_js_value =
            to_value(&lint_target).expect("Failed to convert lint target to JsValue");

        let result = match linter.lint(lint_target_js_value) {
            Ok(result) => result,
            Err(err) => panic!("Failed to lint string: {:?}", err),
        };
        let result = Array::from(&result);

        assert!(
            result.length() > 0,
            "Expected lint errors for invalid MDX, got {:?}",
            result
        );
    }
}
