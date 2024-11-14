use anyhow::{Context, Result};
use rules::RuleFilter;
use serde::{Deserialize, Serialize};
use std::env;
use std::path::{Path, PathBuf};
use std::{fs, io::Read};
use tsify::Tsify;
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
use web_sys::console;

mod app_error;
mod config;
mod document;
mod errors;
mod fix;
mod output;
mod parser;
mod rules;
mod utils;

pub use crate::config::Config;
pub use crate::errors::LintLevel;
pub use crate::output::{rdf::RdfFormatter, simple::SimpleFormatter, LintOutput, OutputFormatter};
pub use crate::utils::is_lintable;

use crate::parser::parse;
use crate::rules::RuleContext;
use crate::utils::set_panic_hook;

#[cfg(target_arch = "wasm32")]
use crate::errors::JsLintError;

#[derive(Debug)]
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
            Ok(lint_target) => match self.lint_internal(&lint_target, None) {
                Ok(diagnostics) => serde_wasm_bindgen::to_value(
                    &diagnostics
                        .iter()
                        .flat_map(|diagnostic| {
                            diagnostic.errors().iter().map(Into::<JsLintError>::into)
                        })
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
                match self.lint_internal(&lint_target, Some(&[rule_id.as_str()])) {
                    Ok(diagnostics) => serde_wasm_bindgen::to_value(
                        &diagnostics
                            .iter()
                            .flat_map(|diagnostic| {
                                diagnostic.errors().iter().map(Into::<JsLintError>::into)
                            })
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

struct LintSourceReference<'reference>(Option<&'reference Path>);

impl Linter {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn lint(&self, input: &LintTarget) -> Result<Vec<LintOutput>> {
        self.lint_internal(input, None)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn lint_only_rule(&self, rule_id: &str, input: &LintTarget) -> Result<Vec<LintOutput>> {
        self.lint_internal(input, Some(&[rule_id]))
    }

    fn lint_internal(
        &self,
        input: &LintTarget,
        check_only_rules: RuleFilter,
    ) -> Result<Vec<LintOutput>> {
        match input {
            LintTarget::FileOrDirectory(path) => {
                self.lint_file_or_directory(path, check_only_rules)
            }
            LintTarget::String(string) => {
                self.lint_string(string, LintSourceReference(None), check_only_rules)
            }
        }
    }

    fn lint_file_or_directory(
        &self,
        path: &PathBuf,
        check_only_rules: RuleFilter,
    ) -> Result<Vec<LintOutput>> {
        if path.is_file() {
            if self.config.is_ignored(path) {
                return Ok(Vec::new());
            }

            let mut file = fs::File::open(path)?;
            let mut contents = String::new();
            file.read_to_string(&mut contents)?;
            self.lint_string(&contents, LintSourceReference(Some(path)), check_only_rules)
        } else if path.is_dir() {
            let collected_vec = fs::read_dir(path)?
                .filter_map(Result::ok)
                .filter(|dir_entry| is_lintable(dir_entry.path()))
                .flat_map(|entry| {
                    self.lint_file_or_directory(&entry.path(), check_only_rules)
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

    fn lint_string(
        &self,
        string: &str,
        source: LintSourceReference,
        check_only_rules: RuleFilter,
    ) -> Result<Vec<LintOutput>> {
        let parse_result = parse(string)?;
        let rule_context = RuleContext::new(parse_result, check_only_rules)?;
        match self.config.rule_registry.run(&rule_context) {
            Ok(diagnostics) => {
                let source = match source.0 {
                    Some(path) => {
                        let current_dir =
                            env::current_dir().context("Failed to get current directory")?;
                        let relative_path = match path.strip_prefix(&current_dir) {
                            Ok(relative_path) => relative_path,
                            Err(_) => path,
                        };
                        &relative_path.to_string_lossy()
                    }
                    None => "[direct input]",
                };
                Ok(vec![LintOutput::new(source, diagnostics)])
            }
            Err(err) => Err(err),
        }
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
        use config::ConfigDir;

        let settings: toml::Value = serde_wasm_bindgen::from_value(config).unwrap();
        let config = Config::from_serializable(settings, &ConfigDir(None)).unwrap();
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
    use super::*;

    use ctor::ctor;

    #[ctor]
    fn init_test_logger() {
        env_logger::builder().is_test(true).try_init().unwrap();
    }

    #[test]
    fn test_lint_valid_string() -> Result<()> {
        let config = Config::default();
        let linter = LinterBuilder::new().configure(config).build()?;

        let valid_mdx = "# Hello, world!\n\nThis is valid MDX document.";
        let result = linter.lint(&LintTarget::String(valid_mdx.to_string()))?;

        assert!(
            result.get(0).unwrap().errors().is_empty(),
            "Expected no lint errors for valid MDX, got {:?}",
            result
        );
        Ok(())
    }

    #[test]
    fn test_lint_invalid_string() -> Result<()> {
        let config = Config::default();
        let linter = LinterBuilder::new().configure(config).build()?;

        let invalid_mdx = "# Incorrect Heading\n\nThis is an invalid MDX document.";
        let result = linter.lint(&LintTarget::String(invalid_mdx.to_string()))?;

        assert!(
            !result.get(0).unwrap().errors().is_empty(),
            "Expected lint errors for invalid MDX"
        );
        Ok(())
    }
}
