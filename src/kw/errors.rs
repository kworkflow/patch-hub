use thiserror::Error;

use crate::{
    infrastructure::{file_system::FileSystemError, process::ProcessError, shell::ShellError},
    kw::readiness::KwReadinessError,
};

#[derive(Debug, Error)]
pub enum KwError {
    #[error("kw actor unavailable: {0}")]
    ActorUnavailable(String),
    #[error("no kw job is running")]
    NoJobRunning,
    #[error("no pre-job branch was recorded")]
    NoRecordedBranch,
    #[error("history error: {0}")]
    History(#[from] FileSystemError),
    #[error("readiness error: {0}")]
    Readiness(#[from] KwReadinessError),
    #[error("shell error: {0}")]
    Shell(#[from] ShellError),
}

/// Accept/refuse verdict for `Start*` messages. The reply is always
/// immediate: an accepted job keeps running inside the actor after the
/// caller has been answered.
#[derive(Debug, Error)]
pub enum KwStartError {
    #[error("kw actor unavailable: {0}")]
    ActorUnavailable(String),
    #[error("a kw job is already running")]
    JobAlreadyRunning,
    #[error("kw jobs are not supported yet")]
    NotImplemented,
    #[error("failed to spawn the kw process: {0}")]
    Spawn(#[from] ProcessError),
    #[error("filesystem error: {0}")]
    Fs(#[from] FileSystemError),
}
