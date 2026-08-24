use std::{
    fs::{File, OpenOptions},
    io::{self, Write},
    os::unix::process::ExitStatusExt,
    path::{Path, PathBuf},
    process::ExitStatus,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
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
    force_killed: bool,
    /// When set, `kill()` (SIGTERM) is recorded but the process keeps
    /// "running": the model for a process group that ignores SIGTERM, so
    /// tests can exercise the SIGKILL escalation.
    ignores_sigterm: bool,
}

impl FakeControl {
    pub fn write_log(&self, contents: &[u8]) {
        let mut file = OpenOptions::new()
            .append(true)
            .open(&self.log_path)
            .unwrap();
        file.write_all(contents).unwrap();
    }

    /// Unblock `wait()`, reporting exit with `exit_code`. A terminal state is
    /// terminal: a process already finished or killed does not exit later.
    pub fn finish(&self, exit_code: i32) {
        let mut state = self.state.lock().unwrap();
        if state.raw_status.is_none() {
            state.raw_status = Some(exit_code << 8);
            drop(state);
            self.notify.notify_one();
        }
    }

    pub fn was_killed(&self) -> bool {
        self.state.lock().unwrap().killed
    }

    pub fn was_force_killed(&self) -> bool {
        self.state.lock().unwrap().force_killed
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
    refuse_spawns: AtomicBool,
    ignore_sigterm: AtomicBool,
}

impl FakeProcess {
    pub fn new() -> Self {
        Self::default()
    }

    /// Make `spawn` fail with an IO error, like a missing binary or an
    /// unwritable log path would; no log file is created and nothing is
    /// recorded, mirroring `OsProcess`'s failure behavior.
    pub fn refuse_spawns(&self, refuse: bool) {
        self.refuse_spawns.store(refuse, Ordering::Relaxed);
    }

    /// Make subsequently spawned processes ignore `kill()` (SIGTERM): the
    /// kill is recorded but they keep "running" until `force_kill()`.
    pub fn ignore_sigterm(&self, ignore: bool) {
        self.ignore_sigterm.store(ignore, Ordering::Relaxed);
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
        if self.refuse_spawns.load(Ordering::Relaxed) {
            return Err(ProcessError::IoError(io::Error::new(
                io::ErrorKind::NotFound,
                "fake spawn failure",
            )));
        }
        File::create(log_path)?;

        let control = Arc::new(FakeControl {
            state: Mutex::new(FakeState {
                raw_status: None,
                killed: false,
                force_killed: false,
                ignores_sigterm: self.ignore_sigterm.load(Ordering::Relaxed),
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
        // Mirrors the real kill()'s ESRCH tolerance: killing an already-dead
        // process is a successful no-op, not a kill.
        if state.raw_status.is_none() {
            state.killed = true;
            if !state.ignores_sigterm {
                state.raw_status = Some(Signal::SIGTERM as i32);
                drop(state);
                self.control.notify.notify_one();
            }
        }
        Ok(())
    }

    fn force_kill(&mut self) -> Result<(), ProcessError> {
        let mut state = self.control.state.lock().unwrap();
        // SIGKILL cannot be ignored: even a SIGTERM-stubborn process dies.
        if state.raw_status.is_none() {
            state.force_killed = true;
            state.raw_status = Some(Signal::SIGKILL as i32);
            drop(state);
            self.control.notify.notify_one();
        }
        Ok(())
    }
}
