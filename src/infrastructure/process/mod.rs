//! Unix-only process spawning: kw itself is Linux-only, and process-group
//! semantics (`process_group`/`killpg`) have no portable equivalent.

mod r#trait;

pub use r#trait::{ProcessError, ProcessTrait, RunningProcess};

#[cfg(test)]
pub use r#trait::MockRunningProcess;

#[cfg(test)]
mod tests;

use std::{
    fs::File,
    io,
    path::Path,
    process::{ExitStatus, Stdio},
};

use async_trait::async_trait;
use nix::{
    errno::Errno,
    sys::signal::{killpg, Signal},
    unistd::Pid,
};
use tokio::process::{Child, Command};

use super::shell::ShellCommand;

// No production caller exists until the kw integration wires KwActor;
// kept per the CachePolicy precedent (src/lore/application/cache.rs).
#[allow(dead_code)]
pub struct OsProcess;

impl ProcessTrait for OsProcess {
    fn spawn(
        &self,
        cmd: &ShellCommand,
        cwd: &Path,
        log_path: &Path,
    ) -> Result<Box<dyn RunningProcess>, ProcessError> {
        let log_out = File::create(log_path)?;
        let log_err = log_out.try_clone()?;

        // stdin is null so a prompt from the child gets EOF and fails fast
        // instead of hanging on a TTY owned by the TUI.
        let child = Command::new(&cmd.program)
            .args(&cmd.args)
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::from(log_out))
            .stderr(Stdio::from(log_err))
            .process_group(0)
            .spawn()?;

        let pid = child
            .id()
            .ok_or_else(|| ProcessError::IoError(io::Error::other("spawned child has no pid")))?;

        Ok(Box::new(OsRunningProcess {
            child,
            pgid: Pid::from_raw(pid as i32),
            reaped: false,
        }))
    }
}

struct OsRunningProcess {
    child: Child,
    pgid: Pid,
    reaped: bool,
}

#[async_trait]
impl RunningProcess for OsRunningProcess {
    async fn wait(&mut self) -> Result<ExitStatus, ProcessError> {
        let status = self.child.wait().await?;
        self.reaped = true;
        Ok(status)
    }

    fn kill(&mut self) -> Result<(), ProcessError> {
        match killpg(self.pgid, Signal::SIGTERM) {
            Ok(()) | Err(Errno::ESRCH) => Ok(()),
            Err(errno) => Err(ProcessError::IoError(io::Error::from(errno))),
        }
    }
}

impl Drop for OsRunningProcess {
    fn drop(&mut self) {
        // A handle dropped without wait() (panic, aborted task) must not leave
        // the job's process group running.
        if !self.reaped {
            let _ = killpg(self.pgid, Signal::SIGTERM);
        }
    }
}
