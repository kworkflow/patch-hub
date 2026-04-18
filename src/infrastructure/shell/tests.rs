use super::{OsShell, ShellCommand, ShellTrait};

#[test]
fn execute_captures_stdout() {
    let shell = OsShell;
    let cmd = ShellCommand::new("echo").arg("hello shell");
    let out = shell.execute(&cmd).unwrap();
    assert!(out.success);
    assert_eq!(String::from_utf8(out.stdout).unwrap().trim(), "hello shell");
}

#[test]
fn execute_captures_stderr() {
    let shell = OsShell;
    // `ls` on a nonexistent path writes to stderr and exits nonzero
    let cmd = ShellCommand::new("ls").arg("/nonexistent_path_patch_hub_test");
    let out = shell.execute(&cmd).unwrap();
    assert!(!out.success);
    assert!(!out.stderr.is_empty());
}

#[test]
fn execute_with_stdin_pipes_input() {
    let shell = OsShell;
    let cmd = ShellCommand::new("cat");
    let out = shell.execute_with_stdin(&cmd, b"piped input").unwrap();
    assert!(out.success);
    assert_eq!(out.stdout, b"piped input");
}

#[test]
fn execute_with_stdin_on_failing_command_returns_error() {
    let shell = OsShell;
    let cmd = ShellCommand::new("false");
    let out = shell.execute_with_stdin(&cmd, b"").unwrap();
    assert!(!out.success);
}

#[test]
fn execute_returns_error_for_missing_binary() {
    let shell = OsShell;
    let cmd = ShellCommand::new("__nonexistent_binary_patch_hub__");
    let result = shell.execute(&cmd);
    assert!(result.is_err());
}

#[test]
fn shell_command_builder_chains_args() {
    let cmd = ShellCommand::new("git")
        .arg("-C")
        .arg("/tmp")
        .args(["status", "--porcelain"]);
    assert_eq!(cmd.program, "git");
    assert_eq!(cmd.args, vec!["-C", "/tmp", "status", "--porcelain"]);
}
