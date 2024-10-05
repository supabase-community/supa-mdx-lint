use std::path::PathBuf;

use anyhow::Result;
use errors::LintError;
use parser::parse;

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
    FileOrDirectory(PathBuf),
    String(&'a str),
}

impl Linter {
    pub fn lint(&self, input: LintTarget) -> Result<Vec<LintError>> {
        match input {
            LintTarget::FileOrDirectory(path) => {
                todo!()
            }
            LintTarget::String(string) => self.lint_string(string),
        }
    }

    fn lint_string(&self, string: &str) -> Result<Vec<LintError>> {
        let ast = parse(string)?;
        todo!()
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
    pub fn build(self) -> Linter {
        Linter {
            config: self.config,
        }
    }
}

#[cfg(test)]
use ctor::ctor;

#[cfg(test)]
#[ctor]
fn init_test_logger() {
    env_logger::builder().is_test(true).try_init().unwrap();
}
