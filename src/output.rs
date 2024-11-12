use std::{io::Write, path::PathBuf};

use anyhow::Result;

use crate::errors::LintError;

pub mod rdf;
pub mod simple;

pub struct LintOutput {
    file_path: PathBuf,
    errors: Vec<LintError>,
}

pub trait OutputFormatter {
    fn format<Writer: Write>(&self, output: &[&LintOutput], io: &mut Writer) -> Result<()>;
}
