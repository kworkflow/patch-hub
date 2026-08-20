//! kw actor: owns kw build/deploy job state and the kw history store.
//!
//! All kw operations go through [`KwHandle`](crate::kw::handle::KwHandle) as
//! typed request/reply messages. `Start*` messages reply immediately with an
//! accept/refuse verdict — a job accepted by the actor keeps running after
//! the caller has been answered, so the AppActor loop never blocks on a
//! kernel build. The actor composes the readiness probes from
//! [`crate::kw::readiness`] and records applies through the shared
//! [`KwHistoryStore`](crate::kw::history::KwHistoryStore).
//!
//! `create_dir_all`, the HEAD-probe `git` call, and history writes run
//! inline on the actor task.

use std::{ops::ControlFlow, path::PathBuf, process::ExitStatus, sync::Arc, time::Duration};

use tokio::{
    spawn,
    sync::{mpsc, oneshot, watch},
};

use crate::{
    config::KernelTree,
    infrastructure::{
        env::EnvTrait,
        file_system::FileSystemTrait,
        process::{ProcessError, ProcessTrait, RunningProcess},
        shell::{ShellCommand, ShellTrait},
    },
    kw::{
        errors::{KwError, KwStartError},
        handle::KwHandle,
        history::KwHistoryStore,
        messages::{KwMessage, StartRequest},
        readiness::{self, KwReadiness, KwVersionCheck, TreeReadiness},
        status::{KwJobKind, KwJobStatus, KwPhase, KwStatusSnapshot},
    },
};

pub const DEFAULT_KW_CHANNEL_SIZE: usize = 16;

/// Grace periods for the cancel escalation ladder: SIGTERM the process
/// group, wait, SIGKILL, wait, then give up. Giving up still terminates the
/// job from the actor's point of view — a group that ignores both signals
/// must not wedge the actor into refusing every later Start with
/// JobAlreadyRunning for the rest of the session.
const TERM_GRACE: Duration = Duration::from_secs(3);
const KILL_GRACE: Duration = Duration::from_secs(2);

/// How a job's process ended, as observed by the detached task that owns
/// the process handle.
enum JobOutcome {
    Exited(ExitStatus),
    WaitFailed(ProcessError),
    Cancelled,
}

/// Internal report from a job task back to the actor loop. Kept off the
/// public [`KwMessage`] protocol: no caller can fake a completion.
enum JobEvent {
    Finished(JobOutcome),
}

/// What the actor remembers about the running job while the detached task
/// owns the process itself (see [`run_job`]).
struct JobState {
    kind: KwJobKind,
    phase: KwPhase,
    kernel_tree_id: String,
    branch: String,
    log_path: PathBuf,
    /// HEAD at accept time, so RestorePreviousBranch can offer to switch
    /// back after the job. Read once the checkout policy lands.
    #[allow(dead_code)]
    pre_job_branch: Option<String>,
    /// `None` once a cancel has been requested; a second `Cancel` is an
    /// idempotent ack.
    cancel_tx: Option<oneshot::Sender<()>>,
}

pub struct KwActor {
    rx: mpsc::Receiver<KwMessage>,
    job_event_rx: mpsc::Receiver<JobEvent>,
    job_event_tx: mpsc::Sender<JobEvent>,
    status_tx: watch::Sender<KwStatusSnapshot>,
    history: Arc<dyn KwHistoryStore>,
    shell: Arc<dyn ShellTrait>,
    fs: Arc<dyn FileSystemTrait>,
    env: Arc<dyn EnvTrait>,
    process: Arc<dyn ProcessTrait>,
    kw_log_dir: PathBuf,
    job: Option<JobState>,
}

impl KwActor {
    pub fn new(
        rx: mpsc::Receiver<KwMessage>,
        history: Arc<dyn KwHistoryStore>,
        process: Arc<dyn ProcessTrait>,
        shell: Arc<dyn ShellTrait>,
        fs: Arc<dyn FileSystemTrait>,
        env: Arc<dyn EnvTrait>,
        kw_log_dir: PathBuf,
    ) -> Self {
        let (status_tx, _) = watch::channel(KwStatusSnapshot::idle());
        let (job_event_tx, job_event_rx) = mpsc::channel(DEFAULT_KW_CHANNEL_SIZE);
        Self {
            rx,
            job_event_rx,
            job_event_tx,
            status_tx,
            history,
            shell,
            fs,
            env,
            process,
            kw_log_dir,
            job: None,
        }
    }

    pub fn spawn(
        history: Arc<dyn KwHistoryStore>,
        process: Arc<dyn ProcessTrait>,
        shell: Arc<dyn ShellTrait>,
        fs: Arc<dyn FileSystemTrait>,
        env: Arc<dyn EnvTrait>,
        kw_log_dir: PathBuf,
    ) -> KwHandle {
        let (tx, rx) = mpsc::channel(DEFAULT_KW_CHANNEL_SIZE);
        tracing::debug!(channel_size = DEFAULT_KW_CHANNEL_SIZE, "spawning kw actor");
        spawn(Self::new(rx, history, process, shell, fs, env, kw_log_dir).run());
        KwHandle::new(tx)
    }

    pub async fn run(mut self) {
        tracing::info!("kw actor started");
        // The job-event sender is held by the actor itself, so that arm of
        // the select never closes while the actor is alive.
        loop {
            tokio::select! {
                message = self.rx.recv() => {
                    let Some(message) = message else { break };
                    if let ControlFlow::Break(()) = self.handle_message(message).await {
                        break;
                    }
                }
                Some(event) = self.job_event_rx.recv() => {
                    self.handle_job_event(event);
                }
            }
        }
        tracing::info!("kw actor stopped");
    }

    async fn handle_message(&mut self, message: KwMessage) -> ControlFlow<()> {
        let message_name = message.name();
        tracing::debug!(message = message_name, "kw request received");

        match message {
            KwMessage::RecordApply { record, reply } => {
                send_kw_reply(
                    message_name,
                    reply,
                    self.history.record_apply(record).map_err(KwError::from),
                );
                ControlFlow::Continue(())
            }
            KwMessage::StartBuild { request, reply } => {
                send_start_reply(
                    message_name,
                    reply,
                    self.start_job(KwJobKind::Build, request),
                );
                ControlFlow::Continue(())
            }
            // Not implemented: reply immediately with NotImplemented.
            KwMessage::StartDeploy { reply, .. }
            | KwMessage::StartBuildThenDeploy { reply, .. } => {
                send_start_reply(message_name, reply, Err(KwStartError::NotImplemented));
                ControlFlow::Continue(())
            }
            KwMessage::Cancel { reply } => {
                send_kw_reply(message_name, reply, self.request_cancel());
                ControlFlow::Continue(())
            }
            KwMessage::GetStatus { reply } => {
                send_value_reply(message_name, reply, self.status_tx.borrow().clone());
                ControlFlow::Continue(())
            }
            KwMessage::WatchStatus { reply } => {
                send_value_reply(message_name, reply, self.status_tx.subscribe());
                ControlFlow::Continue(())
            }
            KwMessage::GetReadiness {
                kernel_tree_id,
                tree,
                reply,
            } => {
                send_kw_reply(
                    message_name,
                    reply,
                    self.evaluate_readiness(&kernel_tree_id, &tree),
                );
                ControlFlow::Continue(())
            }
            // Restoring needs the dirty-worktree check and `git switch`,
            // which arrive with the checkout policy in the build step.
            KwMessage::RestorePreviousBranch { reply } => {
                send_kw_reply(message_name, reply, Err(KwError::NoRecordedBranch));
                ControlFlow::Continue(())
            }
            KwMessage::Shutdown { reply } => {
                // Quit-prompt cooperation: the app asks whether a job is
                // running before quitting; if it quits anyway, the job's
                // process group is killed here. The log may capture partial
                // output and the tree a partial build. The reply is held
                // until the kill escalation has run its (bounded) course,
                // so a caller tearing down the runtime afterwards cannot
                // leave orphaned kw processes behind.
                if self.job.is_some() {
                    tracing::info!("killing kw job process group during shutdown");
                    self.request_cancel().ok();
                    while self.job.is_some() {
                        // `None` is unreachable — the actor holds
                        // `job_event_tx`, so the channel never closes —
                        // but break defensively rather than spin.
                        match self.job_event_rx.recv().await {
                            Some(event) => self.handle_job_event(event),
                            None => break,
                        }
                    }
                }
                reply.send(()).ok();
                tracing::debug!("kw actor shutting down");
                ControlFlow::Break(())
            }
        }
    }

    /// Accepts and starts a build job, or refuses. The reply is sent by the
    /// caller right after this returns: the job itself keeps running in a
    /// detached task and is observed via the status snapshot.
    ///
    /// Refusals, in order: a job already running, no kw binary on PATH,
    /// unresolvable kw-env state, or a tree that fails the readiness
    /// probes. The argv is still the skeleton's minimal `kw build
    /// --alert=n`; the real argv builder (reserved flags, extra-args merge)
    /// and the checkout policy land later in the build step. The `branch`
    /// carried by the Running status is the *requested* branch; the
    /// checkout policy is what will make the tree actually sit on it.
    fn start_job(&mut self, kind: KwJobKind, request: StartRequest) -> Result<(), KwStartError> {
        if self.job.is_some() {
            return Err(KwStartError::JobAlreadyRunning);
        }

        // Hard fail on invoke (integration plan §2.4): with no kw binary on
        // PATH no job can run. The version check is advisory only — kw's
        // shipped VERSION file is stale, so Below/Unknown are logged, never
        // gated.
        let kw_binary = readiness::probe_kw_binary(&*self.env, &*self.shell);
        if !kw_binary.available {
            return Err(KwStartError::KwBinaryMissing);
        }
        if let KwVersionCheck::Below(version_line) = &kw_binary.check {
            tracing::warn!(
                version = %version_line,
                minimum = ?readiness::KW_MIN_VERSION,
                "kw reports a version below the verified floor"
            );
        }

        let tree_path = PathBuf::from(request.tree.path());
        // Unresolvable env state refuses the start: the build record this
        // job writes at completion must know whether it ran under an O=.
        let output_dir = readiness::resolve_output_dir(&*self.fs, &*self.env, &tree_path)?;
        let tree_readiness = readiness::probe_tree(&*self.fs, &tree_path, output_dir.as_deref());
        if !matches!(tree_readiness, TreeReadiness::Ready { .. }) {
            return Err(KwStartError::TreeNotReady(tree_readiness));
        }

        // Probed before anything touches the tree: once the checkout policy
        // lands, `git switch <selected branch>` goes between this probe and
        // the spawn, and the probe must still capture the pre-job HEAD or
        // RestorePreviousBranch would "restore" the branch the job switched
        // to. An unprobed HEAD (detached, or not a git repo) records
        // nothing rather than a wrong branch.
        let pre_job_branch = match self.head_branch(&request.tree) {
            branch if branch.is_empty() => None,
            branch => Some(branch),
        };

        self.fs.create_dir_all(&self.kw_log_dir)?;
        // Millisecond suffix: two jobs started within the same second must
        // not share a log file — spawn truncates it.
        let log_path = self.kw_log_dir.join(format!(
            "build-{}.log",
            chrono::Utc::now().format("%Y%m%d-%H%M%S-%3f")
        ));
        let cmd = ShellCommand::new("kw").args(["build", "--alert=n"]);
        let cwd = PathBuf::from(request.tree.path());
        let process = self.process.spawn(&cmd, &cwd, &log_path)?;

        let (cancel_tx, cancel_rx) = oneshot::channel();
        spawn(run_job(process, cancel_rx, self.job_event_tx.clone()));

        let phase = KwPhase::Building;
        tracing::info!(
            kernel_tree_id = request.kernel_tree_id,
            branch = request.branch,
            log_path = %log_path.display(),
            "kw build job started"
        );
        self.job = Some(JobState {
            kind,
            phase,
            kernel_tree_id: request.kernel_tree_id.clone(),
            branch: request.branch.clone(),
            log_path: log_path.clone(),
            pre_job_branch,
            cancel_tx: Some(cancel_tx),
        });
        self.set_status(KwJobStatus::Running {
            kind,
            phase,
            kernel_tree_id: request.kernel_tree_id,
            branch: request.branch,
            log_path,
        });
        Ok(())
    }

    fn request_cancel(&mut self) -> Result<(), KwError> {
        match self.job.as_mut() {
            Some(job) => {
                if let Some(cancel) = job.cancel_tx.take() {
                    let _ = cancel.send(());
                }
                Ok(())
            }
            None => Err(KwError::NoJobRunning),
        }
    }

    fn handle_job_event(&mut self, event: JobEvent) {
        match event {
            JobEvent::Finished(outcome) => {
                let Some(job) = self.job.take() else {
                    tracing::warn!("kw job finished with no job state recorded");
                    return;
                };
                let status = match outcome {
                    JobOutcome::Exited(exit) if exit.success() => {
                        tracing::info!(
                            kernel_tree_id = job.kernel_tree_id,
                            branch = job.branch,
                            "kw job succeeded"
                        );
                        KwJobStatus::Succeeded {
                            kind: job.kind,
                            kernel_tree_id: job.kernel_tree_id,
                            branch: job.branch,
                            log_path: job.log_path,
                        }
                    }
                    JobOutcome::Exited(exit) => {
                        tracing::warn!(
                            kernel_tree_id = job.kernel_tree_id,
                            branch = job.branch,
                            exit_code = exit.code(),
                            "kw job failed"
                        );
                        KwJobStatus::Failed {
                            kind: job.kind,
                            phase: job.phase,
                            exit_code: exit.code(),
                            log_path: job.log_path,
                        }
                    }
                    JobOutcome::WaitFailed(error) => {
                        tracing::warn!(branch = job.branch, %error, "failed to wait on kw job");
                        KwJobStatus::Failed {
                            kind: job.kind,
                            phase: job.phase,
                            exit_code: None,
                            log_path: job.log_path,
                        }
                    }
                    JobOutcome::Cancelled => {
                        tracing::info!(branch = job.branch, "kw job cancelled");
                        KwJobStatus::Cancelled {
                            kind: job.kind,
                            phase: job.phase,
                        }
                    }
                };
                self.set_status(status);
            }
        }
    }

    fn set_status(&mut self, job: KwJobStatus) {
        // send_replace, not send: no receiver (nobody called WatchStatus
        // yet) is a normal state, not an error.
        self.status_tx.send_replace(KwStatusSnapshot { job });
    }

    fn evaluate_readiness(
        &self,
        kernel_tree_id: &str,
        tree: &KernelTree,
    ) -> Result<KwReadiness, KwError> {
        let head = self.head_branch(tree);
        Ok(readiness::evaluate_readiness(
            &*self.fs,
            &*self.env,
            &*self.shell,
            &*self.history,
            kernel_tree_id,
            tree,
            &head,
        )?)
    }

    /// The tree's current branch, probed via git. An unresolvable HEAD
    /// (not a git repo, or a detached HEAD, for which `branch
    /// --show-current` prints nothing) yields an empty string: no build
    /// record can match it, so deploy-alone readiness refuses — the safe
    /// direction for an unknown HEAD.
    fn head_branch(&self, tree: &KernelTree) -> String {
        let cmd = ShellCommand::new("git").args(["-C", tree.path(), "branch", "--show-current"]);
        match self.shell.execute(&cmd) {
            Ok(output) if output.success => {
                String::from_utf8_lossy(&output.stdout).trim().to_string()
            }
            Ok(output) => {
                tracing::warn!(
                    tree = tree.path(),
                    stderr = %String::from_utf8_lossy(&output.stderr),
                    "failed to probe the kernel tree's HEAD branch"
                );
                String::new()
            }
            Err(error) => {
                tracing::warn!(tree = tree.path(), %error, "failed to probe the kernel tree's HEAD branch");
                String::new()
            }
        }
    }
}

/// Owns the spawned process until it ends: waits on it, or — when the
/// cancel signal fires — runs the kill escalation and reaps it. The
/// `wait()` future is dropped before the cancel arm's body runs, releasing
/// the mutable borrow so `kill()` can be called. Reports the outcome back
/// to the actor over the internal event channel.
async fn run_job(
    mut process: Box<dyn RunningProcess>,
    mut cancel_rx: oneshot::Receiver<()>,
    events: mpsc::Sender<JobEvent>,
) {
    let outcome = tokio::select! {
        status = process.wait() => match status {
            Ok(status) => JobOutcome::Exited(status),
            Err(error) => JobOutcome::WaitFailed(error),
        },
        // A dropped sender (actor shutting down) cancels the job too.
        _ = &mut cancel_rx => cancel_job(&mut *process).await,
    };
    events.send(JobEvent::Finished(outcome)).await.ok();
}

/// Cancel escalation ladder: SIGTERM the group, wait a grace period,
/// SIGKILL, wait again, then give up. Giving up still reports Cancelled and
/// lets the actor clear its job state — a process group that ignores both
/// signals must not wedge the actor into refusing every later Start with
/// `JobAlreadyRunning` for the rest of the session.
async fn cancel_job(process: &mut dyn RunningProcess) -> JobOutcome {
    if let Err(error) = process.kill() {
        tracing::warn!(%error, "failed to SIGTERM kw job process group");
    }
    match tokio::time::timeout(TERM_GRACE, process.wait()).await {
        Ok(outcome) => outcome_after_cancel(outcome),
        Err(_) => {
            tracing::warn!("kw job ignored SIGTERM; escalating to SIGKILL");
            if let Err(error) = process.force_kill() {
                tracing::warn!(%error, "failed to SIGKILL kw job process group");
            }
            match tokio::time::timeout(KILL_GRACE, process.wait()).await {
                Ok(outcome) => outcome_after_cancel(outcome),
                Err(_) => {
                    tracing::warn!(
                        "kw job process group could not be reaped after SIGKILL; giving up"
                    );
                    JobOutcome::Cancelled
                }
            }
        }
    }
}

/// Maps a reap result observed after a cancel request. A signal-terminated
/// process means our SIGTERM/SIGKILL landed — the job was really cancelled.
/// A plain exit means the process finished on its own before the signal:
/// report the real outcome, because a cancel must not mask a failure the
/// build history (and deploy-alone readiness) needs to see.
///
/// Known, accepted edges: a process that *traps* our SIGTERM and exits 0
/// counts as success (kw is bash, so this is possible in principle), and an
/// external signal racing a cancel (e.g. the OOM killer) reads as
/// Cancelled. Both are indistinguishable from the honest cases without
/// comparing who signaled first, and both favor showing the user real
/// output over inventing failures.
fn outcome_after_cancel(result: Result<ExitStatus, ProcessError>) -> JobOutcome {
    use std::os::unix::process::ExitStatusExt;

    match result {
        Ok(status) => match status.signal() {
            Some(_) => JobOutcome::Cancelled,
            None => JobOutcome::Exited(status),
        },
        Err(error) => JobOutcome::WaitFailed(error),
    }
}

fn send_kw_reply<T>(
    message_name: &'static str,
    reply: oneshot::Sender<Result<T, KwError>>,
    result: Result<T, KwError>,
) {
    if let Err(error) = &result {
        tracing::warn!(
            message = message_name,
            error = %error,
            "kw request failed"
        );
    }

    if reply.send(result).is_err() {
        tracing::warn!(
            message = message_name,
            "kw reply receiver dropped before response"
        );
    }
}

fn send_start_reply(
    message_name: &'static str,
    reply: oneshot::Sender<Result<(), KwStartError>>,
    result: Result<(), KwStartError>,
) {
    if let Err(error) = &result {
        tracing::warn!(
            message = message_name,
            error = %error,
            "kw start request refused"
        );
    }

    if reply.send(result).is_err() {
        tracing::warn!(
            message = message_name,
            "kw reply receiver dropped before response"
        );
    }
}

fn send_value_reply<T>(message_name: &'static str, reply: oneshot::Sender<T>, value: T) {
    if reply.send(value).is_err() {
        tracing::warn!(
            message = message_name,
            "kw reply receiver dropped before response"
        );
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io,
        path::Path,
        sync::atomic::{AtomicU64, Ordering},
        time::Duration,
    };

    use crate::{
        infrastructure::{
            env::MockEnvTrait,
            file_system::{FileSystemError, MockFileSystemTrait},
            process::FakeProcess,
            shell::{MockShellTrait, ShellOutput},
        },
        kw::{
            errors::KwStartError,
            history::{KwApplyRecord, MockKwHistoryStore},
            messages::StartRequest,
            readiness::{DeployAloneRefusal, TreeReadiness},
            status::{KwJobKind, KwJobStatus, KwPhase},
        },
    };

    use super::*;

    static TEST_SEQ: AtomicU64 = AtomicU64::new(0);

    /// A real directory: FakeProcess creates the log file on spawn, so the
    /// parent must exist even though the fs trait is mocked.
    fn tmp_log_dir(test_name: &str) -> PathBuf {
        let n = TEST_SEQ.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "patch-hub-kw-actor-{}-{test_name}-{n}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn kernel_tree(path: &Path) -> KernelTree {
        serde_json::from_value(serde_json::json!({
            "path": path.to_str().unwrap(),
            "branch": "master"
        }))
        .unwrap()
    }

    fn start_request() -> StartRequest {
        StartRequest {
            kernel_tree_id: "mainline".to_string(),
            tree: kernel_tree(Path::new("/home/user/linux")),
            branch: "patchset-2026-08-01-17-30-00".to_string(),
        }
    }

    /// Spawns the actor with the already-configured mocks (mockall
    /// expectations need `&mut`, so they are set before the mocks move
    /// behind `Arc`s). The log dir is a real unique temp dir even though
    /// these tests never start a job, so a future test that accidentally
    /// does cannot share a fixed path with the job tests.
    fn spawn_test_actor(
        test_name: &str,
        history: MockKwHistoryStore,
        shell: MockShellTrait,
        fs: MockFileSystemTrait,
        env: MockEnvTrait,
    ) -> KwHandle {
        KwActor::spawn(
            Arc::new(history),
            Arc::new(FakeProcess::new()),
            Arc::new(shell),
            Arc::new(fs),
            Arc::new(env),
            tmp_log_dir(test_name),
        )
    }

    /// fs answers for a ready kernel tree with no active kw env: the
    /// kernel-root probes pass, `.config` exists, `.kw/env.current` is
    /// absent, and `.kw/build.config` is unreadable (arch probes as None).
    fn expect_ready_tree(fs: &mut MockFileSystemTrait) {
        fs.expect_is_dir().returning(|_| true);
        fs.expect_is_file()
            .returning(|path| !path.ends_with(".kw/env.current"));
        fs.expect_exists().returning(|_| true);
        fs.expect_read_to_string().returning(|_| {
            Err(FileSystemError::IoError(io::Error::new(
                io::ErrorKind::NotFound,
                "missing",
            )))
        });
    }

    /// Spawns the actor with a real temp log dir and exposes the
    /// [`FakeProcess`] so tests drive the "running" process. The env mock
    /// has kw on PATH; the shell mock answers the kw version probe and the
    /// pre-job HEAD probe.
    fn spawn_job_actor_with_fs(
        test_name: &str,
        fs: MockFileSystemTrait,
    ) -> (KwHandle, Arc<FakeProcess>, PathBuf) {
        let process = Arc::new(FakeProcess::new());
        let log_dir = tmp_log_dir(test_name);
        let mut shell = MockShellTrait::new();
        shell.expect_execute().returning(|cmd| {
            let stdout = match cmd.program.as_str() {
                "kw" => b"kw, version 0.10.0\n".to_vec(),
                _ => b"master\n".to_vec(),
            };
            Ok(ShellOutput {
                stdout,
                stderr: Vec::new(),
                success: true,
            })
        });
        let mut env = MockEnvTrait::new();
        env.expect_which().returning(|_| true);
        let handle = KwActor::spawn(
            Arc::new(MockKwHistoryStore::new()),
            process.clone(),
            Arc::new(shell),
            Arc::new(fs),
            Arc::new(env),
            log_dir.clone(),
        );
        (handle, process, log_dir)
    }

    fn spawn_job_actor(test_name: &str) -> (KwHandle, Arc<FakeProcess>, PathBuf) {
        let mut fs = MockFileSystemTrait::new();
        expect_ready_tree(&mut fs);
        fs.expect_create_dir_all().returning(|_| Ok(()));
        spawn_job_actor_with_fs(test_name, fs)
    }

    fn apply_record() -> KwApplyRecord {
        KwApplyRecord {
            message_id: "msg-1".to_string(),
            kernel_tree_id: "mainline".to_string(),
            tree_path: "/home/user/linux".to_string(),
            applied_branch: "patchset-2026-08-01-17-30-00".to_string(),
            base_branch: "master".to_string(),
            applied_at: "2026-08-01T17:30:00Z".to_string(),
        }
    }

    #[tokio::test]
    async fn record_apply_writes_through_history_store() {
        let expected = apply_record();
        let mut history = MockKwHistoryStore::new();
        history
            .expect_record_apply()
            .withf(move |record| *record == expected)
            .times(1)
            .returning(|_| Ok(()));
        let handle = spawn_test_actor(
            "record-apply",
            history,
            MockShellTrait::new(),
            MockFileSystemTrait::new(),
            MockEnvTrait::new(),
        );

        handle.record_apply(apply_record()).await.unwrap();
        handle.shutdown().await;
    }

    #[tokio::test]
    async fn record_apply_surfaces_store_errors() {
        let mut history = MockKwHistoryStore::new();
        history
            .expect_record_apply()
            .times(1)
            .returning(|_| Err(FileSystemError::IoError(io::Error::other("disk full"))));
        let handle = spawn_test_actor(
            "record-apply-error",
            history,
            MockShellTrait::new(),
            MockFileSystemTrait::new(),
            MockEnvTrait::new(),
        );

        let err = handle.record_apply(apply_record()).await.unwrap_err();

        assert!(matches!(err, KwError::History(_)));
        handle.shutdown().await;
    }

    #[tokio::test]
    async fn get_status_reports_idle_before_any_job() {
        let handle = spawn_test_actor(
            "idle-status",
            MockKwHistoryStore::new(),
            MockShellTrait::new(),
            MockFileSystemTrait::new(),
            MockEnvTrait::new(),
        );

        let snapshot = handle.get_status().await.unwrap();

        assert_eq!(KwJobStatus::Idle, snapshot.job);
        handle.shutdown().await;
    }

    #[tokio::test]
    async fn watch_status_receiver_sees_current_snapshot() {
        let handle = spawn_test_actor(
            "watch-idle",
            MockKwHistoryStore::new(),
            MockShellTrait::new(),
            MockFileSystemTrait::new(),
            MockEnvTrait::new(),
        );

        let receiver = handle.watch_status().await.unwrap();

        assert_eq!(KwJobStatus::Idle, receiver.borrow().job);
        handle.shutdown().await;
    }

    #[tokio::test]
    async fn get_readiness_composes_probes_and_head_branch() {
        let tree = kernel_tree(Path::new("/home/user/linux"));

        let mut env = MockEnvTrait::new();
        env.expect_which()
            .withf(|name| name == "kw")
            .returning(|_| false);
        let mut fs = MockFileSystemTrait::new();
        fs.expect_is_file().returning(|_| false);
        fs.expect_is_dir().returning(|_| false);
        fs.expect_read_dir().returning(|_| {
            Err(FileSystemError::IoError(io::Error::new(
                io::ErrorKind::NotFound,
                "missing",
            )))
        });
        let mut history = MockKwHistoryStore::new();
        history
            .expect_build_records()
            .withf(|kernel_tree_id, branch| kernel_tree_id == "mainline" && branch == "for-next")
            .times(1)
            .returning(|_, _| Ok((None, None)));
        let mut shell = MockShellTrait::new();
        shell
            .expect_execute()
            .withf(|cmd| {
                cmd.program == "git"
                    && cmd.args == ["-C", "/home/user/linux", "branch", "--show-current"]
            })
            .times(1)
            .returning(|_| {
                Ok(ShellOutput {
                    stdout: b"for-next\n".to_vec(),
                    stderr: Vec::new(),
                    success: true,
                })
            });
        let handle = spawn_test_actor("readiness", history, shell, fs, env);

        let readiness = handle.get_readiness("mainline", &tree).await.unwrap();

        assert!(!readiness.kw_binary.available);
        assert_eq!(TreeReadiness::Missing, readiness.tree);
        assert_eq!(
            Err(DeployAloneRefusal::TreeNotReady(TreeReadiness::Missing)),
            readiness.deploy_alone
        );
        handle.shutdown().await;
    }

    /// Waits until the status leaves `Idle`/`Running` and returns the
    /// terminal status. The receiver may have observed the `Running`
    /// transition first, so a single `changed()` is not enough. The timeout
    /// backstops against a wedged actor; tests that exercise the grace
    /// periods run with paused time instead of waiting them out.
    async fn wait_for_terminal_status(
        watch: &mut watch::Receiver<KwStatusSnapshot>,
    ) -> KwJobStatus {
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let status = watch.borrow().job.clone();
                if !matches!(status, KwJobStatus::Idle | KwJobStatus::Running { .. }) {
                    return status;
                }
                watch.changed().await.unwrap();
            }
        })
        .await
        .expect("status must reach a terminal state")
    }

    #[tokio::test]
    async fn start_build_replies_immediately_and_runs_in_background() {
        let (handle, process, log_dir) = spawn_job_actor("start-immediate");

        // start_build resolves while the spawned process is still running
        // (no finish() was ever signaled).
        let result =
            tokio::time::timeout(Duration::from_secs(1), handle.start_build(start_request()))
                .await
                .expect("start_build must reply immediately");
        result.unwrap();

        let spawned = process.spawned();
        assert_eq!(1, spawned.len());
        assert_eq!("kw", spawned[0].program);
        assert_eq!(["build", "--alert=n"], spawned[0].args.as_slice());
        assert_eq!(Path::new("/home/user/linux"), spawned[0].cwd);
        assert!(spawned[0].log_path.starts_with(&log_dir));

        let snapshot = handle.get_status().await.unwrap();
        assert!(
            matches!(
                snapshot.job,
                KwJobStatus::Running {
                    kind: KwJobKind::Build,
                    phase: KwPhase::Building,
                    ..
                }
            ),
            "unexpected status: {:?}",
            snapshot.job
        );

        process.last_child().finish(0);
        let mut watch = handle.watch_status().await.unwrap();
        let status = wait_for_terminal_status(&mut watch).await;
        assert!(
            matches!(
                status,
                KwJobStatus::Succeeded {
                    kind: KwJobKind::Build,
                    ..
                }
            ),
            "unexpected status: {status:?}"
        );

        handle.shutdown().await;
        std::fs::remove_dir_all(&log_dir).unwrap();
    }

    #[tokio::test]
    async fn second_start_while_running_is_refused() {
        let (handle, process, log_dir) = spawn_job_actor("busy");

        handle.start_build(start_request()).await.unwrap();
        let second = handle.start_build(start_request()).await;

        assert!(matches!(second, Err(KwStartError::JobAlreadyRunning)));

        process.last_child().finish(0);
        handle.shutdown().await;
        std::fs::remove_dir_all(&log_dir).unwrap();
    }

    #[tokio::test]
    async fn failed_build_reports_exit_code_and_log_path() {
        let (handle, process, log_dir) = spawn_job_actor("failed");
        let mut watch = handle.watch_status().await.unwrap();

        handle.start_build(start_request()).await.unwrap();
        process.last_child().finish(2);

        let status = wait_for_terminal_status(&mut watch).await;

        match status {
            KwJobStatus::Failed {
                kind,
                phase,
                exit_code,
                log_path,
            } => {
                assert_eq!(KwJobKind::Build, kind);
                assert_eq!(KwPhase::Building, phase);
                assert_eq!(Some(2), exit_code);
                assert!(log_path.starts_with(&log_dir));
            }
            other => panic!("expected Failed, got {other:?}"),
        }

        handle.shutdown().await;
        std::fs::remove_dir_all(&log_dir).unwrap();
    }

    #[tokio::test]
    async fn cancel_kills_process_group_and_reports_cancelled() {
        let (handle, process, log_dir) = spawn_job_actor("cancel");
        let mut watch = handle.watch_status().await.unwrap();

        handle.start_build(start_request()).await.unwrap();
        // The ack is immediate: process death is observed via the status,
        // not the reply.
        tokio::time::timeout(Duration::from_secs(1), handle.cancel())
            .await
            .expect("cancel must ack immediately")
            .unwrap();

        assert!(process.last_child().was_killed());
        let status = wait_for_terminal_status(&mut watch).await;
        assert_eq!(
            KwJobStatus::Cancelled {
                kind: KwJobKind::Build,
                phase: KwPhase::Building,
            },
            status
        );

        handle.shutdown().await;
        std::fs::remove_dir_all(&log_dir).unwrap();
    }

    #[tokio::test]
    async fn shutdown_kills_running_job() {
        let (handle, process, log_dir) = spawn_job_actor("shutdown-kill");

        handle.start_build(start_request()).await.unwrap();
        // shutdown() returns only after the kill escalation has completed.
        handle.shutdown().await;

        assert!(process.last_child().was_killed());
        std::fs::remove_dir_all(&log_dir).unwrap();
    }

    #[tokio::test]
    async fn spawn_failure_refuses_start_and_stays_idle() {
        let (handle, process, log_dir) = spawn_job_actor("spawn-fail");
        process.refuse_spawns(true);

        let err = handle.start_build(start_request()).await.unwrap_err();

        assert!(matches!(err, KwStartError::Spawn(_)));
        assert_eq!(KwJobStatus::Idle, handle.get_status().await.unwrap().job);
        // A refused start must leave the actor able to accept a later one.
        process.refuse_spawns(false);
        handle.start_build(start_request()).await.unwrap();

        process.last_child().finish(0);
        handle.shutdown().await;
        std::fs::remove_dir_all(&log_dir).unwrap();
    }

    #[tokio::test]
    async fn start_build_refused_when_kw_binary_missing() {
        let process = Arc::new(FakeProcess::new());
        let mut env = MockEnvTrait::new();
        env.expect_which().returning(|_| false);
        let log_dir = tmp_log_dir("kw-missing");
        let handle = KwActor::spawn(
            Arc::new(MockKwHistoryStore::new()),
            process.clone(),
            // No shell or fs calls are expected: the missing binary
            // short-circuits the start before any other probe or spawn.
            Arc::new(MockShellTrait::new()),
            Arc::new(MockFileSystemTrait::new()),
            Arc::new(env),
            log_dir.clone(),
        );

        let err = handle.start_build(start_request()).await.unwrap_err();

        assert!(matches!(err, KwStartError::KwBinaryMissing));
        assert_eq!(KwJobStatus::Idle, handle.get_status().await.unwrap().job);
        assert!(process.spawned().is_empty());

        handle.shutdown().await;
        std::fs::remove_dir_all(&log_dir).unwrap();
    }

    #[tokio::test]
    async fn start_build_refused_when_tree_not_ready() {
        let process = Arc::new(FakeProcess::new());
        let mut env = MockEnvTrait::new();
        env.expect_which().returning(|_| true);
        let mut shell = MockShellTrait::new();
        shell.expect_execute().returning(|_| {
            Ok(ShellOutput {
                stdout: b"kw, version 0.10.0\n".to_vec(),
                stderr: Vec::new(),
                success: true,
            })
        });
        // The kernel-root probes pass, but there is no .kw directory: kw
        // init was never run in this tree.
        let mut fs = MockFileSystemTrait::new();
        fs.expect_is_dir()
            .returning(|path| path.file_name().is_none_or(|name| name != ".kw"));
        fs.expect_is_file()
            .returning(|path| !path.ends_with(".kw/env.current"));
        fs.expect_exists().returning(|_| true);
        let log_dir = tmp_log_dir("tree-not-ready");
        let handle = KwActor::spawn(
            Arc::new(MockKwHistoryStore::new()),
            process.clone(),
            Arc::new(shell),
            Arc::new(fs),
            Arc::new(env),
            log_dir.clone(),
        );

        let err = handle.start_build(start_request()).await.unwrap_err();

        assert!(matches!(
            err,
            KwStartError::TreeNotReady(TreeReadiness::MissingKwDir)
        ));
        assert_eq!(KwJobStatus::Idle, handle.get_status().await.unwrap().job);
        assert!(process.spawned().is_empty());

        handle.shutdown().await;
        std::fs::remove_dir_all(&log_dir).unwrap();
    }

    #[tokio::test]
    async fn start_build_allowed_when_kw_version_below_floor() {
        let process = Arc::new(FakeProcess::new());
        let log_dir = tmp_log_dir("version-below");
        let mut env = MockEnvTrait::new();
        env.expect_which().returning(|_| true);
        let mut shell = MockShellTrait::new();
        shell.expect_execute().returning(|cmd| {
            let stdout = match cmd.program.as_str() {
                // kw's shipped VERSION file is stale (`beta-0.9` even at
                // the 0.10 tag): a below-floor report warns but never
                // gates the start.
                "kw" => b"kw, version beta-0.9\n".to_vec(),
                _ => b"master\n".to_vec(),
            };
            Ok(ShellOutput {
                stdout,
                stderr: Vec::new(),
                success: true,
            })
        });
        let mut fs = MockFileSystemTrait::new();
        expect_ready_tree(&mut fs);
        fs.expect_create_dir_all().returning(|_| Ok(()));
        let handle = KwActor::spawn(
            Arc::new(MockKwHistoryStore::new()),
            process.clone(),
            Arc::new(shell),
            Arc::new(fs),
            Arc::new(env),
            log_dir.clone(),
        );

        handle.start_build(start_request()).await.unwrap();
        assert_eq!(1, process.spawned().len());

        process.last_child().finish(0);
        handle.shutdown().await;
        std::fs::remove_dir_all(&log_dir).unwrap();
    }

    #[tokio::test]
    async fn start_deploy_still_refused_until_the_deploy_step() {
        let handle = spawn_test_actor(
            "deploy-refused",
            MockKwHistoryStore::new(),
            MockShellTrait::new(),
            MockFileSystemTrait::new(),
            MockEnvTrait::new(),
        );

        let result =
            tokio::time::timeout(Duration::from_secs(1), handle.start_deploy(start_request()))
                .await
                .expect("start_deploy must reply immediately");

        assert!(matches!(result, Err(KwStartError::NotImplemented)));
        handle.shutdown().await;
    }

    #[tokio::test]
    async fn cancel_and_restore_without_job_are_immediate_errors() {
        let handle = spawn_test_actor(
            "no-job",
            MockKwHistoryStore::new(),
            MockShellTrait::new(),
            MockFileSystemTrait::new(),
            MockEnvTrait::new(),
        );

        let cancel = tokio::time::timeout(Duration::from_secs(1), handle.cancel())
            .await
            .expect("cancel must reply immediately");
        let restore =
            tokio::time::timeout(Duration::from_secs(1), handle.restore_previous_branch())
                .await
                .expect("restore must reply immediately");

        assert!(matches!(cancel, Err(KwError::NoJobRunning)));
        assert!(matches!(restore, Err(KwError::NoRecordedBranch)));
        handle.shutdown().await;
    }

    #[tokio::test]
    async fn shutdown_stops_actor() {
        let handle = spawn_test_actor(
            "shutdown",
            MockKwHistoryStore::new(),
            MockShellTrait::new(),
            MockFileSystemTrait::new(),
            MockEnvTrait::new(),
        );

        handle.shutdown().await;
        let err = handle.get_status().await.unwrap_err();

        assert!(matches!(err, KwError::ActorUnavailable(_)));
    }

    #[tokio::test]
    async fn log_dir_creation_failure_refuses_start_and_stays_idle() {
        let mut fs = MockFileSystemTrait::new();
        expect_ready_tree(&mut fs);
        fs.expect_create_dir_all().returning(|_| {
            Err(FileSystemError::IoError(io::Error::other(
                "read-only filesystem",
            )))
        });
        let (handle, process, log_dir) = spawn_job_actor_with_fs("log-dir-fail", fs);

        let err = handle.start_build(start_request()).await.unwrap_err();

        assert!(matches!(err, KwStartError::Fs(_)));
        assert_eq!(KwJobStatus::Idle, handle.get_status().await.unwrap().job);
        assert!(process.spawned().is_empty());

        handle.shutdown().await;
        std::fs::remove_dir_all(&log_dir).unwrap();
    }

    #[tokio::test]
    async fn cancel_racing_a_successful_exit_reports_success() {
        let (handle, process, log_dir) = spawn_job_actor("cancel-race");
        let mut watch = handle.watch_status().await.unwrap();

        handle.start_build(start_request()).await.unwrap();
        process.last_child().finish(0);
        // Whether the actor processes the exit or the cancel first is
        // timing-dependent, but the terminal status must be Succeeded
        // either way: the process exited before any signal landed.
        let _ = handle.cancel().await;

        let status = wait_for_terminal_status(&mut watch).await;
        assert!(
            matches!(status, KwJobStatus::Succeeded { .. }),
            "expected Succeeded, got {status:?}"
        );
        // Killing an already-finished process is a no-op, not a kill.
        assert!(!process.last_child().was_killed());

        handle.shutdown().await;
        std::fs::remove_dir_all(&log_dir).unwrap();
    }

    #[tokio::test]
    async fn cancel_racing_a_failed_exit_reports_failure() {
        let (handle, process, log_dir) = spawn_job_actor("cancel-race-fail");
        let mut watch = handle.watch_status().await.unwrap();

        handle.start_build(start_request()).await.unwrap();
        process.last_child().finish(2);
        // Whether the actor processes the exit or the cancel first is
        // timing-dependent, but a plain exit (not signal-terminated) means
        // the process failed on its own before the signal landed: the
        // terminal status must be Failed either way, so the build history
        // records a failure rather than a cancel.
        let _ = handle.cancel().await;

        let status = wait_for_terminal_status(&mut watch).await;
        assert!(
            matches!(
                status,
                KwJobStatus::Failed {
                    exit_code: Some(2),
                    ..
                }
            ),
            "expected Failed(2), got {status:?}"
        );

        handle.shutdown().await;
        std::fs::remove_dir_all(&log_dir).unwrap();
    }

    // Paused time: the runtime auto-advances through the grace-period
    // timers instead of burning wall-clock seconds on them.
    #[tokio::test(start_paused = true)]
    async fn cancel_escalates_to_sigkill_when_sigterm_is_ignored() {
        let (handle, process, log_dir) = spawn_job_actor("sigkill-escalation");
        process.ignore_sigterm(true);
        let mut watch = handle.watch_status().await.unwrap();

        handle.start_build(start_request()).await.unwrap();
        handle.cancel().await.unwrap();

        let status = wait_for_terminal_status(&mut watch).await;
        assert!(
            matches!(status, KwJobStatus::Cancelled { .. }),
            "expected Cancelled, got {status:?}"
        );
        let child = process.last_child();
        assert!(child.was_killed());
        assert!(child.was_force_killed());

        handle.shutdown().await;
        std::fs::remove_dir_all(&log_dir).unwrap();
    }
}
