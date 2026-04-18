mod r#trait;

pub use r#trait::{ShellCommand, ShellError, ShellOutput, ShellTrait};

use std::{
    io::Write,
    process::{Command, Stdio},
};

#[cfg(test)]
pub use r#trait::MockShellTrait;

#[cfg(test)]
mod tests;

pub struct OsShell;

impl ShellTrait for OsShell {
    fn execute(&self, cmd: &ShellCommand) -> Result<ShellOutput, ShellError> {
        let output = Command::new(&cmd.program)
            .args(&cmd.args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()?;

        Ok(ShellOutput {
            stdout: output.stdout,
            stderr: output.stderr,
            success: output.status.success(),
        })
    }

    fn execute_with_stdin(
        &self,
        cmd: &ShellCommand,
        stdin: &[u8],
    ) -> Result<ShellOutput, ShellError> {
        let mut child = Command::new(&cmd.program)
            .args(&cmd.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        if let Some(mut handle) = child.stdin.take() {
            handle.write_all(stdin)?;
        }

        let output = child.wait_with_output()?;

        Ok(ShellOutput {
            stdout: output.stdout,
            stderr: output.stderr,
            success: output.status.success(),
        })
    }

    fn spawn_interactive(&self, cmd: &ShellCommand) -> Result<bool, ShellError> {
        let status = Command::new(&cmd.program).args(&cmd.args).status()?;

        Ok(status.success())
    }
}
