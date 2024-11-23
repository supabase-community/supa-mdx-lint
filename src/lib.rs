use anyhow::{Context, Result};
use rules::RuleFilter;
use std::env;
use std::path::{Path, PathBuf};
use std::{fs, io::Read};

mod app_error;
mod config;
mod errors;
mod fix;
mod geometry;
mod output;
mod parser;
mod rope;
mod rules;
mod utils;

pub use crate::config::Config;
pub use crate::errors::LintLevel;
pub use crate::output::{rdf::RdfFormatter, simple::SimpleFormatter, LintOutput, OutputFormatter};
pub use crate::utils::is_lintable;

use crate::parser::parse;
use crate::rules::RuleContext;

#[derive(Debug)]
pub struct Linter {
    config: Config,
}

#[derive(Debug)]
pub enum LintTarget {
    FileOrDirectory(PathBuf),
    String(String),
}

struct LintSourceReference<'reference>(Option<&'reference Path>);

impl Linter {
    pub fn lint(&self, input: &LintTarget) -> Result<Vec<LintOutput>> {
        self.lint_internal(input, None)
    }

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

pub struct LinterBuilder;
pub struct LinterBuilderWithConfig {
    config: Config,
}

impl LinterBuilder {
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
        let linter = LinterBuilder.configure(config).build()?;

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
        let linter = LinterBuilder.configure(config).build()?;

        let invalid_mdx = "# Incorrect Heading\n\nThis is an invalid MDX document.";
        let result = linter.lint(&LintTarget::String(invalid_mdx.to_string()))?;

        assert!(
            !result.get(0).unwrap().errors().is_empty(),
            "Expected lint errors for invalid MDX"
        );
        Ok(())
    }
}
