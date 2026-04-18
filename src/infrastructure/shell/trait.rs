use mockall::automock;
use thiserror::Error;

use std::io;

#[derive(Debug, Error)]
pub enum ShellError {
    #[error("{0}")]
    IoError(#[from] io::Error),
}

#[derive(Debug)]
pub struct ShellCommand {
    pub program: String,
    pub args: Vec<String>,
}

impl ShellCommand {
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
        }
    }

    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }
}

pub struct ShellOutput {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub success: bool,
}

#[automock]
pub trait ShellTrait: Send + Sync {
    /// Execute a command, capturing stdout and stderr.
    fn execute(&self, cmd: &ShellCommand) -> Result<ShellOutput, ShellError>;

    /// Execute a command, writing `stdin` to the process's standard input and
    /// capturing stdout and stderr.
    fn execute_with_stdin(
        &self,
        cmd: &ShellCommand,
        stdin: &[u8],
    ) -> Result<ShellOutput, ShellError>;

    /// Spawn an interactive subprocess that inherits the current process's
    /// stdio (terminal passthrough). Returns `true` if the child exited
    /// successfully.
    fn spawn_interactive(&self, cmd: &ShellCommand) -> Result<bool, ShellError>;
}
