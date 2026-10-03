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
//! Git checkout and readiness probes run on the blocking pool.
//! `create_dir_all`, history writes, and completion probes run inline
//! on the actor task.

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
        log_scan,
        messages::{DeployOptions, KwMessage, StartRequest},
        models::job::{JobDeploy, JobEvent, JobOutcome, JobState, RestoreContext},
        readiness::{self, BootOnceState, KwReadiness, KwVersionCheck, TreeReadiness},
        remote,
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
        // Prefer control messages over job events so a Cancel that races a
        // successful build exit is observed before BuildThenDeploy would
        // chain into deploy.
        loop {
            tokio::select! {
                biased;
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
                    self.start_job(KwJobKind::Build, request).await,
                );
                ControlFlow::Continue(())
            }
            KwMessage::StartDeploy { request, reply } => {
                send_start_reply(
                    message_name,
                    reply,
                    self.start_job(KwJobKind::Deploy, request).await,
                );
                ControlFlow::Continue(())
            }
            KwMessage::StartBuildThenDeploy { request, reply } => {
                send_start_reply(
                    message_name,
                    reply,
                    self.start_job(KwJobKind::BuildThenDeploy, request).await,
                );
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
                for_branch,
                reply,
            } => {
                send_kw_reply(
                    message_name,
                    reply,
                    self.evaluate_readiness(&kernel_tree_id, &tree, for_branch)
                        .await,
                );
                ControlFlow::Continue(())
            }
            KwMessage::RestorePreviousBranch { reply } => {
                send_kw_reply(message_name, reply, self.restore_previous_branch().await);
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

    /// Accepts and starts a job, or refuses. The reply is sent by the
    /// caller right after this returns; the job keeps running in a
    /// detached task. A start refused after the branch switch rolls
    /// the switch back.
    async fn start_job(
        &mut self,
        kind: KwJobKind,
        request: StartRequest,
    ) -> Result<(), KwStartError> {
        if self.job.is_some() {
            return Err(KwStartError::JobAlreadyRunning);
        }

        let tree_path = PathBuf::from(request.tree.path());
        // Hard fail on invoke: with no kw binary on PATH no job can run.
        // The version check is advisory only — kw's shipped VERSION file
        // is stale, so Below/Unknown are logged, never gated.
        // Probes run on the blocking pool so a slow tree or cold
        // `kw --version` cannot stall the Tokio worker (or delay Cancel
        // past this Start's await).
        let fs = Arc::clone(&self.fs);
        let env = Arc::clone(&self.env);
        let shell = Arc::clone(&self.shell);
        let tree_path_for_probe = tree_path.clone();
        let (kw_binary, output_dir, tree_readiness) = tokio::task::spawn_blocking(move || {
            let kw_binary = readiness::ReadinessService::probe_kw_binary(&*env, &*shell);
            if !kw_binary.available {
                return Err(KwStartError::KwBinaryMissing);
            }
            let output_dir =
                readiness::ReadinessService::resolve_output_dir(&*fs, &*env, &tree_path_for_probe)?;
            let tree_readiness = readiness::ReadinessService::probe_tree(
                &*fs,
                &tree_path_for_probe,
                output_dir.as_deref(),
            );
            Ok((kw_binary, output_dir, tree_readiness))
        })
        .await
        .map_err(|error| KwStartError::GitStateProbe(error.to_string()))??;
        if let KwVersionCheck::Below(version_line) = &kw_binary.check {
            tracing::warn!(
                version = %version_line,
                minimum = ?readiness::KW_MIN_VERSION,
                "kw reports a version below the verified floor"
            );
        }

        // Unresolvable env state refuses the start: the build record this
        // job writes at completion must know whether it ran under an O=.
        // Both values are then snapshotted onto the job — the build runs
        // under them, so the completion record describes them, not the
        // tree's configuration at whatever time the job ends.
        let TreeReadiness::Ready { arch } = tree_readiness else {
            return Err(KwStartError::TreeNotReady(tree_readiness));
        };

        let pre_job_branch = self.checkout_build_branch(&request).await?;

        let deploy = if matches!(kind, KwJobKind::Deploy | KwJobKind::BuildThenDeploy) {
            match self
                .prepare_deploy(
                    kind,
                    &request,
                    &tree_path,
                    output_dir.as_deref(),
                    arch.as_deref(),
                )
                .await
            {
                Ok(deploy) => Some(deploy),
                Err(error) => {
                    self.rollback_switch(request.tree.path(), pre_job_branch.as_deref())
                        .await;
                    return Err(error);
                }
            }
        } else {
            None
        };

        let spawned = match kind {
            KwJobKind::Deploy => self.spawn_deploy_process(
                request.tree.path(),
                deploy
                    .as_ref()
                    .expect("prepare_deploy returns context for Deploy"),
            ),
            KwJobKind::Build | KwJobKind::BuildThenDeploy => self.spawn_build_process(&request),
        };
        let (process, log_path) = match spawned {
            Ok(spawned) => spawned,
            Err(error) => {
                self.rollback_switch(request.tree.path(), pre_job_branch.as_deref())
                    .await;
                return Err(error);
            }
        };

        let (cancel_tx, cancel_rx) = oneshot::channel();
        spawn(run_job(process, cancel_rx, self.job_event_tx.clone()));

        let phase = match kind {
            KwJobKind::Deploy => KwPhase::Deploying,
            KwJobKind::Build | KwJobKind::BuildThenDeploy => KwPhase::Building,
        };
        match kind {
            KwJobKind::Deploy => tracing::info!(
                kernel_tree_id = request.kernel_tree_id,
                branch = request.branch,
                log_path = %log_path.display(),
                "kw deploy job started"
            ),
            KwJobKind::Build | KwJobKind::BuildThenDeploy => tracing::info!(
                kernel_tree_id = request.kernel_tree_id,
                branch = request.branch,
                log_path = %log_path.display(),
                "kw build job started"
            ),
        }
        // Recorded only on accept. Refused starts never reach here, and —
        // with their switch rolled back — can neither clobber a previous
        // job's restore target nor strand the tree on a branch the user
        // did not check out. An accepted job with an unprobed pre-job
        // HEAD clears the target: nothing honest is left to restore to.
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
            output_dir,
            arch,
            log_path: log_path.clone(),
            deploy,
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

    /// Deploy-kind gates after the branch switch: resolved remote for
    /// every deploy kind; the deploy-alone record match only for
    /// StartDeploy, against the requested (now checked-out) branch;
    /// boot-once confirm last so a missing record refuses before the
    /// confirm popup. Any refusal is rolled back by the caller.
    /// BuildThenDeploy skips the record gate because the build has not
    /// run yet. Probes run on the blocking pool.
    async fn prepare_deploy(
        &self,
        kind: KwJobKind,
        request: &StartRequest,
        tree_path: &Path,
        output_dir: Option<&Path>,
        arch: Option<&str>,
    ) -> Result<JobDeploy, KwStartError> {
        let fs = Arc::clone(&self.fs);
        let env = Arc::clone(&self.env);
        let history = Arc::clone(&self.history);
        let request = request.clone();
        let tree_path = tree_path.to_path_buf();
        let output_dir = output_dir.map(Path::to_path_buf);
        let arch = arch.map(str::to_string);
        tokio::task::spawn_blocking(move || {
            prepare_deploy_blocking(
                kind,
                &request,
                &*fs,
                &*env,
                &*history,
                &tree_path,
                output_dir.as_deref(),
                arch.as_deref(),
            )
        })
        .await
        .map_err(|error| KwStartError::GitStateProbe(error.to_string()))?
    }

    /// Refuse a dirty worktree, record HEAD, then `git switch` to the
    /// requested branch. The git calls run on the blocking pool so the
    /// accept reply stays immediate.
    ///
    /// The HEAD probe sits between the dirty check and the switch: it
    /// must capture the branch the user was on, or RestorePreviousBranch
    /// would restore the branch the job switched to. An unprobed HEAD
    /// (detached, or not a git repo) yields `None` rather than a wrong
    /// branch.
    async fn checkout_build_branch(
        &self,
        request: &StartRequest,
    ) -> Result<Option<String>, KwStartError> {
        let shell = Arc::clone(&self.shell);
        let tree_path = request.tree.path().to_string();
        let branch = request.branch.clone();
        let pre_job_branch =
            tokio::task::spawn_blocking(move || -> Result<String, TreeGitError> {
                check_worktree_clean(&*shell, &tree_path)?;
                let pre_job_branch = head_branch(&*shell, &tree_path);
                switch_to_branch(&*shell, &tree_path, &branch)?;
                Ok(pre_job_branch)
            })
            .await
            // A join error means the probe task panicked — a bug, surfaced as
            // an unverifiable git state rather than wedging the actor.
            .map_err(|error| KwStartError::GitStateProbe(error.to_string()))??;
        Ok(if pre_job_branch.is_empty() {
            None
        } else {
            Some(pre_job_branch)
        })
    }

    /// Switches the tree back after a start that reached the branch
    /// switch but failed before accepting the job. A rollback failure is
    /// logged, not reported: the caller already holds the actionable
    /// refusal. An unprobed pre-job HEAD leaves nothing to roll back to.
    async fn rollback_switch(&self, tree_path: &str, pre_job_branch: Option<&str>) {
        let Some(branch) = pre_job_branch else {
            tracing::warn!(
                tree = tree_path,
                "cannot roll back the branch switch: pre-job HEAD was unprobed"
            );
            return;
        };
        let shell = Arc::clone(&self.shell);
        let tree_path = tree_path.to_string();
        let branch = branch.to_string();
        let branch_for_task = branch.clone();
        match tokio::task::spawn_blocking(move || {
            switch_to_branch(&*shell, &tree_path, &branch_for_task)
        })
        .await
        {
            Ok(Ok(())) => tracing::info!(
                branch,
                "rolled back the branch switch after a refused start"
            ),
            Ok(Err(error)) => {
                let error = KwStartError::from(error);
                tracing::warn!(%error, "failed to roll back the branch switch")
            }
            Err(error) => {
                tracing::warn!(%error, "branch-switch rollback task failed to join")
            }
        }
    }

    /// Creates the job's log dir and spawns the kw process. Split from
    /// `start_job` so a failure here can roll the branch switch back.
    fn spawn_build_process(
        &self,
        request: &StartRequest,
    ) -> Result<(Box<dyn RunningProcess>, PathBuf), KwStartError> {
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
        Ok((process, log_path))
    }

    /// Creates the job's log dir and spawns `kw deploy`. Split from
    /// `start_job` so a failure here can roll the branch switch back.
    fn spawn_deploy_process(
        &self,
        tree_path: &str,
        deploy: &JobDeploy,
    ) -> Result<(Box<dyn RunningProcess>, PathBuf), KwStartError> {
        self.fs.create_dir_all(&self.kw_log_dir)?;
        let log_path = self.kw_log_dir.join(format!(
            "deploy-{}.log",
            chrono::Utc::now().format("%Y%m%d-%H%M%S-%3f")
        ));
        let cmd = ShellCommand::new("kw").args(argv::deploy_argv(
            &deploy.remote.endpoint(),
            deploy.options.reboot,
            deploy.options.force,
            &deploy.extra_args,
        ));
        let cwd = PathBuf::from(tree_path);
        let process = self.process.spawn(&cmd, &cwd, &log_path)?;
        Ok((process, log_path))
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
                let outcome = outcome_after_building_cancel(&job, outcome);
                let job = match self.advance_build_then_deploy(job, &outcome) {
                    ControlFlow::Break(()) => return,
                    ControlFlow::Continue(job) => job,
                };
                // The record is written before the status flips: watchers
                // that react to the terminal status find the history
                // already durable.
                self.record_build_outcome(&job, &outcome);
                let status = match outcome {
                    JobOutcome::Exited(exit) if exit.success() => {
                        // kw deploy exits 0 through initramfs and GRUB
                        // failures; only its log tells.
                        let warnings = if job.phase == KwPhase::Deploying {
                            let kernelrelease = job
                                .deploy
                                .as_ref()
                                .and_then(|deploy| deploy.kernelrelease.as_deref());
                            self.read_job_log(&job.log_path)
                                .map(|log| log_scan::deploy_warnings(&log, kernelrelease))
                                .unwrap_or_default()
                        } else {
                            Vec::new()
                        };
                        tracing::info!(
                            kernel_tree_id = job.kernel_tree_id,
                            branch = job.branch,
                            warnings = warnings.len(),
                            "kw job succeeded"
                        );
                        KwJobStatus::Succeeded {
                            kind: job.kind,
                            kernel_tree_id: job.kernel_tree_id,
                            branch: job.branch,
                            log_path: job.log_path,
                            warnings,
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
                            first_error: self
                                .read_job_log(&job.log_path)
                                .and_then(|log| log_scan::first_error(&log)),
                            log_path: job.log_path,
                        }
                    }
                    JobOutcome::WaitFailed(error) => {
                        tracing::warn!(branch = job.branch, %error, "failed to wait on kw job");
                        KwJobStatus::Failed {
                            kind: job.kind,
                            phase: job.phase,
                            exit_code: None,
                            first_error: self
                                .read_job_log(&job.log_path)
                                .and_then(|log| log_scan::first_error(&log)),
                            log_path: job.log_path,
                        }
                    }
                    JobOutcome::Cancelled => {
                        tracing::info!(branch = job.branch, "kw job cancelled");
                        KwJobStatus::Cancelled {
                            kind: job.kind,
                            phase: job.phase,
                            log_path: job.log_path,
                        }
                    }
                };
                self.set_status(status);
            }
        }
    }

    /// After a successful BuildThenDeploy build, write the build record
    /// and spawn the deploy process. `Break` means this Finished was
    /// consumed (job still running, or failed at the deploy spawn
    /// boundary). `Continue` hands the job back for a normal terminal.
    fn advance_build_then_deploy(
        &mut self,
        job: JobState,
        outcome: &JobOutcome,
    ) -> ControlFlow<(), JobState> {
        let JobOutcome::Exited(exit) = outcome else {
            return ControlFlow::Continue(job);
        };
        if job.kind != KwJobKind::BuildThenDeploy
            || job.phase != KwPhase::Building
            || !exit.success()
            // A cancel during Building must not deploy, even if the build
            // process exited 0 (wait racing the cancel arm, or a trapped
            // SIGTERM). `cancel_tx` is taken when Cancel is requested.
            || job.cancel_tx.is_none()
        {
            return ControlFlow::Continue(job);
        }
        // Durable before the phase flips: a deploy spawn failure must not
        // lose the successful build, and KwOps watching Deploying must
        // already see the record.
        let kernelrelease = self.record_build_outcome(&job, outcome);
        let Some(mut deploy) = job.deploy.clone() else {
            tracing::error!("BuildThenDeploy missing deploy context after a successful build");
            self.set_status(KwJobStatus::Failed {
                kind: job.kind,
                phase: KwPhase::Deploying,
                exit_code: None,
                log_path: job.log_path,
                first_error: None,
            });
            return ControlFlow::Break(());
        };
        deploy.kernelrelease = kernelrelease;
        match self.spawn_deploy_process(&job.tree_path, &deploy) {
            Ok((process, log_path)) => {
                let (cancel_tx, cancel_rx) = oneshot::channel();
                spawn(run_job(process, cancel_rx, self.job_event_tx.clone()));
                tracing::info!(
                    kernel_tree_id = job.kernel_tree_id,
                    branch = job.branch,
                    log_path = %log_path.display(),
                    "kw deploy phase started"
                );
                self.set_status(KwJobStatus::Running {
                    kind: job.kind,
                    phase: KwPhase::Deploying,
                    kernel_tree_id: job.kernel_tree_id.clone(),
                    branch: job.branch.clone(),
                    log_path: log_path.clone(),
                });
                let mut job = job;
                job.phase = KwPhase::Deploying;
                job.log_path = log_path;
                job.deploy = Some(deploy);
                job.cancel_tx = Some(cancel_tx);
                self.job = Some(job);
                ControlFlow::Break(())
            }
            Err(error) => {
                tracing::warn!(
                    %error,
                    branch = job.branch,
                    "failed to spawn kw deploy after a successful build"
                );
                self.set_status(KwJobStatus::Failed {
                    kind: job.kind,
                    phase: KwPhase::Deploying,
                    exit_code: None,
                    log_path: job.log_path,
                    first_error: None,
                });
                ControlFlow::Break(())
            }
        }
    }

    /// Persist a finished build (success or failure). Cancel writes
    /// nothing. History and patchset-link errors are logged, not folded
    /// into job status — the build's real outcome already reached the user.
    /// Returns the recorded kernelrelease, if the build produced one.
    fn record_build_outcome(&self, job: &JobState, outcome: &JobOutcome) -> Option<String> {
        if job.kind == KwJobKind::Deploy || job.phase == KwPhase::Deploying {
            return None;
        }
        let success = match outcome {
            JobOutcome::Exited(exit) => exit.success(),
            // The exit status is lost: record an honest failure rather
            // than guess, so deploy-alone readiness cannot trust it.
            JobOutcome::WaitFailed(_) => false,
            JobOutcome::Cancelled => return None,
        };

        // The record describes the accept-time snapshot: the build ran
        // under this tree path, output dir, and arch. Re-resolving them
        // here could describe a configuration the build never used — an
        // env deactivated mid-build would key the record with a wrong
        // `output_dir: None` and probe the tree for an image the build
        // wrote under O=, a Frankenstein match for a later deploy-alone
        // probe.
        let tree_path = Path::new(&job.tree_path);
        let build_root = job.output_dir.as_deref().unwrap_or(tree_path);
        // A failed build may have left a stale image from an earlier
        // successful one behind; only successes record what they
        // produced.
        let (image_path, kernelrelease) = if success {
            (
                readiness::ReadinessService::find_newest_kernel_image(
                    &*self.fs,
                    build_root,
                    job.arch.as_deref(),
                ),
                readiness::ReadinessService::read_kernelrelease(&*self.fs, build_root),
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
            arch: job.arch.clone(),
            image_path: image_path.map(|path| path.to_string_lossy().into_owned()),
            output_dir: job
                .output_dir
                .as_ref()
                .map(|path| path.to_string_lossy().into_owned()),
            kernelrelease: kernelrelease.clone(),
            log_path: job.log_path.to_string_lossy().into_owned(),
            built_at: chrono::Utc::now().to_rfc3339(),
            success,
        };
        if let Err(error) = self.history.record_build(record) {
            tracing::warn!(%error, branch = job.branch, "failed to record kw build history");
        }
        kernelrelease
    }

    /// Reads a finished job's log for the findings its terminal status
    /// carries. An unreadable log only loses those findings: the exit code
    /// is still the verdict.
    fn read_job_log(&self, log_path: &Path) -> Option<String> {
        match self.fs.read_to_string(log_path) {
            Ok(log) => Some(log),
            Err(error) => {
                tracing::warn!(%error, log_path = %log_path.display(), "failed to read kw job log");
                None
            }
        }
    }

    fn set_status(&mut self, job: KwJobStatus) {
        // send_replace, not send: no receiver (nobody called WatchStatus
        // yet) is a normal state, not an error. Restore availability is
        // published with every snapshot so the UI does not have to guess
        // whether RestorePreviousBranch would succeed.
        self.status_tx.send_replace(KwStatusSnapshot {
            job,
            restore_branch: self
                .last_restore
                .as_ref()
                .map(|restore| restore.branch.clone()),
        });
    }

    /// Re-broadcasts the current job with the current restore projection.
    /// Used after restore succeeds (the branch is gone) without changing
    /// the job's own status.
    fn republish_status(&mut self) {
        let job = self.status_tx.borrow().job.clone();
        self.set_status(job);
    }

    /// Git probes and history I/O run on the blocking pool so a slow
    /// tree cannot stall Cancel (or any other message) for the duration.
    async fn evaluate_readiness(
        &self,
        kernel_tree_id: &str,
        tree: &KernelTree,
        for_branch: Option<String>,
    ) -> Result<KwReadiness, KwError> {
        let fs = Arc::clone(&self.fs);
        let env = Arc::clone(&self.env);
        let shell = Arc::clone(&self.shell);
        let history = Arc::clone(&self.history);
        let kernel_tree_id = kernel_tree_id.to_string();
        let tree = tree.clone();
        tokio::task::spawn_blocking(move || {
            let head = head_branch(&*shell, tree.path());
            readiness::ReadinessService::evaluate_readiness(
                &*fs,
                &*env,
                &*shell,
                &*history,
                &kernel_tree_id,
                &tree,
                &head,
                for_branch.as_deref(),
            )
            .map_err(KwError::from)
        })
        .await
        .map_err(|error| KwError::GitStateProbe(error.to_string()))?
    }

    /// Switches the tree that ran the last job back to the branch HEAD was
    /// on when that job was accepted. Refuses while a job is
    /// running (its branch is in use), when nothing was recorded, and on
    /// a dirty worktree. Only a successful switch consumes the context —
    /// a refused restore stays available for a retry.
    async fn restore_previous_branch(&mut self) -> Result<(), KwError> {
        if self.job.is_some() {
            return Err(KwError::JobRunning);
        }
        let Some(restore) = self.last_restore.take() else {
            return Err(KwError::NoRecordedBranch);
        };
        let shell = Arc::clone(&self.shell);
        let tree_path = restore.tree_path.clone();
        let branch = restore.branch.clone();
        // Same spawn_blocking rationale as the start path's checkout: the
        // switch rewrites the worktree.
        let result = tokio::task::spawn_blocking(move || -> Result<(), TreeGitError> {
            check_worktree_clean(&*shell, &tree_path)?;
            switch_to_branch(&*shell, &tree_path, &branch)
        })
        .await
        .map_err(|error| KwError::GitStateProbe(error.to_string()))
        .and_then(|result| result.map_err(KwError::from));
        match result {
            Ok(()) => {
                tracing::info!(
                    tree = restore.tree_path,
                    branch = restore.branch,
                    "restored pre-job branch"
                );
                self.republish_status();
                Ok(())
            }
            Err(error) => {
                self.last_restore = Some(restore);
                Err(error)
            }
        }
    }
}

/// Deploy gates that must not run on the actor task: remote.config,
/// history JSON, image globs, and boot-once files. Record match runs
/// before the boot-once confirm so a missing build refuses cheaper.
#[expect(clippy::too_many_arguments)]
fn prepare_deploy_blocking(
    kind: KwJobKind,
    request: &StartRequest,
    fs: &dyn FileSystemTrait,
    env: &dyn EnvTrait,
    history: &dyn KwHistoryStore,
    tree_path: &Path,
    output_dir: Option<&Path>,
    arch: Option<&str>,
) -> Result<JobDeploy, KwStartError> {
    let options = request.deploy.clone().unwrap_or(DeployOptions {
        reboot: false,
        force: true,
        boot_once_acknowledged: false,
    });
    let remote = remote::resolve_deploy_remote(fs, env, tree_path)
        .map_err(KwStartError::RemoteUnresolved)?;
    let mut kernelrelease = None;
    if kind == KwJobKind::Deploy {
        let (record, latest) = history.build_records(&request.kernel_tree_id, &request.branch)?;
        let image = readiness::ReadinessService::find_newest_kernel_image(
            fs,
            output_dir.unwrap_or(tree_path),
            arch,
        );
        readiness::ReadinessService::check_deploy_alone(
            record.as_ref(),
            latest.as_ref(),
            &request.tree,
            &request.branch,
            output_dir,
            image.as_deref(),
        )
        .map_err(KwStartError::DeployAloneRefused)?;
        kernelrelease = record.and_then(|record| record.kernelrelease);
    }
    let boot_once = readiness::ReadinessService::probe_boot_once(fs, env, tree_path);
    if matches!(boot_once, BootOnceState::On | BootOnceState::Unknown)
        && !options.boot_once_acknowledged
    {
        return Err(KwStartError::BootOnceNotAcknowledged);
    }
    Ok(JobDeploy {
        remote,
        options,
        extra_args: request.extra_args.clone(),
        kernelrelease,
    })
}

/// Fails unless the tree's git state verifies clean. Untracked files
/// don't count: kernel trees accumulate local scratch files, and only
/// tracked changes can corrupt the branch a job builds. A probe that
/// itself fails — git missing, not a repository — fails too.
fn check_worktree_clean(shell: &dyn ShellTrait, tree_path: &str) -> Result<(), TreeGitError> {
    let cmd = ShellCommand::new("git").args([
        "-C",
        tree_path,
        "status",
        "--porcelain",
        "--untracked-files=no",
    ]);
    let output = shell
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

/// Switches the tree onto `branch`. A failure carries git's stderr, which
/// names the usual causes (no such branch, a rebase or merge in
/// progress). The `--` keeps a branch named like a flag from being parsed
/// as one.
fn switch_to_branch(
    shell: &dyn ShellTrait,
    tree_path: &str,
    branch: &str,
) -> Result<(), TreeGitError> {
    let cmd = ShellCommand::new("git").args(["-C", tree_path, "switch", "--", branch]);
    let output = shell
        .execute(&cmd)
        .map_err(|error| TreeGitError::Switch(error.to_string()))?;
    if !output.success {
        return Err(TreeGitError::Switch(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    Ok(())
}

/// The tree's current branch, probed via git. An unresolvable HEAD
/// (not a git repo, or a detached HEAD, for which `branch
/// --show-current` prints nothing) yields an empty string: no build
/// record can match it, so deploy-alone readiness refuses — the safe
/// direction for an unknown HEAD.
fn head_branch(shell: &dyn ShellTrait, tree_path: &str) -> String {
    let cmd = ShellCommand::new("git").args(["-C", tree_path, "branch", "--show-current"]);
    match shell.execute(&cmd) {
        Ok(output) if output.success => String::from_utf8_lossy(&output.stdout).trim().to_string(),
        Ok(output) => {
            tracing::warn!(
                tree = tree_path,
                stderr = %String::from_utf8_lossy(&output.stderr),
                "failed to probe the kernel tree's HEAD branch"
            );
            String::new()
        }
        Err(error) => {
            tracing::warn!(tree = tree_path, %error, "failed to probe the kernel tree's HEAD branch");
            String::new()
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

/// A cancel during Building of a BuildThenDeploy job must not chain into
/// deploy, even when the build process reports exit 0. Rewriting the
/// outcome to [`JobOutcome::Cancelled`] also skips the build record, matching
/// the "cancel in Building writes nothing" rule.
fn outcome_after_building_cancel(job: &JobState, outcome: JobOutcome) -> JobOutcome {
    if job.cancel_tx.is_none()
        && job.kind == KwJobKind::BuildThenDeploy
        && job.phase == KwPhase::Building
        && matches!(&outcome, JobOutcome::Exited(exit) if exit.success())
    {
        JobOutcome::Cancelled
    } else {
        outcome
    }
}

/// Maps a reap result observed after a cancel request. A signal-terminated
/// process means our SIGTERM/SIGKILL landed — the job was really cancelled.
/// A plain exit means the process finished on its own before the signal:
/// report the real outcome, because a cancel must not mask a failure the
/// build history (and deploy-alone readiness) needs to see.
///
/// Known, accepted edges: a process that *traps* our SIGTERM and exits 0
/// counts as success for a standalone build (kw is bash, so this is
/// possible in principle). BuildThenDeploy does not chain that exit into
/// deploy — see [`outcome_after_building_cancel`]. An external signal
/// racing a cancel (e.g. the OOM killer) reads as Cancelled. Those cases
/// are indistinguishable from the honest ones without comparing who
/// signaled first, and both favor showing the user real output over
/// inventing failures.
fn outcome_after_cancel(result: Result<ExitStatus, ProcessError>) -> JobOutcome {
    use std::os::unix::process::ExitStatusExt;

    match result {
        Ok(status) => {
            if status.signal().is_some() {
                return JobOutcome::Cancelled;
            }
            // kw is a bash wrapper: a process-group SIGTERM/SIGKILL often
            // surfaces as a plain exit of 128+signum (143 / 137) rather than
            // WIFSIGNALED. After a cancel request those are still cancels.
            match status.code() {
                Some(143 | 137) => JobOutcome::Cancelled,
                _ => JobOutcome::Exited(status),
            }
        }
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
mod actor_test;
#[cfg(test)]
mod deploy_test;
