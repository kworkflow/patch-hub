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
//! Tracked note (same class as the integration plan's §2.1i): the actor
//! runs quick blocking calls inline in its async task — `create_dir_all`,
//! the git probes of the checkout policy, the build-record completion
//! probes, and the history store's atomic writes — per the ConfigActor
//! precedent. They are all milliseconds-scale; if a real stall ever shows
//! up while a long job runs, they should move behind `spawn_blocking`
//! like the other actors' heavy work.

use std::{
    ops::ControlFlow,
    path::{Path, PathBuf},
    process::ExitStatus,
    sync::Arc,
    time::Duration,
};

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
        argv,
        errors::{KwError, KwStartError, TreeGitError},
        handle::KwHandle,
        history::{KwBuildRecord, KwHistoryStore},
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

/// Where RestorePreviousBranch switches back to: the branch HEAD was on
/// when the last job was accepted, and the tree that branch lives in.
/// Session-only (integration plan §2.1a), deliberately not persisted.
struct RestoreContext {
    tree_path: String,
    branch: String,
}

/// What the actor remembers about the running job while the detached task
/// owns the process itself (see [`run_job`]).
struct JobState {
    kind: KwJobKind,
    phase: KwPhase,
    kernel_tree_id: String,
    branch: String,
    /// Snapshot of the tree path at accept time: the build record carries
    /// it so later readiness checks can detect the tree being repointed
    /// (§2.1e).
    tree_path: String,
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
    /// Recorded only when a job is accepted: a refused start never
    /// clobbers a previous job's restore target.
    last_restore: Option<RestoreContext>,
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
            last_restore: None,
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
            KwMessage::RestorePreviousBranch { reply } => {
                send_kw_reply(message_name, reply, self.restore_previous_branch());
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
    /// unresolvable kw-env state, a tree that fails the readiness probes,
    /// a dirty worktree, or a failed branch switch. The argv comes from
    /// the reserved-flags merge (§2.1f): patch-hub's own flags win over
    /// the request's extra args.
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

        // Refuse on a dirty worktree before touching anything (§2.1a): a
        // switch could otherwise carry unrelated changes into the build
        // branch.
        self.check_worktree_clean(request.tree.path())?;

        // Probed before anything touches the tree: the `git switch` below
        // goes between this probe and the spawn, and the probe must still
        // capture the pre-job HEAD or RestorePreviousBranch would
        // "restore" the branch the job switched to. An unprobed HEAD
        // (detached, or not a git repo) records nothing rather than a
        // wrong branch.
        let pre_job_branch = match self.head_branch(&request.tree) {
            branch if branch.is_empty() => None,
            branch => Some(branch),
        };

        // The checkout policy (§2.1a): the job runs on the requested
        // branch, and HEAD stays there after the job.
        self.switch_to_branch(request.tree.path(), &request.branch)?;

        self.fs.create_dir_all(&self.kw_log_dir)?;
        // Millisecond suffix: two jobs started within the same second must
        // not share a log file — spawn truncates it.
        let log_path = self.kw_log_dir.join(format!(
            "build-{}.log",
            chrono::Utc::now().format("%Y%m%d-%H%M%S-%3f")
        ));
        let cmd = ShellCommand::new("kw").args(argv::build_argv(&request.extra_args));
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
        // Recorded only on accept: a refused start — including one refused
        // after the switch (log-dir creation, spawn) — never clobbers a
        // previous job's restore target. An accepted job with an unprobed
        // pre-job HEAD clears it: nothing honest is left to restore to.
        self.last_restore = pre_job_branch.map(|branch| RestoreContext {
            tree_path: request.tree.path().to_string(),
            branch,
        });
        self.job = Some(JobState {
            kind,
            phase,
            kernel_tree_id: request.kernel_tree_id.clone(),
            branch: request.branch.clone(),
            tree_path: request.tree.path().to_string(),
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
                // The record is written before the status flips: watchers
                // that react to the terminal status find the history
                // already durable.
                self.record_build_outcome(&job, &outcome);
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

    /// Writes the build record for a finished job (§2.1e): success and
    /// failure both — KwOps shows "last build failed" from the stored
    /// record, and deploy-alone readiness requires `success == true`. A
    /// cancelled job writes nothing: it never completed. History and
    /// patchset-link errors are logged, never reported in the job's
    /// status — the build's real outcome already reached the user.
    ///
    /// Build-then-deploy jobs (the deploy step) must instead write this
    /// record at the Building → Deploying phase transition, so a failed
    /// deploy cannot mask a good build.
    fn record_build_outcome(&self, job: &JobState, outcome: &JobOutcome) {
        let success = match outcome {
            JobOutcome::Exited(exit) => exit.success(),
            // The exit status is lost: record an honest failure rather
            // than guess, so deploy-alone readiness cannot trust it.
            JobOutcome::WaitFailed(_) => false,
            JobOutcome::Cancelled => return,
        };

        let tree_path = Path::new(&job.tree_path);
        // If the env state became unresolvable mid-build, the record
        // would be keyed with a wrong `output_dir: None` — a Frankenstein
        // match for a later no-env deploy-alone probe. No record fails
        // safe.
        let output_dir = match readiness::resolve_output_dir(&*self.fs, &*self.env, tree_path) {
            Ok(output_dir) => output_dir,
            Err(error) => {
                tracing::warn!(
                    %error,
                    branch = job.branch,
                    "skipping the build record: kw env state unresolvable"
                );
                return;
            }
        };
        let arch = readiness::read_build_arch(&*self.fs, tree_path);
        // A failed build may have left a stale image from an earlier
        // successful one behind; only successes record what they
        // produced.
        let (image_path, kernelrelease) = if success {
            let build_root = output_dir.as_deref().unwrap_or(tree_path);
            (
                readiness::find_newest_kernel_image(&*self.fs, build_root, arch.as_deref()),
                readiness::read_kernelrelease(&*self.fs, build_root),
            )
        } else {
            (None, None)
        };
        let message_id = match self
            .history
            .apply_record_for_branch(&job.kernel_tree_id, &job.branch)
        {
            Ok(record) => record.map(|record| record.message_id),
            Err(error) => {
                tracing::warn!(
                    %error,
                    branch = job.branch,
                    "build record loses its patchset link"
                );
                None
            }
        };

        let record = KwBuildRecord {
            kernel_tree_id: job.kernel_tree_id.clone(),
            tree_path: job.tree_path.clone(),
            message_id,
            branch: job.branch.clone(),
            arch,
            image_path: image_path.map(|path| path.to_string_lossy().into_owned()),
            output_dir: output_dir.map(|path| path.to_string_lossy().into_owned()),
            kernelrelease,
            log_path: job.log_path.to_string_lossy().into_owned(),
            built_at: chrono::Utc::now().to_rfc3339(),
            success,
        };
        if let Err(error) = self.history.record_build(record) {
            tracing::warn!(%error, branch = job.branch, "failed to record kw build history");
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

    /// Fails unless the tree's git state verifies clean (§2.1a). A probe
    /// that itself fails — git missing, not a repository — fails too:
    /// starting a job or restoring a branch on a tree whose state is
    /// unknown could carry unrecorded changes across branches.
    fn check_worktree_clean(&self, tree_path: &str) -> Result<(), TreeGitError> {
        let cmd = ShellCommand::new("git").args(["-C", tree_path, "status", "--porcelain"]);
        let output = self
            .shell
            .execute(&cmd)
            .map_err(|error| TreeGitError::Probe(error.to_string()))?;
        if !output.success {
            return Err(TreeGitError::Probe(
                String::from_utf8_lossy(&output.stderr).trim().to_string(),
            ));
        }
        if !output.stdout.is_empty() {
            return Err(TreeGitError::DirtyWorktree);
        }
        Ok(())
    }

    /// Switches the tree onto `branch`. A failure carries git's stderr,
    /// which names the usual causes (no such branch, a rebase or merge in
    /// progress).
    fn switch_to_branch(&self, tree_path: &str, branch: &str) -> Result<(), TreeGitError> {
        let cmd = ShellCommand::new("git").args(["-C", tree_path, "switch", branch]);
        let output = self
            .shell
            .execute(&cmd)
            .map_err(|error| TreeGitError::Switch(error.to_string()))?;
        if !output.success {
            return Err(TreeGitError::Switch(
                String::from_utf8_lossy(&output.stderr).trim().to_string(),
            ));
        }
        Ok(())
    }

    /// Switches the tree that ran the last job back to the branch HEAD was
    /// on when that job was accepted (§2.1a). Refuses while a job is
    /// running (its branch is in use), when nothing was recorded, and on
    /// a dirty worktree. Only a successful switch consumes the context —
    /// a refused restore stays available for a retry.
    fn restore_previous_branch(&mut self) -> Result<(), KwError> {
        if self.job.is_some() {
            return Err(KwError::JobRunning);
        }
        let Some(restore) = self.last_restore.take() else {
            return Err(KwError::NoRecordedBranch);
        };
        match self
            .check_worktree_clean(&restore.tree_path)
            .and_then(|()| self.switch_to_branch(&restore.tree_path, &restore.branch))
        {
            Ok(()) => {
                tracing::info!(
                    tree = restore.tree_path,
                    branch = restore.branch,
                    "restored pre-job branch"
                );
                Ok(())
            }
            Err(error) => {
                self.last_restore = Some(restore);
                Err(error.into())
            }
        }
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
        sync::{
            atomic::{AtomicBool, AtomicU64, Ordering},
            Mutex,
        },
        time::Duration,
    };

    use crate::{
        infrastructure::{
            env::MockEnvTrait,
            file_system::{FileSystemError, MockFileSystemTrait},
            process::FakeProcess,
            shell::{MockShellTrait, ShellCommand, ShellOutput},
        },
        kw::{
            errors::KwStartError,
            history::{KwApplyRecord, KwBuildRecord, MockKwHistoryStore},
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
            extra_args: Vec::new(),
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

    const KW_VERSION_OK: &[u8] = b"kw, version 0.10.0\n";
    /// A clean `git status --porcelain` answer.
    const CLEAN_STATUS: (&[u8], &[u8], bool) = (b"", b"", true);
    /// A successful `git switch` answer.
    const SWITCH_OK: (&[u8], bool) = (b"", true);

    fn command_parts(cmd: &ShellCommand) -> Vec<String> {
        let mut parts = vec![cmd.program.clone()];
        parts.extend(cmd.args.clone());
        parts
    }

    fn command(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|part| part.to_string()).collect()
    }

    /// A shell mock that logs every command's argv parts and answers by
    /// content: kw's version probe with `kw_version`; `git status
    /// --porcelain` with `status` (stdout, stderr, success); `git switch`
    /// with `switch` (stderr, success); any other git call — the HEAD
    /// branch probe — with `master`.
    fn recording_shell(
        kw_version: &'static [u8],
        status: (&'static [u8], &'static [u8], bool),
        switch: (&'static [u8], bool),
    ) -> (MockShellTrait, Arc<Mutex<Vec<Vec<String>>>>) {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let calls_in_shell = Arc::clone(&calls);
        let mut shell = MockShellTrait::new();
        shell.expect_execute().returning(move |cmd| {
            calls_in_shell.lock().unwrap().push(command_parts(cmd));
            let output = |stdout: &[u8], stderr: &[u8], success: bool| ShellOutput {
                stdout: stdout.to_vec(),
                stderr: stderr.to_vec(),
                success,
            };
            if cmd.program == "kw" {
                return Ok(output(kw_version, b"", true));
            }
            if cmd.args.iter().any(|arg| arg == "status") {
                return Ok(output(status.0, status.1, status.2));
            }
            if cmd.args.iter().any(|arg| arg == "switch") {
                return Ok(output(b"", switch.0, switch.1));
            }
            Ok(output(b"master\n", b"", true))
        });
        (shell, calls)
    }

    /// A stateful shell double for the restore tests: kw's version probe
    /// answers 0.10.0, `git status --porcelain` reflects the dirty flag,
    /// the HEAD probe reports `head` (with the trailing newline git
    /// prints), and a successful `git switch` updates `head` — mirroring
    /// a real worktree the actor switches between branches.
    struct GitStub {
        head: Arc<Mutex<String>>,
        dirty: Arc<AtomicBool>,
        fail_switches: Arc<AtomicBool>,
    }

    impl GitStub {
        fn on_branch(branch: &str) -> Self {
            Self {
                head: Arc::new(Mutex::new(branch.to_string())),
                dirty: Arc::new(AtomicBool::new(false)),
                fail_switches: Arc::new(AtomicBool::new(false)),
            }
        }

        fn set_dirty(&self, dirty: bool) {
            self.dirty.store(dirty, Ordering::Relaxed);
        }

        fn set_fail_switches(&self, fail: bool) {
            self.fail_switches.store(fail, Ordering::Relaxed);
        }

        fn head(&self) -> String {
            self.head.lock().unwrap().clone()
        }

        fn shell(&self) -> MockShellTrait {
            let head = Arc::clone(&self.head);
            let dirty = Arc::clone(&self.dirty);
            let fail_switches = Arc::clone(&self.fail_switches);
            let mut shell = MockShellTrait::new();
            shell.expect_execute().returning(move |cmd| {
                let output = |stdout: &[u8]| ShellOutput {
                    stdout: stdout.to_vec(),
                    stderr: Vec::new(),
                    success: true,
                };
                if cmd.program == "kw" {
                    return Ok(output(KW_VERSION_OK));
                }
                if cmd.args.iter().any(|arg| arg == "status") {
                    let stdout: &[u8] = if dirty.load(Ordering::Relaxed) {
                        b" M src/main.c\n"
                    } else {
                        b""
                    };
                    return Ok(output(stdout));
                }
                if cmd.args.iter().any(|arg| arg == "switch") {
                    if fail_switches.load(Ordering::Relaxed) {
                        return Ok(ShellOutput {
                            stdout: Vec::new(),
                            stderr: b"error: you need to resolve your current index first\n"
                                .to_vec(),
                            success: false,
                        });
                    }
                    *head.lock().unwrap() = cmd.args.last().unwrap().clone();
                    return Ok(output(b""));
                }
                let current = format!("{}\n", head.lock().unwrap());
                Ok(output(current.as_bytes()))
            });
            shell
        }
    }

    /// fs answers for a ready kernel tree with no active kw env: the
    /// kernel-root probes pass, `.config` exists, `.kw/env.current` is
    /// absent, `.kw/build.config` is unreadable (arch probes as None),
    /// and there is no arch/ dir to glob images from.
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
        fs.expect_read_dir().returning(|_| {
            Err(FileSystemError::IoError(io::Error::new(
                io::ErrorKind::NotFound,
                "missing",
            )))
        });
    }

    /// A ready kernel tree whose log dir can be created.
    fn ready_fs() -> MockFileSystemTrait {
        let mut fs = MockFileSystemTrait::new();
        expect_ready_tree(&mut fs);
        fs.expect_create_dir_all().returning(|_| Ok(()));
        fs
    }

    /// A ready kernel tree whose build produced an image and a
    /// kernelrelease: build.config sets `arch=x86`, `arch/x86/boot/`
    /// holds a bzImage, and `include/config/kernel.release` exists. The
    /// image's metadata is unreadable, so its mtime falls back to the
    /// epoch — still the only, hence newest, candidate.
    fn built_tree_fs() -> MockFileSystemTrait {
        let mut fs = MockFileSystemTrait::new();
        fs.expect_is_dir().returning(|_| true);
        fs.expect_is_file()
            .returning(|path| !path.ends_with(".kw/env.current"));
        fs.expect_exists().returning(|_| true);
        fs.expect_read_to_string().returning(|path| {
            if path.ends_with("build.config") {
                Ok("arch=x86\n".to_string())
            } else if path.ends_with("kernel.release") {
                Ok("6.17.0\n".to_string())
            } else {
                Err(FileSystemError::IoError(io::Error::new(
                    io::ErrorKind::NotFound,
                    "missing",
                )))
            }
        });
        fs.expect_read_dir().returning(|path| {
            if path.ends_with("arch/x86/boot") {
                Ok(vec![PathBuf::from(
                    "/home/user/linux/arch/x86/boot/bzImage",
                )])
            } else {
                Err(FileSystemError::IoError(io::Error::new(
                    io::ErrorKind::NotFound,
                    "missing",
                )))
            }
        });
        fs.expect_metadata()
            .returning(|_| Err(FileSystemError::IoError(io::Error::other("no metadata"))));
        fs.expect_create_dir_all().returning(|_| Ok(()));
        fs
    }

    /// History answers for an actor whose jobs complete: no patchset
    /// link, build-record writes accepted and dropped.
    fn quiet_history() -> MockKwHistoryStore {
        let mut history = MockKwHistoryStore::new();
        history
            .expect_apply_record_for_branch()
            .returning(|_, _| Ok(None));
        history.expect_record_build().returning(|_| Ok(()));
        history
    }

    /// A history double that captures written build records and answers
    /// the patchset-link lookup with `apply_record`.
    fn recording_history(
        apply_record: Option<KwApplyRecord>,
    ) -> (MockKwHistoryStore, Arc<Mutex<Vec<KwBuildRecord>>>) {
        let builds = Arc::new(Mutex::new(Vec::new()));
        let builds_in_store = Arc::clone(&builds);
        let mut history = MockKwHistoryStore::new();
        history
            .expect_apply_record_for_branch()
            .returning(move |_, _| Ok(apply_record.clone()));
        history.expect_record_build().returning(move |record| {
            builds_in_store.lock().unwrap().push(record);
            Ok(())
        });
        (history, builds)
    }

    /// Spawns the actor with every dependency explicit, a real temp log
    /// dir, and the [`FakeProcess`] exposed so tests drive the "running"
    /// process.
    fn spawn_full_actor(
        test_name: &str,
        history: MockKwHistoryStore,
        shell: MockShellTrait,
        fs: MockFileSystemTrait,
        env: MockEnvTrait,
    ) -> (KwHandle, Arc<FakeProcess>, PathBuf) {
        let process = Arc::new(FakeProcess::new());
        let log_dir = tmp_log_dir(test_name);
        let handle = KwActor::spawn(
            Arc::new(history),
            process.clone(),
            Arc::new(shell),
            Arc::new(fs),
            Arc::new(env),
            log_dir.clone(),
        );
        (handle, process, log_dir)
    }

    /// Spawns the actor with a real temp log dir and exposes the
    /// [`FakeProcess`] so tests drive the "running" process. The env mock
    /// has kw on PATH.
    fn spawn_job_actor_with_mocks(
        test_name: &str,
        shell: MockShellTrait,
        fs: MockFileSystemTrait,
    ) -> (KwHandle, Arc<FakeProcess>, PathBuf) {
        let mut env = MockEnvTrait::new();
        env.expect_which().returning(|_| true);
        spawn_full_actor(test_name, quiet_history(), shell, fs, env)
    }

    fn spawn_job_actor(test_name: &str) -> (KwHandle, Arc<FakeProcess>, PathBuf) {
        let (shell, _calls) = recording_shell(KW_VERSION_OK, CLEAN_STATUS, SWITCH_OK);
        spawn_job_actor_with_mocks(test_name, shell, ready_fs())
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
    async fn start_build_merges_extra_args_into_the_spawned_argv() {
        let (handle, process, log_dir) = spawn_job_actor("extra-args");

        let mut request = start_request();
        request.extra_args = [
            "--verbose",
            "--alert=vv",
            "--save-log-to",
            "/tmp/x.log",
            "--ccache",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        handle.start_build(request).await.unwrap();

        let spawned = process.spawned();
        // Reserved options win: the user's --alert and --save-log-to are
        // stripped, the rest passes through in order.
        assert_eq!(
            ["build", "--alert=n", "--verbose", "--ccache"].as_slice(),
            spawned[0].args.as_slice()
        );

        process.last_child().finish(0);
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
        // kw's shipped VERSION file is stale (`beta-0.9` even at the 0.10
        // tag): a below-floor report warns but never gates the start.
        let (shell, _calls) = recording_shell(b"kw, version beta-0.9\n", CLEAN_STATUS, SWITCH_OK);
        let (handle, process, log_dir) =
            spawn_job_actor_with_mocks("version-below", shell, ready_fs());

        handle.start_build(start_request()).await.unwrap();
        assert_eq!(1, process.spawned().len());

        process.last_child().finish(0);
        handle.shutdown().await;
        std::fs::remove_dir_all(&log_dir).unwrap();
    }

    #[tokio::test]
    async fn start_build_switches_to_requested_branch_before_spawning() {
        let (shell, calls) = recording_shell(KW_VERSION_OK, CLEAN_STATUS, SWITCH_OK);
        let (handle, process, log_dir) =
            spawn_job_actor_with_mocks("checkout-order", shell, ready_fs());

        handle.start_build(start_request()).await.unwrap();

        {
            let calls = calls.lock().unwrap();
            let git_position = |subcommand: &str| {
                calls
                    .iter()
                    .position(|call| {
                        call.first().map(String::as_str) == Some("git")
                            && call.iter().any(|part| part == subcommand)
                    })
                    .expect("expected git call missing")
            };
            let status = git_position("status");
            let head = git_position("--show-current");
            let switch = git_position("switch");
            assert!(
                status < head && head < switch,
                "checkout policy must probe dirty state, then HEAD, then switch: {calls:?}"
            );
            assert_eq!(
                &calls[switch],
                &command(&[
                    "git",
                    "-C",
                    "/home/user/linux",
                    "switch",
                    "patchset-2026-08-01-17-30-00"
                ])
            );
        }
        assert_eq!(1, process.spawned().len());

        process.last_child().finish(0);
        handle.shutdown().await;
        std::fs::remove_dir_all(&log_dir).unwrap();
    }

    #[tokio::test]
    async fn start_build_refused_when_worktree_is_dirty() {
        let (shell, calls) =
            recording_shell(KW_VERSION_OK, (b" M src/main.c\n", b"", true), SWITCH_OK);
        let mut fs = MockFileSystemTrait::new();
        expect_ready_tree(&mut fs);
        let (handle, process, log_dir) = spawn_job_actor_with_mocks("dirty", shell, fs);

        let err = handle.start_build(start_request()).await.unwrap_err();

        assert!(matches!(err, KwStartError::DirtyWorktree));
        assert_eq!(KwJobStatus::Idle, handle.get_status().await.unwrap().job);
        assert!(process.spawned().is_empty());
        // The refusal happens before any branch mutation.
        assert!(!calls
            .lock()
            .unwrap()
            .iter()
            .any(|call| call.iter().any(|part| part == "switch")));

        handle.shutdown().await;
        std::fs::remove_dir_all(&log_dir).unwrap();
    }

    #[tokio::test]
    async fn start_build_refused_when_git_state_is_unverifiable() {
        let (shell, _calls) = recording_shell(
            KW_VERSION_OK,
            (b"", b"fatal: not a git repository\n", false),
            SWITCH_OK,
        );
        let mut fs = MockFileSystemTrait::new();
        expect_ready_tree(&mut fs);
        let (handle, process, log_dir) = spawn_job_actor_with_mocks("git-probe-fail", shell, fs);

        let err = handle.start_build(start_request()).await.unwrap_err();

        assert!(matches!(err, KwStartError::GitStateProbe(_)));
        assert!(err.to_string().contains("not a git repository"));
        assert_eq!(KwJobStatus::Idle, handle.get_status().await.unwrap().job);
        assert!(process.spawned().is_empty());

        handle.shutdown().await;
        std::fs::remove_dir_all(&log_dir).unwrap();
    }

    #[tokio::test]
    async fn start_build_refused_when_branch_switch_fails() {
        let (shell, _calls) = recording_shell(
            KW_VERSION_OK,
            CLEAN_STATUS,
            (
                b"error: pathspec 'no-such-branch' did not match any file(s) known to git\n",
                false,
            ),
        );
        let (handle, process, log_dir) =
            spawn_job_actor_with_mocks("switch-fail", shell, ready_fs());

        let err = handle.start_build(start_request()).await.unwrap_err();

        assert!(matches!(err, KwStartError::CheckoutFailed(_)));
        assert!(err.to_string().contains("did not match"));
        assert_eq!(KwJobStatus::Idle, handle.get_status().await.unwrap().job);
        assert!(process.spawned().is_empty());

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
    async fn restore_switches_back_to_pre_job_branch_and_is_consumed() {
        let git = GitStub::on_branch("master");
        let (handle, process, log_dir) =
            spawn_job_actor_with_mocks("restore", git.shell(), ready_fs());
        let mut watch = handle.watch_status().await.unwrap();

        handle.start_build(start_request()).await.unwrap();
        // The checkout policy left HEAD on the build branch.
        assert_eq!(git.head(), "patchset-2026-08-01-17-30-00");
        process.last_child().finish(0);
        let status = wait_for_terminal_status(&mut watch).await;
        assert!(matches!(status, KwJobStatus::Succeeded { .. }));

        handle.restore_previous_branch().await.unwrap();
        assert_eq!(git.head(), "master");

        // A successful restore consumes the context: a second restore has
        // nothing to do.
        let err = handle.restore_previous_branch().await.unwrap_err();
        assert!(matches!(err, KwError::NoRecordedBranch));

        handle.shutdown().await;
        std::fs::remove_dir_all(&log_dir).unwrap();
    }

    #[tokio::test]
    async fn restore_refused_while_job_is_running() {
        let git = GitStub::on_branch("master");
        let (handle, process, log_dir) =
            spawn_job_actor_with_mocks("restore-running", git.shell(), ready_fs());

        handle.start_build(start_request()).await.unwrap();
        let err = handle.restore_previous_branch().await.unwrap_err();
        assert!(matches!(err, KwError::JobRunning));

        // The context survives the refusal: restore works once the job
        // ends.
        process.last_child().finish(0);
        let mut watch = handle.watch_status().await.unwrap();
        let _ = wait_for_terminal_status(&mut watch).await;
        handle.restore_previous_branch().await.unwrap();
        assert_eq!(git.head(), "master");

        handle.shutdown().await;
        std::fs::remove_dir_all(&log_dir).unwrap();
    }

    #[tokio::test]
    async fn restore_refused_when_worktree_is_dirty() {
        let git = GitStub::on_branch("master");
        let (handle, process, log_dir) =
            spawn_job_actor_with_mocks("restore-dirty", git.shell(), ready_fs());
        let mut watch = handle.watch_status().await.unwrap();

        handle.start_build(start_request()).await.unwrap();
        process.last_child().finish(0);
        let _ = wait_for_terminal_status(&mut watch).await;

        git.set_dirty(true);
        let err = handle.restore_previous_branch().await.unwrap_err();
        assert!(matches!(err, KwError::DirtyWorktree));
        // The refused restore did not touch the tree.
        assert_eq!(git.head(), "patchset-2026-08-01-17-30-00");

        // The context survives: clean the tree and retry.
        git.set_dirty(false);
        handle.restore_previous_branch().await.unwrap();
        assert_eq!(git.head(), "master");

        handle.shutdown().await;
        std::fs::remove_dir_all(&log_dir).unwrap();
    }

    #[tokio::test]
    async fn restore_failure_keeps_the_context_for_a_retry() {
        let git = GitStub::on_branch("master");
        let (handle, process, log_dir) =
            spawn_job_actor_with_mocks("restore-fail", git.shell(), ready_fs());
        let mut watch = handle.watch_status().await.unwrap();

        handle.start_build(start_request()).await.unwrap();
        process.last_child().finish(0);
        let _ = wait_for_terminal_status(&mut watch).await;

        git.set_fail_switches(true);
        let err = handle.restore_previous_branch().await.unwrap_err();
        assert!(matches!(err, KwError::CheckoutFailed(_)));
        assert!(err.to_string().contains("resolve your current index"));

        git.set_fail_switches(false);
        handle.restore_previous_branch().await.unwrap();
        assert_eq!(git.head(), "master");

        handle.shutdown().await;
        std::fs::remove_dir_all(&log_dir).unwrap();
    }

    #[tokio::test]
    async fn refused_start_does_not_clobber_the_restore_context() {
        let git = GitStub::on_branch("master");
        let (handle, process, log_dir) =
            spawn_job_actor_with_mocks("restore-clobber", git.shell(), ready_fs());
        let mut watch = handle.watch_status().await.unwrap();

        handle.start_build(start_request()).await.unwrap();
        process.last_child().finish(0);
        let _ = wait_for_terminal_status(&mut watch).await;
        assert_eq!(git.head(), "patchset-2026-08-01-17-30-00");

        // This start is refused at spawn — after its HEAD probe and
        // switch — and must not overwrite the recorded restore target.
        process.refuse_spawns(true);
        let mut second = start_request();
        second.branch = "patchset-two".to_string();
        let err = handle.start_build(second).await.unwrap_err();
        assert!(matches!(err, KwStartError::Spawn(_)));
        assert_eq!(git.head(), "patchset-two");

        handle.restore_previous_branch().await.unwrap();
        assert_eq!(git.head(), "master");

        handle.shutdown().await;
        std::fs::remove_dir_all(&log_dir).unwrap();
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
        let (shell, _calls) = recording_shell(KW_VERSION_OK, CLEAN_STATUS, SWITCH_OK);
        let mut fs = MockFileSystemTrait::new();
        expect_ready_tree(&mut fs);
        fs.expect_create_dir_all().returning(|_| {
            Err(FileSystemError::IoError(io::Error::other(
                "read-only filesystem",
            )))
        });
        let (handle, process, log_dir) = spawn_job_actor_with_mocks("log-dir-fail", shell, fs);

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

    #[tokio::test]
    async fn successful_build_writes_a_full_build_record() {
        let (history, builds) = recording_history(Some(apply_record()));
        let (shell, _calls) = recording_shell(KW_VERSION_OK, CLEAN_STATUS, SWITCH_OK);
        let (handle, process, log_dir) =
            spawn_full_actor("build-record", history, shell, built_tree_fs(), {
                let mut env = MockEnvTrait::new();
                env.expect_which().returning(|_| true);
                env
            });
        let mut watch = handle.watch_status().await.unwrap();

        handle.start_build(start_request()).await.unwrap();
        process.last_child().finish(0);
        let status = wait_for_terminal_status(&mut watch).await;
        assert!(matches!(status, KwJobStatus::Succeeded { .. }));

        {
            let builds = builds.lock().unwrap();
            assert_eq!(1, builds.len());
            let record = &builds[0];
            assert_eq!("mainline", record.kernel_tree_id);
            assert_eq!("/home/user/linux", record.tree_path);
            assert_eq!(Some("msg-1"), record.message_id.as_deref());
            assert_eq!("patchset-2026-08-01-17-30-00", record.branch);
            assert_eq!(Some("x86"), record.arch.as_deref());
            assert_eq!(
                Some("/home/user/linux/arch/x86/boot/bzImage"),
                record.image_path.as_deref()
            );
            assert_eq!(None, record.output_dir);
            assert_eq!(Some("6.17.0"), record.kernelrelease.as_deref());
            assert!(record.log_path.starts_with(log_dir.to_str().unwrap()));
            assert!(record.success);
            // The readiness latest-lookup parses built_at as RFC3339.
            assert!(chrono::DateTime::parse_from_rfc3339(&record.built_at).is_ok());
        }

        handle.shutdown().await;
        std::fs::remove_dir_all(&log_dir).unwrap();
    }

    #[tokio::test]
    async fn failed_build_writes_a_failure_record() {
        let (history, builds) = recording_history(Some(apply_record()));
        let (shell, _calls) = recording_shell(KW_VERSION_OK, CLEAN_STATUS, SWITCH_OK);
        let (handle, process, log_dir) =
            spawn_full_actor("failed-record", history, shell, built_tree_fs(), {
                let mut env = MockEnvTrait::new();
                env.expect_which().returning(|_| true);
                env
            });
        let mut watch = handle.watch_status().await.unwrap();

        handle.start_build(start_request()).await.unwrap();
        process.last_child().finish(2);
        let status = wait_for_terminal_status(&mut watch).await;
        assert!(matches!(
            status,
            KwJobStatus::Failed {
                exit_code: Some(2),
                ..
            }
        ));

        {
            let builds = builds.lock().unwrap();
            assert_eq!(1, builds.len());
            let record = &builds[0];
            assert!(!record.success);
            // Config/env facts are still recorded; what the build never
            // produced is not — a stale image from an earlier build must
            // not leak into a failure record.
            assert_eq!(Some("x86"), record.arch.as_deref());
            assert_eq!(None, record.image_path);
            assert_eq!(None, record.kernelrelease);
            assert_eq!(Some("msg-1"), record.message_id.as_deref());
        }

        handle.shutdown().await;
        std::fs::remove_dir_all(&log_dir).unwrap();
    }

    #[tokio::test]
    async fn cancelled_build_writes_no_record() {
        let (history, builds) = recording_history(None);
        let (shell, _calls) = recording_shell(KW_VERSION_OK, CLEAN_STATUS, SWITCH_OK);
        let (handle, _process, log_dir) =
            spawn_full_actor("cancel-record", history, shell, ready_fs(), {
                let mut env = MockEnvTrait::new();
                env.expect_which().returning(|_| true);
                env
            });
        let mut watch = handle.watch_status().await.unwrap();

        handle.start_build(start_request()).await.unwrap();
        handle.cancel().await.unwrap();
        let status = wait_for_terminal_status(&mut watch).await;

        assert!(matches!(status, KwJobStatus::Cancelled { .. }));
        assert!(builds.lock().unwrap().is_empty());

        handle.shutdown().await;
        std::fs::remove_dir_all(&log_dir).unwrap();
    }

    #[tokio::test]
    async fn lost_exit_status_records_a_failed_build() {
        let (history, builds) = recording_history(None);
        let (shell, _calls) = recording_shell(KW_VERSION_OK, CLEAN_STATUS, SWITCH_OK);
        let (handle, process, log_dir) =
            spawn_full_actor("wait-failure", history, shell, ready_fs(), {
                let mut env = MockEnvTrait::new();
                env.expect_which().returning(|_| true);
                env
            });
        let mut watch = handle.watch_status().await.unwrap();
        process.fail_waits(true);

        handle.start_build(start_request()).await.unwrap();
        process.last_child().finish(0);
        let status = wait_for_terminal_status(&mut watch).await;

        // A lost exit status is an honest failure: the record must not
        // become deploy-alone evidence.
        assert!(matches!(
            status,
            KwJobStatus::Failed {
                exit_code: None,
                ..
            }
        ));
        {
            let builds = builds.lock().unwrap();
            assert_eq!(1, builds.len());
            assert!(!builds[0].success);
        }

        handle.shutdown().await;
        std::fs::remove_dir_all(&log_dir).unwrap();
    }

    #[tokio::test]
    async fn build_record_write_failure_keeps_the_terminal_status() {
        let mut history = MockKwHistoryStore::new();
        history
            .expect_apply_record_for_branch()
            .returning(|_, _| Ok(None));
        history
            .expect_record_build()
            .returning(|_| Err(FileSystemError::IoError(io::Error::other("disk full"))));
        let (shell, _calls) = recording_shell(KW_VERSION_OK, CLEAN_STATUS, SWITCH_OK);
        let (handle, process, log_dir) =
            spawn_full_actor("record-write-fails", history, shell, ready_fs(), {
                let mut env = MockEnvTrait::new();
                env.expect_which().returning(|_| true);
                env
            });
        let mut watch = handle.watch_status().await.unwrap();

        handle.start_build(start_request()).await.unwrap();
        process.last_child().finish(0);
        let status = wait_for_terminal_status(&mut watch).await;

        // The build's real outcome reached the user; a history-write
        // failure must not turn it into a reported failure.
        assert!(matches!(status, KwJobStatus::Succeeded { .. }));

        handle.shutdown().await;
        std::fs::remove_dir_all(&log_dir).unwrap();
    }

    #[tokio::test]
    async fn build_record_keeps_no_patchset_link_when_apply_lookup_fails() {
        let builds = Arc::new(Mutex::new(Vec::new()));
        let builds_in_store = Arc::clone(&builds);
        let mut history = MockKwHistoryStore::new();
        history.expect_apply_record_for_branch().returning(|_, _| {
            Err(FileSystemError::IoError(io::Error::other(
                "corrupt history",
            )))
        });
        history.expect_record_build().returning(move |record| {
            builds_in_store.lock().unwrap().push(record);
            Ok(())
        });
        let (shell, _calls) = recording_shell(KW_VERSION_OK, CLEAN_STATUS, SWITCH_OK);
        let (handle, process, log_dir) =
            spawn_full_actor("link-lookup-fails", history, shell, ready_fs(), {
                let mut env = MockEnvTrait::new();
                env.expect_which().returning(|_| true);
                env
            });
        let mut watch = handle.watch_status().await.unwrap();

        handle.start_build(start_request()).await.unwrap();
        process.last_child().finish(0);
        let _ = wait_for_terminal_status(&mut watch).await;

        // The lookup error must not drop the record, only the link.
        {
            let builds = builds.lock().unwrap();
            assert_eq!(1, builds.len());
            assert_eq!(None, builds[0].message_id);
            assert!(builds[0].success);
        }

        handle.shutdown().await;
        std::fs::remove_dir_all(&log_dir).unwrap();
    }

    #[tokio::test]
    async fn build_record_skipped_when_env_state_breaks_mid_build() {
        // An env that resolves at Start but not at completion must not
        // produce a record keyed with a wrong `output_dir: None`.
        let env_broken = Arc::new(AtomicBool::new(false));
        let env_broken_in_fs = Arc::clone(&env_broken);
        let mut fs = MockFileSystemTrait::new();
        fs.expect_is_dir().returning(|_| true);
        fs.expect_is_file().returning(|_| true);
        fs.expect_exists().returning(|_| true);
        fs.expect_read_to_string().returning(move |path| {
            if path.ends_with("env.current") && !env_broken_in_fs.load(Ordering::Relaxed) {
                Ok("testenv\n".to_string())
            } else {
                Err(FileSystemError::IoError(io::Error::other("unreadable")))
            }
        });
        fs.expect_create_dir_all().returning(|_| Ok(()));
        let mut env = MockEnvTrait::new();
        env.expect_which().returning(|_| true);
        env.expect_var()
            .returning(|_| Ok("/home/user/.cache".to_string()));
        let (history, builds) = recording_history(None);
        let (shell, _calls) = recording_shell(KW_VERSION_OK, CLEAN_STATUS, SWITCH_OK);
        let (handle, process, log_dir) = spawn_full_actor("env-breaks", history, shell, fs, env);
        let mut watch = handle.watch_status().await.unwrap();

        handle.start_build(start_request()).await.unwrap();
        // Break the env state after the start probes but before the
        // completion probes run.
        env_broken.store(true, Ordering::Relaxed);
        process.last_child().finish(0);
        let status = wait_for_terminal_status(&mut watch).await;

        assert!(matches!(status, KwJobStatus::Succeeded { .. }));
        assert!(builds.lock().unwrap().is_empty());

        handle.shutdown().await;
        std::fs::remove_dir_all(&log_dir).unwrap();
    }
}
