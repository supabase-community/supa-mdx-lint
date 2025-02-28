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
    #[error("Unmatched pair: {0}: [{1}, {2}]")]
    UnmatchedPair(
        String,
        /// Start row (1-indexed)
        usize,
        /// Start column (1-indexed)
        usize,
    ),
    #[error("Parse error: Variant not found: {0}")]
    VariantNotFound(String),
}

#[derive(Error, Debug, Default)]
pub(crate) struct MultiError(Vec<Box<dyn std::error::Error>>);

impl MultiError {
    pub(crate) fn add_error(&mut self, error: Box<dyn std::error::Error>) {
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

#[derive(Debug)]
#[must_use = "The result may contain an error, which it is recommended to check"]
pub(crate) struct ResultBoth<T, E: std::error::Error> {
    res: Option<T>,
    err: Option<E>,
}

impl<T, E: std::error::Error> ResultBoth<T, E> {
    pub(crate) fn new() -> Self {
        Self {
            res: None,
            err: None,
        }
    }

    pub(crate) fn has_result(&self) -> bool {
        self.res.is_some()
    }

    pub(crate) fn has_err(&self) -> bool {
        self.err.is_some()
    }

    pub(crate) fn result(&self) -> Option<&T> {
        self.res.as_ref()
    }

    pub(crate) fn set_result(mut self, result: T) -> Self {
        self.res = Some(result);
        self
    }

    pub(crate) fn err(&self) -> Option<&E> {
        self.err.as_ref()
    }

    pub(crate) fn set_err(mut self, err: E) -> Self {
        self.err = Some(err);
        self
    }

    pub(crate) fn set_maybe_err(mut self, err: Option<E>) -> Self {
        self.err = err;
        self
    }
}
