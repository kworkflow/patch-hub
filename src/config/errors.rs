use thiserror::Error;

use crate::infrastructure::file_system::FileSystemError;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("config actor unavailable: {0}")]
    ActorUnavailable(String),
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
    #[error("invalid stay-on-applied-branch value: {0}")]
    InvalidStayOnAppliedBranch(String),
    #[error("filesystem error: {0}")]
    Fs(#[from] FileSystemError),
}
