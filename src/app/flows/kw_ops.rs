use std::sync::Arc;

use color_eyre::Result;

use crate::{
    app::{
        popup::AppPopup,
        screens::{kw_ops::KwOpsState, CurrentScreen},
        App,
    },
    infrastructure::file_system::FileSystemError,
    input::event::InputEvent,
    kw::{
        errors::KwError,
        messages::StartRequest,
        status::{KwJobStatus, KwStatusSnapshot},
    },
};

/// Bytes read from the end of a kw job log. Kernel build logs can be
/// far larger than this; the TUI only needs a bounded tail.
pub(crate) const LOG_TAIL_MAX_BYTES: usize = 64 * 1024;

/// Refresh the KwOps log panel from the current job's log file.
///
/// A missing file is normal just after accept (`Waiting for kw output…`
/// is projected from an empty tail). Other read errors become a
/// non-fatal diagnostic in the panel and do not change job status.
/// Returns whether the displayed text changed.
///
/// The read runs on the blocking pool: the log is a real file, and
/// AppActor's loop must not stall on disk I/O.
pub(crate) async fn refresh_kw_ops_log_tail(app: &mut App) -> bool {
    if app.state.kw.ops.is_none() {
        return false;
    }
    let Some(path) = app
        .state
        .kw
        .status
        .as_ref()
        .and_then(|status| status.job.log_path())
        .map(std::path::PathBuf::from)
    else {
        return false;
    };
    let fs = Arc::clone(&app.services.fs);
    let result =
        tokio::task::spawn_blocking(move || fs.read_tail_to_string(&path, LOG_TAIL_MAX_BYTES))
            .await;
    let text = match result {
        Ok(read) => tail_text_from_read(read),
        Err(error) => format!("(could not read log: {error})"),
    };
    let Some(ops) = app.state.kw.ops.as_mut() else {
        return false;
    };
    assign_log_tail(ops, text)
}

fn tail_text_from_read(result: Result<String, FileSystemError>) -> String {
    match result {
        Ok(text) => text,
        Err(FileSystemError::IoError(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            String::new()
        }
        Err(error) => format!("(could not read log: {error})"),
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

/// Projects a kw snapshot into App state and releases the optimistic
/// start lock once the actor has left Idle.
pub(crate) fn apply_kw_snapshot(app: &mut App, snapshot: KwStatusSnapshot) {
    if let Some(ops) = app.state.kw.ops.as_mut() {
        if !matches!(snapshot.job, KwJobStatus::Idle) {
            ops.start_requested = false;
        }
    }
    app.state.kw.status = Some(snapshot);
}

/// Keyboard-only fallback: pull status when the watch is unavailable.
pub(crate) async fn poll_kw_status(app: &mut App) -> bool {
    let Some(kw) = app.services.kw.clone() else {
        return false;
    };
    let Ok(snapshot) = kw.get_status().await else {
        return false;
    };
    let changed = app.state.kw.status.as_ref() != Some(&snapshot);
    apply_kw_snapshot(app, snapshot);
    changed
}

/// Seed the projection from GetStatus after the watch is gone.
///
/// A successful query lets the poll arm engage if a job is already
/// running (the subscribe-failed-with-live-job corner). A failed query
/// clears status so a dead actor cannot leave a stale "building"
/// indicator.
pub(crate) async fn fallback_kw_status(app: &mut App) {
    let Some(kw) = app.services.kw.clone() else {
        app.state.kw.status = None;
        return;
    };
    match kw.get_status().await {
        Ok(snapshot) => apply_kw_snapshot(app, snapshot),
        Err(_) => app.state.kw.status = None,
    }
}

pub async fn handle_kw_ops(app: &mut App, input: InputEvent) -> Result<()> {
    let editing = app.state.kw.ops.as_ref().is_some_and(|ops| ops.editing);
    if editing {
        let Some(ops) = app.state.kw.ops.as_mut() else {
            return Ok(());
        };
        match input {
            InputEvent::CancelKwOpsEdit => ops.cancel_edit(),
            InputEvent::Backspace => ops.backspace_edit(),
            InputEvent::TextInput(ch) => ops.append_edit(ch),
            InputEvent::StageKwOpsEdit => ops.commit_edit(),
            _ => {}
        }
        return Ok(());
    }

    match input {
        InputEvent::OpenHelp => {
            app.state.popup = Some(generate_help_popup());
        }
        InputEvent::Back => {
            app.set_current_screen(CurrentScreen::PatchsetDetails);
        }
        InputEvent::NavigateDown => {
            if let Some(ops) = app.state.kw.ops.as_mut() {
                ops.highlight_next();
            }
        }
        InputEvent::NavigateUp => {
            if let Some(ops) = app.state.kw.ops.as_mut() {
                ops.highlight_prev();
            }
        }
        InputEvent::EditKwOpsField => {
            if !job_is_busy(app) {
                if let Some(ops) = app.state.kw.ops.as_mut() {
                    ops.begin_edit();
                }
            }
        }
        InputEvent::StartKwBuild => start_build(app).await?,
        InputEvent::CancelKwJob => cancel_job(app).await?,
        InputEvent::RestoreKwBranch => restore_branch(app).await?,
        _ => {}
    }
    Ok(())
}

/// Opens KwOps from Patchset Details. Failures keep Details active.
pub async fn open_kw_ops(app: &mut App) -> Result<()> {
    let Some(details) = app.state.lore.details.as_ref() else {
        return Ok(());
    };
    let patchset_title = details.representative_patch.title().clone();
    let message_id = details.representative_patch.message_id().href.clone();

    let Some(kw) = app.services.kw.clone() else {
        app.state.popup = Some(AppPopup::info(
            "Kw operations unavailable",
            "kw jobs require a Unix kw actor, which is not attached.",
        ));
        return Ok(());
    };

    let Some(kernel_tree_id) = app.state.config.target_kernel_tree().clone() else {
        app.state.popup = Some(AppPopup::info(
            "Kw operations unavailable",
            "No target kernel tree is configured. Set target_kernel_tree in the config.",
        ));
        return Ok(());
    };
    let Some(tree) = app.state.config.get_kernel_tree(&kernel_tree_id).cloned() else {
        app.state.popup = Some(AppPopup::info(
            "Kw operations unavailable",
            format!("Target kernel tree '{kernel_tree_id}' is not in the configuration."),
        ));
        return Ok(());
    };

    match kw.get_readiness(&kernel_tree_id, &tree, None).await {
        Ok(readiness) => {
            let reuse = app.state.kw.ops.as_ref().is_some_and(|ops| {
                ops.message_id == message_id && ops.kernel_tree_id == kernel_tree_id
            });
            if reuse {
                if let Some(ops) = app.state.kw.ops.as_mut() {
                    ops.reenter(patchset_title, tree, readiness);
                }
            } else {
                app.state.kw.ops = Some(KwOpsState::new(
                    patchset_title,
                    message_id,
                    kernel_tree_id,
                    tree,
                    readiness,
                ));
            }
            app.set_current_screen(CurrentScreen::KwOps);
            refresh_kw_ops_log_tail(app).await;
        }
        Err(error) => {
            app.state.popup = Some(AppPopup::info(
                "Kw operations unavailable",
                error.to_string(),
            ));
        }
    }
    Ok(())
}

async fn start_build(app: &mut App) -> Result<()> {
    if job_is_busy(app) {
        return Ok(());
    }
    let Some(ops) = app.state.kw.ops.as_ref() else {
        return Ok(());
    };
    let branch = ops.branch.trim().to_string();
    if branch.is_empty() {
        let body = if ops.head_unreadable {
            "HEAD is detached or unverifiable. Type a branch name before starting a build."
        } else {
            "Set a branch before starting a build."
        };
        app.state.popup = Some(AppPopup::info("Cannot start build", body));
        return Ok(());
    }
    let Some(kw) = app.services.kw.clone() else {
        app.state.popup = Some(AppPopup::info(
            "Cannot start build",
            "kw jobs require a Unix kw actor, which is not attached.",
        ));
        return Ok(());
    };
    let request = StartRequest {
        kernel_tree_id: ops.kernel_tree_id.clone(),
        tree: ops.tree.clone(),
        branch,
        extra_args: ops.extra_arg_tokens(),
        deploy: None,
    };
    match kw.start_build(request).await {
        Ok(()) => {
            if let Some(ops) = app.state.kw.ops.as_mut() {
                ops.cancel_requested = false;
                ops.start_requested = true;
            }
            // Apply the actor's snapshot immediately so keyboard-only
            // mode (no watch) cannot latch on start_requested, and so a
            // second Start in the same loop turn sees the job as busy.
            if let Ok(snapshot) = kw.get_status().await {
                apply_kw_snapshot(app, snapshot);
            }
        }
        Err(error) => {
            app.state.popup = Some(AppPopup::info("Cannot start build", error.to_string()));
        }
    }
    Ok(())
}

async fn cancel_job(app: &mut App) -> Result<()> {
    if !job_is_running(app) {
        return Ok(());
    }
    let Some(kw) = app.services.kw.clone() else {
        return Ok(());
    };
    match kw.cancel().await {
        Ok(()) => {
            if let Some(ops) = app.state.kw.ops.as_mut() {
                ops.cancel_requested = true;
            }
        }
        Err(KwError::NoJobRunning) => {
            if let Some(ops) = app.state.kw.ops.as_mut() {
                ops.cancel_requested = false;
            }
        }
        Err(error) => {
            app.state.popup = Some(AppPopup::info("Cannot cancel job", error.to_string()));
        }
    }
    Ok(())
}

async fn restore_branch(app: &mut App) -> Result<()> {
    if job_is_running(app) {
        return Ok(());
    }
    let Some(restore) = app
        .state
        .kw
        .status
        .as_ref()
        .and_then(|status| status.restore_branch.clone())
    else {
        return Ok(());
    };
    let Some(kw) = app.services.kw.clone() else {
        return Ok(());
    };
    match kw.restore_previous_branch().await {
        Ok(()) => {
            if let Some(ops) = app.state.kw.ops.as_mut() {
                ops.branch = restore;
                ops.head_unreadable = false;
            }
        }
        Err(error) => {
            app.state.popup = Some(AppPopup::info("Cannot restore branch", error.to_string()));
        }
    }
    Ok(())
}

fn job_is_running(app: &App) -> bool {
    matches!(
        app.state.kw.status.as_ref().map(|status| &status.job),
        Some(KwJobStatus::Running { .. })
    )
}

fn job_is_busy(app: &App) -> bool {
    job_is_running(app)
        || app
            .state
            .kw
            .ops
            .as_ref()
            .is_some_and(|ops| ops.start_requested)
}

/// Records that the user confirmed boot-once and consumes the pending
/// deploy kind. The KwOps input commit re-issues Start against this
/// acknowledgement; until then Proceed only stores the ack.
pub(crate) async fn resume_pending_deploy(app: &mut App) -> Result<()> {
    let Some(ops) = app.state.kw.ops.as_mut() else {
        return Ok(());
    };
    ops.boot_once_acknowledged = true;
    let _kind = ops.pending_deploy.take();
    Ok(())
}

/// Drops a deploy start that was waiting on the boot-once confirm popup.
pub(crate) fn clear_pending_deploy(app: &mut App) {
    if let Some(ops) = app.state.kw.ops.as_mut() {
        ops.pending_deploy = None;
    }
}

pub fn generate_help_popup() -> AppPopup {
    AppPopup::help()
        .title("Kw operations")
        .description(
            "Start a kw build on the configured target kernel tree. Deploy is not available yet.",
        )
        .keybind("ESC / q", "Return to patchset details")
        .keybind("j/k", "Move between branch and extra arguments")
        .keybind("e / ENTER", "Edit the focused field")
        .keybind("b", "Start build")
        .keybind("c", "Cancel the running job")
        .keybind("r", "Restore the previous branch")
        .keybind("?", "Show this help screen")
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::screens::kw_ops::KwOpsState;
    use crate::kw::readiness::{
        BootOnceState, DeployAloneRefusal, KwBinaryProbe, KwReadiness, KwVersionCheck,
        TreeReadiness,
    };
    use crate::kw::remote::RemoteRefusal;

    fn sample_ops() -> KwOpsState {
        KwOpsState::new(
            "title".to_string(),
            "mid".to_string(),
            "linux".to_string(),
            serde_json::from_value(serde_json::json!({
                "path": "/kernel",
                "branch": "main"
            }))
            .unwrap(),
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
                kernel_image: None,
                build_record: None,
                latest_build: None,
                deploy_alone: Err(DeployAloneRefusal::NoBuildRecord),
                current_branch: Some("main".to_string()),
                deploy_remote: Err(RemoteRefusal::NoRemotesConfigured),
                boot_once: BootOnceState::Unknown,
            },
        )
    }

    #[test]
    fn missing_log_is_an_empty_tail_not_a_diagnostic() {
        let missing =
            FileSystemError::IoError(std::io::Error::new(std::io::ErrorKind::NotFound, "missing"));
        assert_eq!("", tail_text_from_read(Err(missing)));
    }

    #[test]
    fn other_read_errors_become_a_panel_diagnostic() {
        let error = FileSystemError::IoError(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "denied",
        ));
        let text = tail_text_from_read(Err(error));
        assert!(text.contains("could not read log"));
        assert!(text.contains("denied"));
    }

    #[test]
    fn assign_log_tail_reports_whether_text_changed() {
        let mut ops = sample_ops();
        assert!(assign_log_tail(&mut ops, "cc1: compiling".to_string()));
        assert!(!assign_log_tail(&mut ops, "cc1: compiling".to_string()));
        assert!(assign_log_tail(&mut ops, "done".to_string()));
        assert_eq!("done", ops.log_tail);
    }
}
