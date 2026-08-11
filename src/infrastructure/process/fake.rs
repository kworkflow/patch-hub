use std::{
    fs::{File, OpenOptions},
    io::Write,
    os::unix::process::ExitStatusExt,
    path::{Path, PathBuf},
    process::ExitStatus,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use nix::sys::signal::Signal;
use tokio::sync::Notify;

use super::{ProcessError, ProcessTrait, RunningProcess};
use crate::infrastructure::shell::ShellCommand;

/// Everything a [`FakeProcess::spawn`] call was asked to do.
#[derive(Debug, Clone)]
pub struct SpawnRecord {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub log_path: PathBuf,
}

/// Test-side control over the [`FakeRunningProcess`] produced by one spawn:
/// when and how it finishes, whether it was killed, and what it "wrote" to
/// its log file while running.
pub struct FakeControl {
    state: Mutex<FakeState>,
    notify: Notify,
    log_path: PathBuf,
}

// Raw wait-status encoding (see waitpid(2)): an exit code lives in bits 8–15,
// a killing signal in the low 7 bits. Storing raw values lets `wait()` hand
// back a real `ExitStatus` via `ExitStatusExt::from_raw`.
struct FakeState {
    raw_status: Option<i32>,
    killed: bool,
}

impl FakeControl {
    pub fn write_log(&self, contents: &[u8]) {
        let mut file = OpenOptions::new()
            .append(true)
            .open(&self.log_path)
            .unwrap();
        file.write_all(contents).unwrap();
    }

    /// Unblock `wait()`, reporting exit with `exit_code`.
    pub fn finish(&self, exit_code: i32) {
        self.state.lock().unwrap().raw_status = Some(exit_code << 8);
        self.notify.notify_one();
    }

    pub fn was_killed(&self) -> bool {
        self.state.lock().unwrap().killed
    }
}

struct FakeSpawn {
    record: SpawnRecord,
    control: Arc<FakeControl>,
}

/// A [`ProcessTrait`] double that never executes anything: spawns are
/// recorded for later argv/cwd assertions and each returns a
/// [`FakeRunningProcess`] the test drives through its [`FakeControl`].
#[derive(Default)]
pub struct FakeProcess {
    spawns: Mutex<Vec<FakeSpawn>>,
}

impl FakeProcess {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn spawned(&self) -> Vec<SpawnRecord> {
        self.spawns
            .lock()
            .unwrap()
            .iter()
            .map(|spawn| spawn.record.clone())
            .collect()
    }

    /// # Panics
    ///
    /// If nothing has been spawned yet.
    pub fn last_child(&self) -> Arc<FakeControl> {
        self.spawns
            .lock()
            .unwrap()
            .last()
            .map(|spawn| spawn.control.clone())
            .expect("FakeProcess::last_child called before any spawn")
    }
}

impl ProcessTrait for FakeProcess {
    fn spawn(
        &self,
        cmd: &ShellCommand,
        cwd: &Path,
        log_path: &Path,
    ) -> Result<Box<dyn RunningProcess>, ProcessError> {
        File::create(log_path)?;

        let control = Arc::new(FakeControl {
            state: Mutex::new(FakeState {
                raw_status: None,
                killed: false,
            }),
            notify: Notify::new(),
            log_path: log_path.to_path_buf(),
        });
        self.spawns.lock().unwrap().push(FakeSpawn {
            record: SpawnRecord {
                program: cmd.program.clone(),
                args: cmd.args.clone(),
                cwd: cwd.to_path_buf(),
                log_path: log_path.to_path_buf(),
            },
            control: control.clone(),
        });

        Ok(Box::new(FakeRunningProcess { control }))
    }
}

struct FakeRunningProcess {
    control: Arc<FakeControl>,
}

#[async_trait]
impl RunningProcess for FakeRunningProcess {
    async fn wait(&mut self) -> Result<ExitStatus, ProcessError> {
        loop {
            // The state lock is released before the await; a finish()/kill()
            // racing the check is not lost because Notify stores one permit.
            let raw_status = self.control.state.lock().unwrap().raw_status;
            if let Some(raw) = raw_status {
                return Ok(ExitStatus::from_raw(raw));
            }
            self.control.notify.notified().await;
        }
    }

    fn kill(&mut self) -> Result<(), ProcessError> {
        let mut state = self.control.state.lock().unwrap();
        state.killed = true;
        if state.raw_status.is_none() {
            state.raw_status = Some(Signal::SIGTERM as i32);
        }
        drop(state);
        self.control.notify.notify_one();
        Ok(())
    }
}
