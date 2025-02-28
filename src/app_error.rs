use std::fmt::Display;

use thiserror::Error;

#[derive(Error, Debug)]
pub(crate) enum AppError {
    #[error("File system error encountered when {0}: {1}")]
    FileSystemError(String, #[source] std::io::Error),
}

#[derive(Error, Debug)]
pub(crate) enum ParseError {
    #[error("Lint time configuration comments must have a rule: found only \"{0}\"")]
    ConfigurationCommentMissingRule(String),
    #[error("Position is required, but underlying node has no position: {0}")]
    MissingPosition(String),
    #[error("Unmatched configuration pair - {0}: [Row {1}]")]
    UnmatchedConfigurationPair(
        String,
        /// Start row (1-indexed)
        usize,
    ),
}

#[derive(Error, Debug)]
pub enum PublicError {
    #[error("Variant not found: {0}")]
    VariantNotFound(String),
}

#[derive(Error, Debug, Default)]
pub(crate) struct MultiError(Vec<Box<dyn std::error::Error>>);

impl MultiError {
    pub(crate) fn add_err(&mut self, error: Box<dyn std::error::Error>) {
        self.0.push(error);
    }
}

impl Display for MultiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, err) in self.0.iter().enumerate() {
            writeln!(f, "\nError {} of {}: {}", i + 1, self.0.len(), err)?;
        }
        Ok(())
    }
}

#[derive(Debug, Default)]
#[must_use = "The result may contain an error, which it is recommended to check"]
pub(crate) struct ResultBoth<T, E: std::error::Error> {
    res: T,
    err: Option<E>,
}

impl<T, E: std::error::Error> ResultBoth<T, E> {
    pub(crate) fn new(res: T, err: Option<E>) -> Self {
        Self { res, err }
    }

    #[allow(unused)]
    pub(crate) fn has_err(&self) -> bool {
        self.err.is_some()
    }

    pub(crate) fn split(self) -> (T, Option<E>) {
        (self.res, self.err)
    }

    pub(crate) fn unwrap(self) -> T {
        self.res
    }
}
