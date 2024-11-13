use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("File system error encountered when {0}: {1}")]
    FileSystemError(String, #[source] std::io::Error),
}

#[derive(Error, Debug)]
pub enum ParseError {
    #[error("Parse error: Variant not found.")]
    VariantNotFound,
}
