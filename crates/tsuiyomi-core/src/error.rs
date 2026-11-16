use std::{io, path::PathBuf};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("failed to read directory {path}: {source}")]
    ReadDir {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("failed to read {path}: {source}")]
    ReadFile {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("failed to parse {path}: {source}")]
    ParseJson {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    #[error("unsupported archive format: {path}")]
    UnsupportedArchive { path: PathBuf },

    #[error("failed to read archive {path}: {message}")]
    Archive { path: PathBuf, message: String },

    #[error("missing page {name} in {path}")]
    MissingPage { path: PathBuf, name: String },
}
