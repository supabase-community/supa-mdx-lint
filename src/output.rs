use std::io::Write;

use anyhow::Result;

use crate::errors::LintError;

pub mod rdf;
pub mod simple;

#[derive(Debug)]
pub struct LintOutput {
    file_path: String,
    errors: Vec<LintError>,
}

impl LintOutput {
    pub fn new(file_path: impl AsRef<str>, errors: Vec<LintError>) -> Self {
        Self {
            file_path: file_path.as_ref().to_string(),
            errors,
        }
    }

    pub fn errors(&self) -> &[LintError] {
        &self.errors
    }
}

pub trait OutputFormatter {
    fn format<Writer: Write>(&self, output: &[LintOutput], io: &mut Writer) -> Result<()>;
}
