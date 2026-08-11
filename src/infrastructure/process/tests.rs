use std::{
    os::unix::process::ExitStatusExt,
    path::{Path, PathBuf},
    process::ExitStatus,
    time::{Duration, Instant},
};

use nix::{errno::Errno, sys::signal::kill, unistd::Pid};

use super::{MockRunningProcess, OsProcess, ProcessTrait, RunningProcess};
use crate::infrastructure::shell::ShellCommand;

struct TempDir(PathBuf);

impl TempDir {
    fn new(test_name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "patch_hub_process_test_{}_{test_name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

async fn wait_for(mut cond: impl FnMut() -> bool, timeout: Duration) -> bool {
    let start = Instant::now();
    while !cond() {
        if start.elapsed() > timeout {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    true
}

fn pid_is_gone(pid: i32) -> bool {
    matches!(
        kill(Pid::from_raw(pid), None::<nix::sys::signal::Signal>),
        Err(Errno::ESRCH)
    )
}

#[tokio::test]
async fn spawn_returns_while_process_still_running() {
    let dir = TempDir::new("spawn_returns");
    let log = dir.path().join("job.log");
    let cmd = ShellCommand::new("sh").args(["-c", "sleep 0.5; echo done"]);

    let mut process = OsProcess.spawn(&cmd, dir.path(), &log).unwrap();

    // spawn returned while the child was still inside its sleep
    let early_log = std::fs::read_to_string(&log).unwrap();
    assert!(!early_log.contains("done"));

    let status = process.wait().await.unwrap();
    assert!(status.success());
    let final_log = std::fs::read_to_string(&log).unwrap();
    assert!(final_log.contains("done"));
}

#[tokio::test]
async fn wait_returns_exit_code() {
    let dir = TempDir::new("exit_code");
    let log = dir.path().join("job.log");
    let cmd = ShellCommand::new("sh").args(["-c", "exit 42"]);

    let mut process = OsProcess.spawn(&cmd, dir.path(), &log).unwrap();
    let status = process.wait().await.unwrap();

    assert!(!status.success());
    assert_eq!(status.code(), Some(42));
}

#[tokio::test]
async fn log_file_grows_incrementally_with_stdout_and_stderr() {
    let dir = TempDir::new("incremental_log");
    let log = dir.path().join("job.log");
    let cmd =
        ShellCommand::new("sh").args(["-c", "echo first; echo errline >&2; sleep 1; echo second"]);

    let mut process = OsProcess.spawn(&cmd, dir.path(), &log).unwrap();

    // output reaches the file while the process is still running, not at exit
    let saw_partial_log = wait_for(
        || {
            let contents = std::fs::read_to_string(&log).unwrap();
            contents.contains("first")
                && contents.contains("errline")
                && !contents.contains("second")
        },
        Duration::from_millis(800),
    )
    .await;
    assert!(saw_partial_log);

    let status = process.wait().await.unwrap();
    assert!(status.success());
    let contents = std::fs::read_to_string(&log).unwrap();
    assert!(contents.contains("first"));
    assert!(contents.contains("errline"));
    assert!(contents.contains("second"));
}

#[tokio::test]
async fn spawn_runs_in_given_cwd() {
    let dir = TempDir::new("cwd");
    let log = dir.path().join("job.log");
    let cmd = ShellCommand::new("pwd");

    let mut process = OsProcess.spawn(&cmd, dir.path(), &log).unwrap();
    let status = process.wait().await.unwrap();

    assert!(status.success());
    let out = std::fs::read_to_string(&log).unwrap();
    let expected = dir.path().canonicalize().unwrap();
    assert_eq!(out.trim(), expected.to_string_lossy());
}

#[tokio::test]
async fn kill_terminates_process_group() {
    let dir = TempDir::new("kill_group");
    let log = dir.path().join("job.log");
    let sidecar = dir.path().join("grandchild.pid");
    let script = format!("sleep 60 & echo $! > \"{}\"; wait", sidecar.display());
    let cmd = ShellCommand::new("sh").args(["-c", &script]);

    let mut process = OsProcess.spawn(&cmd, dir.path(), &log).unwrap();

    let sidecar_ready = wait_for(|| sidecar.exists(), Duration::from_secs(2)).await;
    assert!(sidecar_ready);
    let grandchild_pid: i32 = std::fs::read_to_string(&sidecar)
        .unwrap()
        .trim()
        .parse()
        .unwrap();

    process.kill().unwrap();
    let status = tokio::time::timeout(Duration::from_secs(2), process.wait())
        .await
        .expect("wait must complete shortly after kill")
        .unwrap();
    // Signal death vs. exit code 128+SIGTERM is shell-dependent; what matters
    // is the job did not succeed.
    assert!(!status.success());

    let grandchild_gone = wait_for(|| pid_is_gone(grandchild_pid), Duration::from_secs(2)).await;
    assert!(grandchild_gone);
}

#[tokio::test]
async fn kill_after_successful_exit_is_ok() {
    let dir = TempDir::new("kill_idempotent");
    let log = dir.path().join("job.log");
    let cmd = ShellCommand::new("sh").args(["-c", "exit 0"]);

    let mut process = OsProcess.spawn(&cmd, dir.path(), &log).unwrap();
    process.wait().await.unwrap();

    process.kill().unwrap();
    process.kill().unwrap();
}

#[tokio::test]
async fn spawn_missing_binary_returns_error() {
    let dir = TempDir::new("missing_binary");
    let log = dir.path().join("job.log");
    let cmd = ShellCommand::new("__nonexistent_binary_patch_hub__");

    let result = OsProcess.spawn(&cmd, dir.path(), &log);

    assert!(result.is_err());
}

#[tokio::test]
async fn dropped_unreaped_process_group_is_killed() {
    let dir = TempDir::new("drop_kills");
    let log = dir.path().join("job.log");
    let sidecar = dir.path().join("grandchild.pid");
    let script = format!("sleep 60 & echo $! > \"{}\"; wait", sidecar.display());
    let cmd = ShellCommand::new("sh").args(["-c", &script]);

    let process = OsProcess.spawn(&cmd, dir.path(), &log).unwrap();

    let sidecar_ready = wait_for(|| sidecar.exists(), Duration::from_secs(2)).await;
    assert!(sidecar_ready);
    let grandchild_pid: i32 = std::fs::read_to_string(&sidecar)
        .unwrap()
        .trim()
        .parse()
        .unwrap();

    // no wait(): dropping the handle must not orphan the process group
    drop(process);

    let grandchild_gone = wait_for(|| pid_is_gone(grandchild_pid), Duration::from_secs(2)).await;
    assert!(grandchild_gone);
}

#[tokio::test]
async fn running_process_is_dyn_compatible_and_mockable() {
    let mut mock = MockRunningProcess::new();
    mock.expect_wait().returning(|| Ok(ExitStatus::from_raw(0)));
    mock.expect_kill().returning(|| Ok(()));

    let mut process: Box<dyn RunningProcess> = Box::new(mock);
    process.kill().unwrap();
    let status = process.wait().await.unwrap();
    assert!(status.success());
}
