use std::{io, path::PathBuf, sync::Arc};

use color_eyre::Result;
use tokio::task;

use crate::{
    app::{
        models::{
            kw_ops::{DeployStartKind, KwOpsFocus, KwOpsState},
            popup::AppPopup,
        },
        screens::CurrentScreen,
        App,
    },
    infrastructure::file_system::FileSystemError,
    input::event::InputEvent,
    kw::{
        errors::{KwError, KwStartError},
        messages::{DeployOptions, StartRequest},
        models::readiness::BootOnceState,
        status::{KwJobStatus, KwStatusSnapshot},
    },
};

/// Bytes read from the end of a kw job log. Kernel build logs can be
/// far larger than this; the TUI only needs a bounded tail.
pub(crate) const LOG_TAIL_MAX_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy)]
enum KwStartKind {
    Build,
    Deploy,
    BuildThenDeploy,
}

impl KwStartKind {
    fn title(self) -> &'static str {
        match self {
            Self::Build => "Cannot start build",
            Self::Deploy => "Cannot start deploy",
            Self::BuildThenDeploy => "Cannot start build+deploy",
        }
    }

    fn action_word(self) -> &'static str {
        match self {
            Self::Build => "build",
            Self::Deploy | Self::BuildThenDeploy => "job",
        }
    }

    fn pending_kind(self) -> Option<DeployStartKind> {
        match self {
            Self::Build => None,
            Self::Deploy => Some(DeployStartKind::Deploy),
            Self::BuildThenDeploy => Some(DeployStartKind::BuildThenDeploy),
        }
    }
}

impl App {
    /// Refresh the KwOps log panel from the job log. A missing file is normal
    /// just after accept. Other read errors become a non-fatal panel
    /// diagnostic and do not change job status. Returns whether the text
    /// changed. The read runs on the blocking pool so the actor loop does
    /// not stall on disk I/O.
    pub(crate) async fn refresh_kw_ops_log_tail(&mut self) -> bool {
        if self.state.kw.ops.is_none() {
            return false;
        }
        let Some(path) = self
            .state
            .kw
            .status
            .as_ref()
            .and_then(|status| status.job.log_path())
            .map(PathBuf::from)
        else {
            return false;
        };
        let fs = Arc::clone(&self.services.fs);
        let result =
            task::spawn_blocking(move || fs.read_tail_to_string(&path, LOG_TAIL_MAX_BYTES)).await;
        let text = match result {
            Ok(read) => Self::map_tail_read(read),
            Err(error) => format!("(could not read log: {error})"),
        };
        let Some(ops) = self.state.kw.ops.as_mut() else {
            return false;
        };
        Self::assign_log_tail(ops, text)
    }

    /// Projects a kw snapshot into App state and releases the optimistic
    /// start lock once the actor has left Idle.
    pub(crate) fn apply_kw_snapshot(&mut self, snapshot: KwStatusSnapshot) {
        if let Some(ops) = self.state.kw.ops.as_mut() {
            let left_idle = match &snapshot.job {
                KwJobStatus::Idle => false,
                KwJobStatus::Running { .. }
                | KwJobStatus::Succeeded { .. }
                | KwJobStatus::Failed { .. }
                | KwJobStatus::Cancelled { .. } => true,
            };
            if left_idle {
                ops.start_requested = false;
            }
        }
        self.state.kw.status = Some(snapshot);
    }

    /// Re-probe deploy-alone after a job ends so the (d) label tracks the
    /// in-session build record instead of staying on the pre-build snapshot.
    pub(crate) async fn apply_kw_snapshot_refreshing_readiness(
        &mut self,
        snapshot: KwStatusSnapshot,
    ) {
        let refresh = Self::is_snapshot_job_terminal(&snapshot.job)
            && self.state.navigation.current_screen == CurrentScreen::KwOps;
        self.apply_kw_snapshot(snapshot);
        if refresh {
            self.refresh_kw_ops_readiness().await;
        }
    }

    /// Keyboard-only fallback: pull status when the watch is unavailable.
    pub(crate) async fn poll_kw_status(&mut self) -> bool {
        let Some(kw) = self.services.kw.clone() else {
            return false;
        };
        let Ok(snapshot) = kw.get_status().await else {
            return false;
        };
        let changed = self.state.kw.status.as_ref() != Some(&snapshot);
        self.apply_kw_snapshot_refreshing_readiness(snapshot).await;
        changed
    }

    /// Seed the projection from GetStatus after the watch is gone.
    ///
    /// A successful query lets polling engage if a job is already running.
    /// A failed query clears status so a dead actor cannot leave a stale
    /// "building" indicator.
    pub(crate) async fn fallback_kw_status(&mut self) {
        let Some(kw) = self.services.kw.clone() else {
            self.state.kw.status = None;
            return;
        };
        match kw.get_status().await {
            Ok(snapshot) => self.apply_kw_snapshot_refreshing_readiness(snapshot).await,
            Err(_) => self.state.kw.status = None,
        }
    }

    pub async fn handle_kw_ops(&mut self, input: InputEvent) -> Result<()> {
        let editing = self.state.kw.ops.as_ref().is_some_and(|ops| ops.editing);
        if editing {
            match input {
                InputEvent::CancelKwOpsEdit => {
                    if let Some(ops) = self.state.kw.ops.as_mut() {
                        ops.cancel_edit();
                    }
                }
                InputEvent::Backspace => {
                    if let Some(ops) = self.state.kw.ops.as_mut() {
                        ops.backspace_edit();
                    }
                }
                InputEvent::TextInput(ch) => {
                    if let Some(ops) = self.state.kw.ops.as_mut() {
                        ops.append_edit(ch);
                    }
                }
                InputEvent::StageKwOpsEdit => {
                    let focus = self.state.kw.ops.as_ref().map(|ops| ops.focus);
                    if let Some(ops) = self.state.kw.ops.as_mut() {
                        ops.commit_edit();
                    }
                    if focus == Some(KwOpsFocus::Branch) {
                        self.refresh_kw_ops_readiness().await;
                    }
                }
                _ => {}
            }
            return Ok(());
        }

        match input {
            InputEvent::OpenHelp => {
                self.state.popup = Some(Self::build_kw_ops_help_popup());
            }
            InputEvent::Back => {
                self.set_current_screen(CurrentScreen::PatchsetDetails);
            }
            InputEvent::NavigateDown => {
                if let Some(ops) = self.state.kw.ops.as_mut() {
                    ops.highlight_next();
                }
            }
            InputEvent::NavigateUp => {
                if let Some(ops) = self.state.kw.ops.as_mut() {
                    ops.highlight_prev();
                }
            }
            InputEvent::EditKwOpsField => {
                if !self.is_job_busy() {
                    if let Some(ops) = self.state.kw.ops.as_mut() {
                        ops.begin_edit();
                    }
                }
            }
            InputEvent::StartKwBuild => self.start_job(KwStartKind::Build).await?,
            InputEvent::StartKwDeploy => self.start_job(KwStartKind::Deploy).await?,
            InputEvent::StartKwBuildThenDeploy => {
                self.start_job(KwStartKind::BuildThenDeploy).await?
            }
            InputEvent::CancelKwJob => self.cancel_job().await?,
            InputEvent::RestoreKwBranch => self.restore_branch().await?,
            _ => {}
        }
        Ok(())
    }

    /// Opens KwOps from Patchset Details. Failures keep Details active.
    pub async fn open_kw_ops(&mut self) -> Result<()> {
        let Some(details) = self.state.lore.details.as_ref() else {
            return Ok(());
        };
        let patchset_title = details.representative_patch.title().clone();
        let message_id = details.representative_patch.message_id().href.clone();

        let Some(kw) = self.services.kw.clone() else {
            self.state.popup = Some(AppPopup::info(
                "Kw operations unavailable",
                "kw jobs require a Unix kw actor, which is not attached.",
            ));
            return Ok(());
        };

        let Some(kernel_tree_id) = self.state.config.target_kernel_tree().clone() else {
            self.state.popup = Some(AppPopup::info(
                "Kw operations unavailable",
                "No target kernel tree is configured. Set target_kernel_tree in the config.",
            ));
            return Ok(());
        };
        let Some(tree) = self.state.config.get_kernel_tree(&kernel_tree_id).cloned() else {
            self.state.popup = Some(AppPopup::info(
                "Kw operations unavailable",
                format!("Target kernel tree '{kernel_tree_id}' is not in the configuration."),
            ));
            return Ok(());
        };

        match kw.get_readiness(&kernel_tree_id, &tree, None).await {
            Ok(readiness) => {
                let reuse = self.state.kw.ops.as_ref().is_some_and(|ops| {
                    ops.message_id == message_id && ops.kernel_tree_id == kernel_tree_id
                });
                if reuse {
                    if let Some(ops) = self.state.kw.ops.as_mut() {
                        ops.reenter(patchset_title, tree, readiness);
                    }
                } else {
                    self.state.kw.ops = Some(KwOpsState::new(
                        patchset_title,
                        message_id,
                        kernel_tree_id,
                        tree,
                        readiness,
                    ));
                }
                self.set_current_screen(CurrentScreen::KwOps);
                self.refresh_kw_ops_log_tail().await;
            }
            Err(error) => {
                self.state.popup = Some(AppPopup::info(
                    "Kw operations unavailable",
                    error.to_string(),
                ));
            }
        }
        Ok(())
    }

    /// Records that the user confirmed boot-once and re-issues the pending
    /// StartDeploy / StartBuildThenDeploy against that acknowledgement.
    pub(crate) async fn resume_pending_deploy(&mut self) -> Result<()> {
        let Some(ops) = self.state.kw.ops.as_mut() else {
            return Ok(());
        };
        ops.boot_once_acknowledged = true;
        let Some(kind) = ops.pending_deploy.take() else {
            return Ok(());
        };
        match kind {
            DeployStartKind::Deploy => self.start_job(KwStartKind::Deploy).await,
            DeployStartKind::BuildThenDeploy => self.start_job(KwStartKind::BuildThenDeploy).await,
        }
    }

    /// Drops a deploy start that was waiting on the boot-once confirm popup.
    pub(crate) fn clear_pending_deploy(&mut self) {
        if let Some(ops) = self.state.kw.ops.as_mut() {
            ops.pending_deploy = None;
        }
    }

    pub fn build_kw_ops_help_popup() -> AppPopup {
        AppPopup::help()
            .title("Kw operations")
            .description(
                "Start a kw build and/or remote deploy on the configured target kernel tree.",
            )
            .keybind("ESC / q", "Return to patchset details")
            .keybind("j/k", "Move between branch and extra arguments")
            .keybind("e / ENTER", "Edit the focused field")
            .keybind("b", "Start build")
            .keybind("d", "Start deploy")
            .keybind("D", "Start build then deploy")
            .keybind("c", "Cancel the running job")
            .keybind("r", "Restore the previous branch")
            .keybind("?", "Show this help screen")
            .build()
    }
}

impl App {
    fn map_tail_read(result: Result<String, FileSystemError>) -> String {
        match result {
            Ok(text) => text,
            Err(FileSystemError::IoError(error)) => {
                if error.kind() == io::ErrorKind::NotFound {
                    String::new()
                } else {
                    format!("(could not read log: {error})")
                }
            }
        }
    }

    fn assign_log_tail(ops: &mut KwOpsState, text: String) -> bool {
        if ops.log_tail == text {
            false
        } else {
            ops.log_tail = text;
            true
        }
    }

    fn is_snapshot_job_terminal(job: &KwJobStatus) -> bool {
        match job {
            KwJobStatus::Succeeded { .. }
            | KwJobStatus::Failed { .. }
            | KwJobStatus::Cancelled { .. } => true,
            KwJobStatus::Idle | KwJobStatus::Running { .. } => false,
        }
    }

    async fn start_job(&mut self, kind: KwStartKind) -> Result<()> {
        if self.is_job_busy() {
            return Ok(());
        }
        let Some(ops) = self.state.kw.ops.as_ref() else {
            return Ok(());
        };
        let branch = ops.branch.trim().to_string();
        if branch.is_empty() {
            let body = if ops.head_unreadable {
                format!(
                    "HEAD is detached or unverifiable. Type a branch name before starting a {}.",
                    kind.action_word()
                )
            } else {
                format!("Set a branch before starting a {}.", kind.action_word())
            };
            self.state.popup = Some(AppPopup::info(kind.title(), body));
            return Ok(());
        }

        // Record/tree refusals before the boot-once confirm so a deploy
        // without a build does not ask the user to proceed, then refuse.
        match kind {
            KwStartKind::Deploy => {
                if let Err(reason) = &ops.readiness.deploy_alone {
                    self.state.popup = Some(AppPopup::info(kind.title(), reason.to_string()));
                    return Ok(());
                }
            }
            KwStartKind::Build | KwStartKind::BuildThenDeploy => {}
        }
        if kind.pending_kind().is_some() && Self::needs_boot_once_confirm(ops) {
            if let Some(ops) = self.state.kw.ops.as_mut() {
                ops.pending_deploy = kind.pending_kind();
            }
            self.state.popup = Some(AppPopup::boot_once_warning());
            return Ok(());
        }

        let reboot = self.state.config.kw_reboot_after_deploy();
        let force = self.state.config.kw_deploy_force();
        let Some(ops) = self.state.kw.ops.as_ref() else {
            return Ok(());
        };
        let request = StartRequest {
            kernel_tree_id: ops.kernel_tree_id.clone(),
            tree: ops.tree.clone(),
            branch,
            extra_args: ops.extra_arg_tokens(),
            deploy: kind.pending_kind().map(|_| DeployOptions {
                reboot,
                force,
                boot_once_acknowledged: ops.boot_once_acknowledged,
            }),
        };
        let Some(kw) = self.services.kw.clone() else {
            self.state.popup = Some(AppPopup::info(
                kind.title(),
                "kw jobs require a Unix kw actor, which is not attached.",
            ));
            return Ok(());
        };
        let result = match kind {
            KwStartKind::Build => kw.start_build(request).await,
            KwStartKind::Deploy => kw.start_deploy(request).await,
            KwStartKind::BuildThenDeploy => kw.start_build_then_deploy(request).await,
        };
        match result {
            Ok(()) => {
                if let Some(ops) = self.state.kw.ops.as_mut() {
                    ops.cancel_requested = false;
                    ops.start_requested = true;
                }
                // Apply the actor's snapshot immediately so keyboard-only
                // mode (no watch) cannot latch on start_requested, and so a
                // second Start in the same loop turn sees the job as busy.
                if let Ok(snapshot) = kw.get_status().await {
                    self.apply_kw_snapshot(snapshot);
                }
            }
            Err(KwStartError::BootOnceNotAcknowledged) => {
                if let Some(ops) = self.state.kw.ops.as_mut() {
                    ops.pending_deploy = kind.pending_kind();
                }
                self.state.popup = Some(AppPopup::boot_once_warning());
            }
            Err(error) => {
                self.state.popup = Some(AppPopup::info(kind.title(), error.to_string()));
            }
        }
        Ok(())
    }

    fn needs_boot_once_confirm(ops: &KwOpsState) -> bool {
        match ops.readiness.boot_once {
            BootOnceState::Off => false,
            BootOnceState::On | BootOnceState::Unknown => !ops.boot_once_acknowledged,
        }
    }

    async fn refresh_kw_ops_readiness(&mut self) {
        let Some(kw) = self.services.kw.clone() else {
            return;
        };
        let Some((kernel_tree_id, tree, branch)) = self.state.kw.ops.as_ref().map(|ops| {
            (
                ops.kernel_tree_id.clone(),
                ops.tree.clone(),
                ops.branch.clone(),
            )
        }) else {
            return;
        };
        let for_branch = {
            let trimmed = branch.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        };

        match kw
            .get_readiness(&kernel_tree_id, &tree, for_branch.as_deref())
            .await
        {
            Ok(readiness) => {
                if let Some(ops) = self.state.kw.ops.as_mut() {
                    ops.readiness = readiness;
                }
            }
            Err(error) => {
                self.state.popup = Some(AppPopup::info(
                    "Cannot refresh kw status",
                    error.to_string(),
                ));
            }
        }
    }

    async fn cancel_job(&mut self) -> Result<()> {
        if !self.is_job_running() {
            return Ok(());
        }
        let Some(kw) = self.services.kw.clone() else {
            return Ok(());
        };
        match kw.cancel().await {
            Ok(()) => {
                if let Some(ops) = self.state.kw.ops.as_mut() {
                    ops.cancel_requested = true;
                }
            }
            Err(KwError::NoJobRunning) => {
                if let Some(ops) = self.state.kw.ops.as_mut() {
                    ops.cancel_requested = false;
                }
            }
            Err(error) => {
                self.state.popup = Some(AppPopup::info("Cannot cancel job", error.to_string()));
            }
        }
        Ok(())
    }

    async fn restore_branch(&mut self) -> Result<()> {
        if self.is_job_running() {
            return Ok(());
        }
        let Some(restore) = self
            .state
            .kw
            .status
            .as_ref()
            .and_then(|status| status.restore_branch.clone())
        else {
            return Ok(());
        };
        let Some(kw) = self.services.kw.clone() else {
            return Ok(());
        };
        match kw.restore_previous_branch().await {
            Ok(()) => {
                if let Some(ops) = self.state.kw.ops.as_mut() {
                    ops.branch = restore;
                    ops.head_unreadable = false;
                }
            }
            Err(error) => {
                self.state.popup = Some(AppPopup::info("Cannot restore branch", error.to_string()));
            }
        }
        Ok(())
    }

    fn is_job_running(&self) -> bool {
        match self.state.kw.status.as_ref().map(|status| &status.job) {
            Some(KwJobStatus::Running { .. }) => true,
            Some(
                KwJobStatus::Idle
                | KwJobStatus::Succeeded { .. }
                | KwJobStatus::Failed { .. }
                | KwJobStatus::Cancelled { .. },
            )
            | None => false,
        }
    }

    fn is_job_busy(&self) -> bool {
        self.is_job_running()
            || self
                .state
                .kw
                .ops
                .as_ref()
                .is_some_and(|ops| ops.start_requested)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::models::kw_ops::KwOpsState;
    use crate::kw::models::readiness::{
        BootOnceState, DeployAloneRefusal, KwBinaryProbe, KwReadiness, KwVersionCheck,
        TreeReadiness,
    };
    use crate::kw::models::remote::RemoteRefusal;

    fn sample_ops() -> KwOpsState {
        KwOpsState::new(
            "title".to_string(),
            "mid".to_string(),
            "linux".to_string(),
            serde_json::from_value(serde_json::json!({
                "path": "/kernel",
                "branch": "main"
            }))
            .expect("json parses"),
            KwReadiness {
                kw_binary: KwBinaryProbe {
                    available: true,
                    version_line: Some("kw, version 0.10.0".to_string()),
                    check: KwVersionCheck::Meets,
                },
                tree: TreeReadiness::Ready {
                    arch: Some("x86_64".to_string()),
                },
                output_dir: None,
                deploy_alone: Err(DeployAloneRefusal::NoBuildRecord),
                current_branch: Some("main".to_string()),
                deploy_remote: Err(RemoteRefusal::NoRemotesConfigured),
                boot_once: BootOnceState::Unknown,
            },
        )
    }

    #[test]
    fn missing_log_is_an_empty_tail_not_a_diagnostic() {
        let missing = FileSystemError::IoError(io::Error::new(io::ErrorKind::NotFound, "missing"));
        assert_eq!("", App::map_tail_read(Err(missing)));
    }

    #[test]
    fn other_read_errors_become_a_panel_diagnostic() {
        let error =
            FileSystemError::IoError(io::Error::new(io::ErrorKind::PermissionDenied, "denied"));
        let text = App::map_tail_read(Err(error));
        assert!(text.contains("could not read log"));
        assert!(text.contains("denied"));
    }

    #[test]
    fn assign_log_tail_reports_whether_text_changed() {
        let mut ops = sample_ops();
        assert!(App::assign_log_tail(&mut ops, "cc1: compiling".to_string()));
        assert!(!App::assign_log_tail(
            &mut ops,
            "cc1: compiling".to_string()
        ));
        assert!(App::assign_log_tail(&mut ops, "done".to_string()));
        assert_eq!("done", ops.log_tail);
    }

    #[test]
    fn help_lists_deploy_keys() {
        let AppPopup::Help {
            description,
            formatted_keybinds,
            ..
        } = App::build_kw_ops_help_popup()
        else {
            panic!("expected help popup");
        };
        assert!(description
            .as_deref()
            .is_some_and(|text| text.contains("remote deploy")));
        assert!(formatted_keybinds.contains("d: Start deploy"));
        assert!(formatted_keybinds.contains("D: Start build then deploy"));
        assert!(!formatted_keybinds.contains("not available yet"));
    }
}
