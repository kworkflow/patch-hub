//! kw actor: owns kw build/deploy job state and the kw history store.
//!
//! All kw operations go through [`KwHandle`](crate::kw::handle::KwHandle) as
//! typed request/reply messages. `Start*` messages reply immediately with an
//! accept/refuse verdict — a job accepted by the actor keeps running after
//! the caller has been answered, so the AppActor loop never blocks on a
//! kernel build. The actor composes the readiness probes from
//! [`crate::kw::readiness`] and records applies through the shared
//! [`KwHistoryStore`](crate::kw::history::KwHistoryStore).

// No production caller until the actor is wired into the app; the allow
// marks the module as a live root so the message/status/handle types it
// references stay live too. Removed once main.rs spawns the actor. Kept
// per the CachePolicy precedent (src/lore/application/cache.rs).
#![allow(dead_code)]

use std::{ops::ControlFlow, path::PathBuf, process::ExitStatus, sync::Arc};

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
        readiness::{self, KwReadiness},
        status::{KwJobKind, KwJobStatus, KwPhase, KwStatusSnapshot},
    },
};

pub const DEFAULT_KW_CHANNEL_SIZE: usize = 16;

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

    #[allow(clippy::too_many_arguments)]
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
                    if let ControlFlow::Break(()) = self.handle_message(message) {
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

    fn handle_message(&mut self, message: KwMessage) -> ControlFlow<()> {
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
            // Deploy acceptance lands with the deploy step; the
            // immediate-reply contract already holds, so callers never
            // learn to depend on a blocking reply.
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
            KwMessage::Shutdown => {
                // Quit-prompt cooperation: the app asks whether a job is
                // running before quitting; if it quits anyway, the job's
                // process group is killed here. The log may capture partial
                // output and the tree a partial build.
                if let Some(cancel) = self.job.as_mut().and_then(|job| job.cancel_tx.take()) {
                    tracing::info!("killing kw job process group during shutdown");
                    let _ = cancel.send(());
                }
                tracing::debug!("kw actor shutting down");
                ControlFlow::Break(())
            }
        }
    }

    /// Accepts and starts a build job, or refuses. The reply is sent by the
    /// caller right after this returns: the job itself keeps running in a
    /// detached task and is observed via the status snapshot.
    ///
    /// The argv is the skeleton's minimal `kw build --alert=n`; the real
    /// argv builder (reserved flags, extra-args merge) and the checkout
    /// policy land with the build step.
    fn start_job(&mut self, kind: KwJobKind, request: StartRequest) -> Result<(), KwStartError> {
        if self.job.is_some() {
            return Err(KwStartError::JobAlreadyRunning);
        }

        self.fs.create_dir_all(&self.kw_log_dir)?;
        let log_path = self.kw_log_dir.join(format!(
            "build-{}.log",
            chrono::Utc::now().format("%Y%m%d-%H%M%S")
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
                        tracing::info!(branch = job.branch, "kw job succeeded");
                        KwJobStatus::Succeeded { kind: job.kind }
                    }
                    JobOutcome::Exited(exit) => {
                        tracing::warn!(
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
/// cancel signal fires — kills the whole process group and reaps it. The
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
        _ = &mut cancel_rx => {
            if let Err(error) = process.kill() {
                tracing::warn!(%error, "failed to kill kw job process group");
            }
            let _ = process.wait().await;
            JobOutcome::Cancelled
        }
    };
    events.send(JobEvent::Finished(outcome)).await.ok();
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
    /// behind `Arc`s).
    fn spawn_test_actor(
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
            PathBuf::from("/tmp/patch-hub-test-kw-logs"),
        )
    }

    /// Spawns the actor with a real temp log dir and exposes the
    /// [`FakeProcess`] so tests drive the "running" process.
    fn spawn_job_actor(test_name: &str) -> (KwHandle, Arc<FakeProcess>, PathBuf) {
        let process = Arc::new(FakeProcess::new());
        let log_dir = tmp_log_dir(test_name);
        let mut fs = MockFileSystemTrait::new();
        fs.expect_create_dir_all().returning(|_| Ok(()));
        let handle = KwActor::spawn(
            Arc::new(MockKwHistoryStore::new()),
            process.clone(),
            Arc::new(MockShellTrait::new()),
            Arc::new(fs),
            Arc::new(MockEnvTrait::new()),
            log_dir.clone(),
        );
        (handle, process, log_dir)
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
        let handle = spawn_test_actor(history, shell, fs, env);

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
    /// transition first, so a single `changed()` is not enough.
    async fn wait_for_terminal_status(
        watch: &mut watch::Receiver<KwStatusSnapshot>,
    ) -> KwJobStatus {
        tokio::time::timeout(Duration::from_secs(1), async {
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

        // The §3.1 reply contract: start_build resolves while the spawned
        // process is still running (no finish() was ever signaled).
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
        assert_eq!(
            KwJobStatus::Succeeded {
                kind: KwJobKind::Build
            },
            status
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
        handle.shutdown().await;

        // shutdown() only enqueues the message; the actor being gone
        // proves the Shutdown (and its kill) was processed.
        let err = handle.get_status().await.unwrap_err();
        assert!(matches!(err, KwError::ActorUnavailable(_)));
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
    async fn start_deploy_still_refused_until_the_deploy_step() {
        let handle = spawn_test_actor(
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
            MockKwHistoryStore::new(),
            MockShellTrait::new(),
            MockFileSystemTrait::new(),
            MockEnvTrait::new(),
        );

        handle.shutdown().await;
        let err = handle.get_status().await.unwrap_err();

        assert!(matches!(err, KwError::ActorUnavailable(_)));
    }
}
