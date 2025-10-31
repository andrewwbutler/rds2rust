//! Error types for RDS parsing and writing.

use thiserror::Error;

/// Result type alias using our Error type.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors that can occur during RDS parsing or writing.
#[derive(Error, Debug)]
pub enum Error {
    #[error("Invalid RDS format: {0}")]
    InvalidFormat(String),

    #[error("Unsupported RDS version: {0}")]
    UnsupportedVersion(u32),

    #[error("Unexpected end of file")]
    UnexpectedEof,

    #[error("Unknown SEXP type: {0}")]
    UnknownSexpType(u32),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("UTF-8 error: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),

    #[error("Invalid reference index: {0}")]
    InvalidReference(usize),

    #[error("Unsupported feature: {0}")]
    Unsupported(String),
}
