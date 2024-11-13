use std::{io::Write, str::FromStr};

use anyhow::Result;

use crate::{app_error, errors::LintError};

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

    pub fn file_path(&self) -> &str {
        &self.file_path
    }

    pub fn errors(&self) -> &[LintError] {
        &self.errors
    }
}

#[derive(Debug, Clone)]
pub enum OutputFormatter {
    Simple(simple::SimpleFormatter),
    Rdf(rdf::RdfFormatter),
}

impl OutputFormatter {
    pub fn format<Writer: Write>(&self, output: &[LintOutput], io: &mut Writer) -> Result<()> {
        match self {
            Self::Simple(formatter) => formatter.format(output, io),
            Self::Rdf(formatter) => formatter.format(output, io),
        }
    }
}

impl FromStr for OutputFormatter {
    type Err = app_error::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "simple" => Ok(Self::Simple(simple::SimpleFormatter)),
            "rdf" => Ok(Self::Rdf(rdf::RdfFormatter)),
            _ => Err(app_error::ParseError::VariantNotFound),
        }
    }
}
