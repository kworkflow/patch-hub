use color_eyre::Result;

use crate::{
    app::{
        popup::AppPopup,
        screens::{kw_ops::KwOpsState, CurrentScreen},
        App,
    },
    input::event::InputEvent,
    kw::{errors::KwError, messages::StartRequest, status::KwJobStatus},
};

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
            if !job_is_running(app) {
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

    match kw.get_readiness(&kernel_tree_id, &tree).await {
        Ok(readiness) => {
            app.state.kw.ops = Some(KwOpsState::new(
                patchset_title,
                message_id,
                kernel_tree_id,
                tree,
                readiness,
            ));
            app.set_current_screen(CurrentScreen::KwOps);
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
    if job_is_running(app) {
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
    };
    match kw.start_build(request).await {
        Ok(()) => {
            if let Some(ops) = app.state.kw.ops.as_mut() {
                ops.cancel_requested = false;
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
