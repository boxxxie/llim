use std::path::PathBuf;

use compact_str::CompactString;
use thiserror::Error;

use crate::client::error::ClientError;

pub type Result<T> = std::result::Result<T, LlimError>;

#[derive(Error, Debug, Clone)]
pub enum LlimError {
    #[error("Invalid Linear API key")]
    InvalidLinearToken,

    #[error("Configuration file not found at {path}")]
    ConfigFileNotFound { path: PathBuf },

    #[error("Failed to load config from {path}: {message}")]
    ConfigLoadError {
        path: PathBuf,
        message: CompactString,
    },

    #[error("Failed to save config to {path}: {message}")]
    ConfigSaveError {
        path: PathBuf,
        message: CompactString,
    },

    #[error("Configuration validation error on {field}: {message}")]
    ConfigValidationError {
        field: CompactString,
        message: CompactString,
    },

    #[error("Database error: {0}")]
    DatabaseError(CompactString),

    #[error("Linear client error: {0}")]
    Client(#[from] ClientError),

    #[error("{0}")]
    GeneralError(CompactString),
}

impl LlimError {
    pub fn config_save_error(path: PathBuf, err: impl std::fmt::Display) -> Self {
        Self::ConfigSaveError {
            path,
            message: err.to_string().into(),
        }
    }
}
