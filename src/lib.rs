use anyhow::Result;
use std::path::PathBuf;
use std::{fs, io::Read};
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
use serde::{Deserialize, Serialize};
#[cfg(target_arch = "wasm32")]
use tsify::Tsify;

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
use errors::JsLintError;

#[wasm_bindgen]
pub struct Linter {
    config: Config,
}

#[cfg(target_arch = "wasm32")]
#[derive(Serialize, Deserialize, Tsify)]
#[tsify(from_wasm_abi)]
pub struct JsLintTarget {
    _type: String,
    path: Option<String>,
    text: Option<String>,
}

#[cfg(target_arch = "wasm32")]
impl JsLintTarget {
    fn to_lint_target(self) -> Result<LintTarget, JsValue> {
        match self._type.as_str() {
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

pub enum LintTarget {
    FileOrDirectory(PathBuf),
    String(String),
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
impl Linter {
    #[wasm_bindgen]
    pub fn lint(&self, input: JsValue) -> Result<JsValue, JsValue> {
        let js_target: JsLintTarget = serde_wasm_bindgen::from_value(input).unwrap();
        match js_target.to_lint_target() {
            Ok(lint_target) => match self.lint_internal(lint_target) {
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
}

impl Linter {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn lint(&self, input: LintTarget) -> Result<Vec<LintError>> {
        self.lint_internal(input)
    }

    pub fn lint_internal(&self, input: LintTarget) -> Result<Vec<LintError>> {
        match input {
            LintTarget::FileOrDirectory(path) => self.lint_file_or_directory(path),
            LintTarget::String(string) => self.lint_string(&string),
        }
    }

    fn lint_file_or_directory(&self, path: PathBuf) -> Result<Vec<LintError>> {
        if path.is_file() {
            let mut file = fs::File::open(path)?;
            let mut contents = String::new();
            file.read_to_string(&mut contents)?;
            self.lint_string(&contents)
        } else if path.is_dir() {
            let collected_vec = fs::read_dir(path)?
                .filter_map(Result::ok)
                .flat_map(|entry| {
                    self.lint_file_or_directory(entry.path())
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

    fn lint_string(&self, string: &str) -> Result<Vec<LintError>> {
        let parse_result = parse(string)?;
        let rule_context = RuleContext::new(parse_result);
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
use ctor::ctor;

#[cfg(test)]
#[ctor]
fn init_test_logger() {
    env_logger::builder().is_test(true).try_init().unwrap();
}

#[cfg(test)]
mod tests {

    use super::*;

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

    #[test]
    fn test_lint_invalid_string() -> Result<()> {
        let config = Config::default();
        let linter = LinterBuilder::new().configure(config).build()?;

        let invalid_mdx = "# Incorrect Heading\n\nThis is an invalid MDX document.";
        let result = linter.lint(LintTarget::String(invalid_mdx.to_string()))?;

        assert!(!result.is_empty(), "Expected lint errors for invalid MDX");
        Ok(())
    }
}
