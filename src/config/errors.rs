use thiserror::Error;

use crate::infrastructure::file_system::FileSystemError;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[allow(dead_code)]
    #[error("config actor unavailable: {0}")]
    ActorUnavailable(String),
    #[error("failed to save config: {0}")]
    Save(String),
    #[error("invalid page size: {0}")]
    InvalidPageSize(String),
    #[error("invalid directory: {0}")]
    InvalidDirectory(String),
    #[error("invalid patch renderer: {0}")]
    InvalidPatchRenderer(String),
    #[error("invalid cover renderer: {0}")]
    InvalidCoverRenderer(String),
    #[error("invalid max log age: {0}")]
    InvalidMaxLogAge(String),
    #[error("filesystem error: {0}")]
    Fs(#[from] FileSystemError),
}
