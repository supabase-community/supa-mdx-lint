use anyhow::{Context as _, Result};
use bon::bon;
use context::Context;
use rules::RuleFilter;
use std::env;
use std::path::{Path, PathBuf};
use std::{fs, io::Read};

use crate::output::LintOutput;
use crate::parser::parse;

mod app_error;
mod comments;
mod config;
mod context;
mod errors;
mod geometry;
mod parser;
mod utils;

pub mod fix;
pub mod output;
#[doc(hidden)]
pub mod rope;
pub mod rules;

#[doc(inline)]
pub use crate::config::{Config, ConfigDir};
#[doc(inline)]
pub use crate::errors::{LintError, LintLevel};

#[derive(Debug)]
pub struct Linter {
    config: Config,
}

#[derive(Debug)]
pub enum LintTarget<'a> {
    FileOrDirectory(PathBuf),
    String(&'a str),
}

struct LintSourceReference<'reference>(Option<&'reference Path>);

#[bon]
impl Linter {
    #[builder]
    pub fn new(config: Option<Config>) -> Result<Self> {
        let mut this = Self {
            config: config.unwrap_or_default(),
        };

        this.config
            .rule_registry
            .setup(&mut this.config.rule_specific_settings)?;

        Ok(this)
    }

    pub fn is_lintable(&self, path: impl AsRef<Path>) -> bool {
        self.config.is_lintable(path)
    }

    pub fn is_ignored(&self, path: impl AsRef<Path>) -> bool {
        self.config.is_ignored(path)
    }

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
                .filter(|dir_entry| self.is_lintable(dir_entry.path()))
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
        let rule_context = Context::builder()
            .parse_result(&parse_result)
            .maybe_check_only_rules(check_only_rules)
            .build()?;
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

#[doc(hidden)]
pub mod internal {
    pub use crate::geometry::Offsets;
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
    fn public_api() {
        // Install a compatible nightly toolchain if it is missing
        rustup_toolchain::install(public_api::MINIMUM_NIGHTLY_RUST_VERSION).unwrap();

        // Build rustdoc JSON
        let rustdoc_json = rustdoc_json::Builder::default()
            .toolchain(public_api::MINIMUM_NIGHTLY_RUST_VERSION)
            .build()
            .unwrap();

        // Derive the public API from the rustdoc JSON
        let public_api = public_api::Builder::from_rustdoc_json(rustdoc_json)
            .build()
            .unwrap();

        // Assert that the public API looks correct
        insta::assert_snapshot!(public_api);
    }

    #[test]
    fn test_lint_valid_string() -> Result<()> {
        let mut linter = Linter::builder().build()?;
        linter
            .config
            .rule_registry
            .deactivate_all_but("Rule001HeadingCase");

        let valid_mdx = "# Hello, world!\n\nThis is a valid document.";
        let result = linter.lint(&LintTarget::String(&valid_mdx.to_string()))?;

        assert!(
            result.get(0).unwrap().errors().is_empty(),
            "Expected no lint errors for valid MDX, got {:?}",
            result
        );
        Ok(())
    }

    #[test]
    fn test_lint_invalid_string() -> Result<()> {
        let mut linter = Linter::builder().build()?;
        linter
            .config
            .rule_registry
            .deactivate_all_but("Rule001HeadingCase");

        let invalid_mdx = "# Incorrect Heading\n\nThis is an invalid document.";
        let result = linter.lint(&LintTarget::String(&invalid_mdx.to_string()))?;

        assert!(
            !result.get(0).unwrap().errors().is_empty(),
            "Expected lint errors for invalid MDX"
        );
        Ok(())
    }
}
