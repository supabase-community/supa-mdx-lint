use std::{io::Write, str::FromStr};

use anyhow::Result;

use crate::{app_error, errors::LintError};

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

#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum OutputFormatter {
    Markdown(markdown::MarkdownFormatter),
    #[cfg(feature = "pretty")]
    Pretty(pretty::PrettyFormatter),
    Simple(simple::SimpleFormatter),
    Rdf(rdf::RdfFormatter),
}

impl OutputFormatter {
    pub fn format<Writer: Write>(&self, output: &[LintOutput], io: &mut Writer) -> Result<()> {
        match self {
            Self::Markdown(formatter) => formatter.format(output, io),
            #[cfg(feature = "pretty")]
            Self::Pretty(formatter) => formatter.format(output, io),
            Self::Simple(formatter) => formatter.format(output, io),
            Self::Rdf(formatter) => formatter.format(output, io),
        }
    }

    pub fn should_log_metadata(&self) -> bool {
        match self {
            Self::Markdown(formatter) => formatter.should_log_metadata(),
            #[cfg(feature = "pretty")]
            Self::Pretty(formatter) => formatter.should_log_metadata(),
            Self::Simple(formatter) => formatter.should_log_metadata(),
            Self::Rdf(formatter) => formatter.should_log_metadata(),
        }
    }
}

impl FromStr for OutputFormatter {
    type Err = app_error::PublicError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            #[cfg(feature = "pretty")]
            "pretty" => Ok(Self::Pretty(pretty::PrettyFormatter)),
            "simple" => Ok(Self::Simple(simple::SimpleFormatter)),
            "rdf" => Ok(Self::Rdf(rdf::RdfFormatter)),
            other => Err(app_error::PublicError::VariantNotFound(other.to_string())),
        }
    }
}
