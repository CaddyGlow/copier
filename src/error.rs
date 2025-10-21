use std::io;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, CopierError>;

#[derive(Debug, Error)]
pub enum CopierError {
    #[error("invalid utf-8 path: {0}")]
    InvalidUtfPath(String),

    #[error("io error: {0}")]
    Io(#[from] io::Error),

    #[error("configuration error: {0}")]
    Config(String),

    #[error("failed to parse config: {0}")]
    ConfigParse(String),

    #[error("invalid argument: {0}")]
    InvalidArgument(String),

    #[error("markdown parse error: {0}")]
    Markdown(String),

    #[error("operation aborted: {0}")]
    Aborted(String),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
