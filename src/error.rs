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

    #[error("Unexpected EOF at position {position}: needed {needed} bytes, {available} available")]
    UnexpectedEofDetail {
        position: usize,
        needed: usize,
        available: usize,
    },

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

    #[error("Lazy payload truncated: expected {expected} bytes, got {actual}")]
    TruncatedLazyPayload { expected: u64, actual: u64 },

    #[error("Cannot write lazy object; materialize first or use full parse mode")]
    CannotWriteLazyObject,

    #[error("JavaScript callback failed: {0}")]
    CallbackFailed(String),

    #[error("Memory budget exhausted: needed {needed} bytes, {available} available")]
    MemoryBudgetExceeded { needed: usize, available: usize },
}
