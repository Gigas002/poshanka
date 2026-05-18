use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("failed to read {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("invalid config: {0}")]
    Config(String),

    #[error("invalid theme: {0}")]
    Theme(String),

    #[error("invalid color in theme: {0}")]
    InvalidHexRgba(#[from] libposhanka::ParseHexRgbaError),

    #[error("missing required field `{0}` in config [base]")]
    MissingBaseField(&'static str),
}
