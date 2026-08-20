// The actor that constructs/reads these is unix-only.
#![cfg_attr(not(unix), allow(dead_code))]

use thiserror::Error;

#[cfg(unix)]
use crate::infrastructure::process::ProcessError;
use crate::{
    infrastructure::{file_system::FileSystemError, shell::ShellError},
    kw::readiness::{KwReadinessError, TreeReadiness},
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
    #[error("kw binary not found on PATH; install kw and make sure it is on PATH")]
    KwBinaryMissing,
    #[error("the kernel tree is not ready: {0}")]
    TreeNotReady(TreeReadiness),
    #[error("could not resolve the kw env state: {0}")]
    Readiness(#[from] KwReadinessError),
    #[error("the kernel tree has staged and/or unstaged changes; commit or stash them first")]
    DirtyWorktree,
    #[error("could not verify the kernel tree's git state: {0}")]
    GitStateProbe(String),
    #[error("failed to switch the kernel tree to the requested branch: {0}")]
    CheckoutFailed(String),
    #[error("kw jobs are not supported yet")]
    NotImplemented,
    // Spawning a process is unix-only (ProcessTrait is cfg(unix)).
    #[cfg(unix)]
    #[error("failed to spawn the kw process: {0}")]
    Spawn(#[from] ProcessError),
    #[error("filesystem error: {0}")]
    Fs(#[from] FileSystemError),
}
