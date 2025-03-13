use std::{collections::HashSet, str::FromStr};

use anyhow::Result;

use crate::{app_error::PublicError, errors::LintError, LintLevel};

pub mod markdown;
#[cfg(feature = "pretty")]
pub mod pretty;
pub mod rdf;
pub mod simple;

#[derive(Debug)]
pub struct LintOutput {
    file_path: String,
    errors: Vec<LintError>,
}

impl LintOutput {
    pub(crate) fn new(file_path: impl AsRef<str>, errors: Vec<LintError>) -> Self {
        Self {
            file_path: file_path.as_ref().to_string(),
            errors,
        }
    }

    pub fn file_path(&self) -> &str {
        &self.file_path
    }

    pub fn errors(&self) -> &[LintError] {
        &self.errors
    }
}

pub struct OutputSummary {
    pub num_files: usize,
    pub num_warnings: usize,
    pub num_errors: usize,
}

pub trait OutputFormatter: Send + Sync + std::fmt::Debug {
    fn id(&self) -> &'static str;
    fn format(&self, output: &[LintOutput]) -> Result<String>;
    fn should_log_metadata(&self) -> bool;

    fn get_summary(&self, output: &[LintOutput]) -> OutputSummary {
        let mut seen_files = HashSet::<&str>::new();
        let mut num_errors = 0;
        let mut num_warnings = 0;

        for o in output {
            seen_files.insert(&o.file_path);
            for error in &o.errors {
                match error.level {
                    LintLevel::Error => num_errors += 1,
                    LintLevel::Warning => num_warnings += 1,
                }
            }
        }

        OutputSummary {
            num_files: seen_files.len(),
            num_warnings,
            num_errors,
        }
    }
}

#[doc(hidden)]
pub mod internal {
    //! Contains internal implementations that are needed for the supa-mdx-lint
    //! binary. Should **not** be used by library users as API stability is
    //! not guaranteed.

    use super::*;

    #[derive(Debug)]
    pub struct NativeOutputFormatter(Box<dyn OutputFormatter>);

    impl Clone for NativeOutputFormatter {
        fn clone(&self) -> Self {
            // Clone is required for clap parsing.
            //
            // These are zero-sized types with no state information, so
            // cloning by recreating (a) is efficient and (b) will not cause
            // any unexpected logic errors.
            match self.0.id() {
                "markdown" => Self(Box::new(markdown::MarkdownFormatter)),
                #[cfg(feature = "pretty")]
                "pretty" => Self(Box::new(pretty::PrettyFormatter)),
                "rdf" => Self(Box::new(rdf::RdfFormatter)),
                "simple" => Self(Box::new(simple::SimpleFormatter)),
                _ => panic!("NativeOutputFormatter should only be used to wrap the native output formats, not a user-provided custom format"),
            }
        }
    }

    impl std::ops::Deref for NativeOutputFormatter {
        type Target = Box<dyn OutputFormatter>;

        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    impl std::ops::DerefMut for NativeOutputFormatter {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }

    impl FromStr for NativeOutputFormatter {
        type Err = PublicError;

        fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
            match s {
                "markdown" => Ok(NativeOutputFormatter(Box::new(markdown::MarkdownFormatter))),
                #[cfg(feature = "pretty")]
                "pretty" => Ok(NativeOutputFormatter(Box::new(pretty::PrettyFormatter))),
                "rdf" => Ok(NativeOutputFormatter(Box::new(rdf::RdfFormatter))),
                "simple" => Ok(NativeOutputFormatter(Box::new(simple::SimpleFormatter))),
                s => Err(PublicError::VariantNotFound(s.to_string())),
            }
        }
    }
}
