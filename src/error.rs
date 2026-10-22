use serde::Serialize;

/// Errors returned by this crate.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid save header: {0}")]
    InvalidHeader(String),
    #[error("decompression failed: {0}")]
    Decompress(String),
    #[error("GVAS error at offset {offset}: {msg}")]
    Gvas { offset: usize, msg: String },
    #[error("http error: {0}")]
    Http(String),
    #[error("unauthorized")]
    Unauthorized,
    #[error("unexpected http status {0}")]
    Status(u16),
    #[error("invalid id: {0}")]
    InvalidId(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// A non-fatal problem found while parsing.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ParseWarning {
    pub path: String,
    pub message: String,
}
