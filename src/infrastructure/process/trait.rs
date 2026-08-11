use async_trait::async_trait;
use mockall::automock;
use thiserror::Error;

use std::{io, path::Path, process::ExitStatus};

use crate::infrastructure::shell::ShellCommand;

#[derive(Debug, Error)]
pub enum ProcessError {
    #[error("{0}")]
    IoError(#[from] io::Error),
}

// The two traits in this module have no production caller until the kw
// integration wires KwActor; kept per the CachePolicy precedent
// (src/lore/application/cache.rs).
#[allow(dead_code)]
#[automock]
pub trait ProcessTrait: Send + Sync {
    /// Spawn `cmd` with `cwd` as its working directory, redirecting stdout and
    /// stderr to `log_path` (created/truncated; the parent directory must
    /// already exist). Returns immediately with a handle to the still-running
    /// process, which is spawned as a process-group leader.
    fn spawn(
        &self,
        cmd: &ShellCommand,
        cwd: &Path,
        log_path: &Path,
    ) -> Result<Box<dyn RunningProcess>, ProcessError>;
}

// `automock` must stay the outermost attribute: with `async_trait` listed
// first, the generated mock's async methods return an unusable type.
#[allow(dead_code)]
#[automock]
#[async_trait]
pub trait RunningProcess: Send {
    /// Resolve once the process exits, without blocking a worker thread.
    async fn wait(&mut self) -> Result<ExitStatus, ProcessError>;

    /// Send SIGTERM to the whole process group, not just the direct child.
    /// Idempotent: an already-gone group (ESRCH) is reported as success.
    ///
    /// Both methods take `&mut self`: a consumer that waits while staying able
    /// to cancel should `tokio::select!` between `wait()` and its cancel
    /// signal, then call `kill()`.
    fn kill(&mut self) -> Result<(), ProcessError>;
}
