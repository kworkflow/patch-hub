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

use std::{ops::ControlFlow, path::PathBuf, sync::Arc};

use tokio::{
    spawn,
    sync::{mpsc, oneshot, watch},
};

use crate::{
    config::KernelTree,
    infrastructure::{
        env::EnvTrait,
        file_system::FileSystemTrait,
        process::ProcessTrait,
        shell::{ShellCommand, ShellTrait},
    },
    kw::{
        errors::{KwError, KwStartError},
        handle::KwHandle,
        history::KwHistoryStore,
        messages::KwMessage,
        readiness::{self, KwReadiness},
        status::KwStatusSnapshot,
    },
};

pub const DEFAULT_KW_CHANNEL_SIZE: usize = 16;

pub struct KwActor {
    rx: mpsc::Receiver<KwMessage>,
    status_tx: watch::Sender<KwStatusSnapshot>,
    history: Arc<dyn KwHistoryStore>,
    shell: Arc<dyn ShellTrait>,
    fs: Arc<dyn FileSystemTrait>,
    env: Arc<dyn EnvTrait>,
    // Read once job execution lands; kept per the CachePolicy precedent
    // (src/lore/application/cache.rs).
    #[allow(dead_code)]
    process: Arc<dyn ProcessTrait>,
    #[allow(dead_code)]
    kw_log_dir: PathBuf,
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
        Self {
            rx,
            status_tx,
            history,
            shell,
            fs,
            env,
            process,
            kw_log_dir,
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
        while let Some(message) = self.rx.recv().await {
            if let ControlFlow::Break(()) = self.handle_message(message) {
                break;
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
            // Job execution lands with the build step; the immediate-reply
            // contract holds from the skeleton onward, so callers never
            // learn to depend on a blocking reply.
            KwMessage::StartBuild { reply, .. }
            | KwMessage::StartDeploy { reply, .. }
            | KwMessage::StartBuildThenDeploy { reply, .. } => {
                send_start_reply(message_name, reply, Err(KwStartError::NotImplemented));
                ControlFlow::Continue(())
            }
            KwMessage::Cancel { reply } => {
                send_kw_reply(message_name, reply, Err(KwError::NoJobRunning));
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
                tracing::debug!("kw actor shutting down");
                ControlFlow::Break(())
            }
        }
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
    use std::{io, path::Path, time::Duration};

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
            status::KwJobStatus,
        },
    };

    use super::*;

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

    #[tokio::test]
    async fn start_build_replies_immediately_with_refusal() {
        let handle = spawn_test_actor(
            MockKwHistoryStore::new(),
            MockShellTrait::new(),
            MockFileSystemTrait::new(),
            MockEnvTrait::new(),
        );

        // The immediate-reply contract: the answer must arrive without any
        // job completing — there is not even a job yet.
        let result =
            tokio::time::timeout(Duration::from_secs(1), handle.start_build(start_request()))
                .await
                .expect("start_build must reply immediately");

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
