use anyhow::Result;
use errors::LintError;
use parser::parse;
use rules::RuleContext;
use std::{fs, io::Read, path::Path};

pub mod config;
mod document;
mod errors;
mod parser;
mod rules;
mod utils;

use crate::config::Config;

pub struct Linter {
    config: Config,
}

pub enum LintTarget<'a> {
    FileOrDirectory(&'a Path),
    String(&'a str),
}

impl Linter {
    pub fn lint(&self, input: LintTarget) -> Result<Vec<LintError>> {
        match input {
            LintTarget::FileOrDirectory(path) => self.lint_file_or_directory(path),
            LintTarget::String(string) => self.lint_string(string),
        }
    }

    fn lint_file_or_directory(&self, path: &Path) -> Result<Vec<LintError>> {
        if path.is_file() {
            let mut file = fs::File::open(path)?;
            let mut contents = String::new();
            file.read_to_string(&mut contents)?;
            self.lint_string(&contents)
        } else if path.is_dir() {
            let collected_vec = fs::read_dir(path)?
                .filter_map(Result::ok)
                .flat_map(|entry| {
                    self.lint_file_or_directory(&entry.path())
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

pub struct LinterBuilder;
pub struct LinterBuilderWithConfig {
    config: Config,
}

impl Default for LinterBuilder {
    fn default() -> Self {
        LinterBuilder::new()
    }
}

impl LinterBuilder {
    pub fn new() -> Self {
        LinterBuilder
    }

    pub fn configure(self, config: Config) -> LinterBuilderWithConfig {
        LinterBuilderWithConfig { config }
    }
}

impl LinterBuilderWithConfig {
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
    use log::debug;

    use super::*;

    #[test]
    fn test_lint_valid_string() -> Result<()> {
        let config = Config::default();
        let linter = LinterBuilder::new().configure(config).build()?;

        let valid_mdx = "# Hello, world!\n\nThis is valid MDX document.";
        let result = linter.lint(LintTarget::String(valid_mdx))?;

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
        let result = linter.lint(LintTarget::String(invalid_mdx))?;

        debug!("Lint errors: {:?}", result);

        assert!(!result.is_empty(), "Expected lint errors for invalid MDX");
        Ok(())
    }
}
