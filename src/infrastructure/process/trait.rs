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
#[automock]
#[async_trait]
pub trait RunningProcess: Send {
    /// Resolve once the process exits, without blocking a worker thread.
    async fn wait(&mut self) -> Result<ExitStatus, ProcessError>;

    /// Send SIGTERM to the whole process group, not just the direct child.
    /// An already-gone group (ESRCH) is success. A leaderless group is
    /// signaled on purpose so grandchildren die, but a recycled pgid can
    /// hit an unrelated group. Both methods take `&mut self`: `select!`
    /// between `wait()` and cancel, then `kill()`.
    fn kill(&mut self) -> Result<(), ProcessError>;

    /// Send SIGKILL to the whole process group. The escalation rung for a
    /// group that ignored `kill()`'s SIGTERM; same ESRCH tolerance.
    fn force_kill(&mut self) -> Result<(), ProcessError>;
}
