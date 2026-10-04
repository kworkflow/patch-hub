//! Owned presentation projections built from `AppState` by `App::present`.
//!
//! They are what `App` knows about presentation before `UiCore` turns them
//! into paint-ready `UiScene` nodes.

use ansi_to_tui::IntoText;

use super::{
    models::{kw_ops::KwOpsFocus, popup::AppPopup},
    screens::{details_actions::PatchsetAction, CurrentScreen},
    state::AppState,
};
use crate::kw::{
    argv,
    models::{
        readiness::{BootOnceState, DeployAloneRefusal, KwBinaryProbe, TreeReadiness},
        remote::{KwRemote, RemoteRefusal},
    },
    status::{KwJobStatus, KwPhase, KwStatusSnapshot},
};

use crate::app::models::view_model::{
    AppViewModel, BookmarkedViewModel, ConfigEntryRow, EditConfigViewModel, KwOpsViewModel,
    LatestPatchsetsViewModel, MailingListEntry, MailingListSelectionViewModel, PatchSummaryRow,
    PatchsetDetailsViewModel, PopupViewBody, PopupViewModel, ScreenViewModel, TagTrailerCounts,
    TargetListStatus,
};

/// Builds the full view model the UI renders from the current app state.
///
/// [`App::present`] performs this conversion.
impl From<&AppState> for AppViewModel {
    fn from(state: &AppState) -> Self {
        let screen = ScreenViewModel::from(state);
        let popup = state.popup.as_ref().map(PopupViewModel::from);
        let kw_running = state
            .kw
            .status
            .as_ref()
            .and_then(KwStatusSnapshot::running_indicator);
        AppViewModel {
            screen,
            popup,
            kw_running,
        }
    }
}

impl From<&AppState> for ScreenViewModel {
    fn from(state: &AppState) -> Self {
        match state.navigation.current_screen {
            CurrentScreen::MailingListSelection => {
                ScreenViewModel::MailingListSelection(MailingListSelectionViewModel::from(state))
            }
            CurrentScreen::BookmarkedPatchsets => {
                ScreenViewModel::Bookmarked(BookmarkedViewModel::from(state))
            }
            CurrentScreen::LatestPatchsets => {
                ScreenViewModel::Latest(LatestPatchsetsViewModel::from(state))
            }
            CurrentScreen::PatchsetDetails => {
                ScreenViewModel::PatchsetDetails(PatchsetDetailsViewModel::from(state))
            }
            CurrentScreen::EditConfig => {
                ScreenViewModel::EditConfig(EditConfigViewModel::from(state))
            }
            CurrentScreen::KwOps => ScreenViewModel::KwOps(Box::new(KwOpsViewModel::from(state))),
        }
    }
}

impl From<&AppState> for MailingListSelectionViewModel {
    fn from(state: &AppState) -> Self {
        let mls = &state.lore.mailing_list_selection;

        let target_list_status = if mls.target_list.is_empty() {
            TargetListStatus::Empty
        } else {
            let mut status = TargetListStatus::NoMatch;
            for list in &mls.mailing_lists {
                if list.name().eq(&mls.target_list) {
                    status = TargetListStatus::ExactMatch;
                    break;
                } else if list.name().starts_with(mls.target_list.as_str()) {
                    status = TargetListStatus::PrefixMatch;
                }
            }
            status
        };

        let entries = mls
            .possible_mailing_lists
            .iter()
            .map(|l| MailingListEntry {
                name: l.name().clone(),
                description: l.description().clone(),
            })
            .collect();

        MailingListSelectionViewModel {
            entries,
            highlighted_index: mls.highlighted_list_index,
            target_list: mls.target_list.clone(),
            target_list_status,
        }
    }
}

impl From<&AppState> for BookmarkedViewModel {
    fn from(state: &AppState) -> Self {
        let bs = &state.user_state.bookmarked_patchsets;
        let rows = bs
            .bookmarked_patchsets
            .iter()
            .enumerate()
            .map(|(i, p)| PatchSummaryRow {
                title: p.title().clone(),
                author_name: p.author().name.clone(),
                version: p.version(),
                total_in_series: p.total_in_series(),
                absolute_index: i,
            })
            .collect();
        BookmarkedViewModel {
            rows,
            selected_index: bs.patchset_index,
        }
    }
}

impl From<&AppState> for LatestPatchsetsViewModel {
    fn from(state: &AppState) -> Self {
        let lps = state
            .lore
            .latest_patchsets
            .as_ref()
            .expect("LatestPatchsets must be initialised before projecting");

        let page_number = lps.page_number();
        let base_index = (page_number - 1) * state.config.page_size();

        let rows = lps
            .get_current_patch_feed_page()
            .unwrap_or_default()
            .into_iter()
            .enumerate()
            .map(|(i, p)| PatchSummaryRow {
                title: p.title().clone(),
                author_name: p.author().name.clone(),
                version: p.version(),
                total_in_series: p.total_in_series(),
                absolute_index: base_index + i,
            })
            .collect();

        LatestPatchsetsViewModel {
            rows,
            selected_index: lps.patchset_index(),
            page_number,
            target_list: lps.target_list().to_string(),
        }
    }
}

impl From<&AppState> for PatchsetDetailsViewModel {
    fn from(state: &AppState) -> Self {
        let details = state
            .lore
            .details
            .as_ref()
            .expect("PatchsetDetails must be initialised before projecting");

        let i = details.preview_index;
        let message_id = &details.representative_patch.message_id().href;

        let preview_title = if matches!(
            state.user_state.reviewed_patchsets.get(message_id),
            Some(set) if set.contains(&i)
        ) {
            " Preview [REVIEWED-BY] ".to_string()
        } else if *details.patches_to_reply.get(i).unwrap_or(&false) {
            " Preview [REVIEWED-BY]* ".to_string()
        } else {
            " Preview ".to_string()
        };

        let staged_to_reply = if *details
            .patchset_actions
            .get(&PatchsetAction::ReplyWithReviewedBy)
            .unwrap_or(&false)
        {
            let number_offset = if details.has_cover_letter { 0 } else { 1 };
            let numbers = details
                .patches_to_reply
                .iter()
                .enumerate()
                .filter_map(|(j, &val)| {
                    if val {
                        Some((j + number_offset).to_string())
                    } else {
                        None
                    }
                })
                .collect::<Vec<String>>();
            if numbers.is_empty() {
                None
            } else {
                Some(format!("({})", numbers.join(", ")))
            }
        } else {
            None
        };

        PatchsetDetailsViewModel {
            patch_title: details.representative_patch.title().clone(),
            author_name: details.representative_patch.author().name.clone(),
            version: details.representative_patch.version(),
            patch_count: details.representative_patch.total_in_series(),
            last_updated: details.representative_patch.updated().clone(),
            tag_trailer_counts: TagTrailerCounts {
                reviewed_by: details.reviewed_by[i].len(),
                tested_by: details.tested_by[i].len(),
                acked_by: details.acked_by[i].len(),
            },
            staged_to_reply,
            preview_entries: details
                .patches_preview
                .iter()
                .map(|s| s.as_str().into_text().unwrap_or_default())
                .collect(),
            preview_index: i,
            preview_scroll_offset: details.preview_scroll_offset,
            preview_pan: details.preview_pan,
            preview_fullscreen: details.preview_fullscreen,
            preview_title,
            is_bookmarked: *details
                .patchset_actions
                .get(&PatchsetAction::Bookmark)
                .unwrap_or(&false),
            is_apply_staged: *details
                .patchset_actions
                .get(&PatchsetAction::Apply)
                .unwrap_or(&false),
            is_current_patch_reply_staged: *details.patches_to_reply.get(i).unwrap_or(&false),
        }
    }
}

impl From<&AppState> for EditConfigViewModel {
    fn from(state: &AppState) -> Self {
        let ec = state
            .config_state
            .edit_config
            .as_ref()
            .expect("EditConfig must be initialised before projecting");

        let is_editing_mode = ec.is_editing();
        let highlighted = ec.highlighted();
        let editing_tree_selector = is_editing_mode && ec.highlighted_is_tree_selector();

        let entries = (0..ec.config_count())
            .filter_map(|i| {
                ec.config(i).map(|(label, value)| {
                    let is_highlighted = i == highlighted;
                    let is_editing = is_editing_mode && is_highlighted;
                    let edit_cursor_value = if is_editing {
                        if editing_tree_selector && ec.curr_edit().is_empty() {
                            "<none>".to_string()
                        } else {
                            ec.curr_edit().to_string()
                        }
                    } else {
                        String::new()
                    };
                    ConfigEntryRow {
                        label,
                        value,
                        is_highlighted,
                        is_editing,
                        edit_cursor_value,
                    }
                })
            })
            .collect();

        EditConfigViewModel {
            entries,
            is_editing_mode,
            editing_tree_selector,
        }
    }
}

impl From<&AppState> for KwOpsViewModel {
    fn from(state: &AppState) -> Self {
        let ops = state
            .kw
            .ops
            .as_ref()
            .expect("KwOps must be initialised before projecting");
        let running = match state.kw.status.as_ref().map(|status| &status.job) {
            Some(KwJobStatus::Running { .. }) => true,
            Some(
                KwJobStatus::Idle
                | KwJobStatus::Succeeded { .. }
                | KwJobStatus::Failed { .. }
                | KwJobStatus::Cancelled { .. },
            )
            | None => false,
        };
        let start_requested = ops.start_requested;
        let restore_branch = state
            .kw
            .status
            .as_ref()
            .and_then(|status| status.restore_branch.clone());
        let branch = if ops.editing && ops.focus == KwOpsFocus::Branch {
            ops.edit_buffer.clone()
        } else {
            ops.branch.clone()
        };
        let extra_args = if ops.editing && ops.focus == KwOpsFocus::ExtraArgs {
            ops.edit_buffer.clone()
        } else {
            ops.extra_args.clone()
        };
        let branch_empty = ops.branch.trim().is_empty();
        let kw_available = ops.readiness.kw_binary.available;
        let start_block = ViewModelFormatService::find_start_block_reason(
            running,
            start_requested,
            branch_empty,
            kw_available,
        );
        let start_label = ViewModelFormatService::format_action_label(start_block.clone(), 'b');
        let remote_block = match &ops.readiness.deploy_remote {
            Err(reason) => Some(format!(
                "unavailable ({})",
                ViewModelFormatService::compact_remote_refusal(reason)
            )),
            Ok(_) => None,
        };
        let deploy_block = start_block
            .clone()
            .or_else(|| remote_block.clone())
            .or_else(|| match &ops.readiness.deploy_alone {
                Err(reason) => Some(format!(
                    "unavailable ({})",
                    ViewModelFormatService::compact_deploy_alone_refusal(reason)
                )),
                Ok(()) => None,
            });
        let deploy_label = ViewModelFormatService::format_action_label(deploy_block, 'd');
        let build_deploy_label =
            ViewModelFormatService::format_action_label(start_block.or(remote_block), 'D');
        let cancel_label = if running {
            if ops.cancel_requested {
                "requested; waiting for the job to stop".to_string()
            } else {
                "available (c)".to_string()
            }
        } else {
            "unavailable".to_string()
        };
        let restore_label = match (running, restore_branch.as_deref()) {
            (true, _) => "unavailable (a job is running)".to_string(),
            (false, Some(branch)) => format!("available (r) to {branch}"),
            (false, None) => "unavailable".to_string(),
        };
        let job = state.kw.status.as_ref().map(|status| &status.job);
        let (warnings, first_error) = match job {
            Some(KwJobStatus::Succeeded { warnings, .. }) => {
                if warnings.is_empty() {
                    (None, None)
                } else {
                    (Some(warnings.join(" | ")), None)
                }
            }
            Some(KwJobStatus::Failed { first_error, .. }) => (None, first_error.clone()),
            Some(
                KwJobStatus::Idle | KwJobStatus::Running { .. } | KwJobStatus::Cancelled { .. },
            )
            | None => (None, None),
        };
        let log_path = job
            .and_then(KwJobStatus::log_path)
            .map(|path| path.display().to_string());
        let log_tail = if ops.log_tail.is_empty() {
            if running {
                "Waiting for kw output…".to_string()
            } else {
                "(no log yet)".to_string()
            }
        } else {
            ops.log_tail.clone()
        };

        KwOpsViewModel {
            patchset_title: ops.patchset_title.clone(),
            message_id: ops.message_id.clone(),
            kernel_tree_id: ops.kernel_tree_id.clone(),
            tree_path: ops.tree.path().clone(),
            branch,
            extra_args,
            branch_focused: ops.focus == KwOpsFocus::Branch,
            extras_focused: ops.focus == KwOpsFocus::ExtraArgs,
            editing: ops.editing,
            kw_binary: ViewModelFormatService::format_kw_binary(&ops.readiness.kw_binary),
            tree_readiness: ViewModelFormatService::format_tree_readiness(&ops.readiness.tree),
            output_dir: ops
                .readiness
                .output_dir
                .as_ref()
                .map_or_else(|| "(none)".to_string(), |path| path.display().to_string()),
            job_status: if start_requested && !running {
                "starting…".to_string()
            } else {
                ViewModelFormatService::format_job_status(job, ops.cancel_requested)
            },
            command: format!(
                "kw {}",
                argv::KwArgvService::build_argv(&ops.extra_arg_tokens_for_preview()).join(" ")
            ),
            start_label,
            cancel_label,
            restore_label,
            remote: ViewModelFormatService::format_deploy_remote(&ops.readiness.deploy_remote),
            boot_once: ViewModelFormatService::format_boot_once(
                ops.readiness.boot_once,
                ops.boot_once_acknowledged,
            ),
            deploy_command: ViewModelFormatService::format_deploy_command(
                &ops.readiness.deploy_remote,
                state.config.kw_reboot_after_deploy(),
                state.config.kw_deploy_force(),
                &ops.extra_arg_tokens_for_preview(),
            ),
            deploy_label,
            build_deploy_label,
            branch_guidance: if ops.head_unreadable && ops.branch.trim().is_empty() {
                Some(
                    "HEAD is detached or unverifiable; type a branch before starting a job."
                        .to_string(),
                )
            } else {
                None
            },
            warnings,
            first_error,
            log_path,
            log_tail,
        }
    }
}

struct ViewModelFormatService;

impl ViewModelFormatService {
    fn find_start_block_reason(
        running: bool,
        start_requested: bool,
        branch_empty: bool,
        kw_available: bool,
    ) -> Option<String> {
        if running || start_requested {
            Some("unavailable (a job is already running)".to_string())
        } else if !kw_available {
            Some("unavailable (kw not on PATH)".to_string())
        } else if branch_empty {
            Some("unavailable (set a branch first)".to_string())
        } else {
            None
        }
    }

    fn format_action_label(blocked: Option<String>, key: char) -> String {
        blocked.unwrap_or_else(|| format!("available ({key})"))
    }

    fn format_deploy_remote(remote: &Result<KwRemote, RemoteRefusal>) -> String {
        match remote {
            Ok(remote) => remote.endpoint(),
            Err(reason) => reason.to_string(),
        }
    }

    fn format_boot_once(state: BootOnceState, acknowledged: bool) -> String {
        match (state, acknowledged) {
            (BootOnceState::Off, _) => "off".to_string(),
            (BootOnceState::On, true) => "on (confirmed)".to_string(),
            (BootOnceState::Unknown, true) => "unknown (confirmed)".to_string(),
            (BootOnceState::On, false) => "on (confirm before deploy)".to_string(),
            (BootOnceState::Unknown, false) => "unknown (confirm before deploy)".to_string(),
        }
    }

    fn format_deploy_command(
        remote: &Result<KwRemote, RemoteRefusal>,
        reboot: bool,
        force: bool,
        extra_args: &[String],
    ) -> String {
        match remote {
            Ok(remote) => format!(
                "kw {}",
                argv::KwArgvService::build_deploy_argv(
                    &remote.endpoint(),
                    reboot,
                    force,
                    extra_args
                )
                .join(" ")
            ),
            Err(_) => "(no remote)".to_string(),
        }
    }

    fn compact_remote_refusal(reason: &RemoteRefusal) -> &'static str {
        match reason {
            RemoteRefusal::NoRemotesConfigured => "no remotes configured",
            RemoteRefusal::NoDefault { .. } => "no default remote",
            RemoteRefusal::DefaultNotFound { .. } => "default remote missing",
        }
    }

    fn compact_deploy_alone_refusal(reason: &DeployAloneRefusal) -> &'static str {
        match reason {
            DeployAloneRefusal::TreeNotReady(_) => "tree not ready",
            DeployAloneRefusal::NoBuildRecord => "no build recorded",
            DeployAloneRefusal::LastBuildFailed => "last build failed",
            DeployAloneRefusal::HeadMismatch { .. } => "build was on another branch",
            DeployAloneRefusal::TreePathDrift { .. } => "tree path changed",
            DeployAloneRefusal::OutputDirMismatch => "kw env changed",
            DeployAloneRefusal::ImageMissing => "kernel image missing",
        }
    }

    fn format_kw_binary(probe: &KwBinaryProbe) -> String {
        if !probe.available {
            return "not on PATH".to_string();
        }
        match &probe.version_line {
            Some(line) => line.clone(),
            None => "available (version unknown)".to_string(),
        }
    }

    fn format_tree_readiness(tree: &TreeReadiness) -> String {
        match tree {
            TreeReadiness::Ready { arch: Some(arch) } => format!("ready (arch={arch})"),
            TreeReadiness::Ready { arch: None } => "ready (arch unset)".to_string(),
            other => other.to_string(),
        }
    }

    fn format_job_status(job: Option<&KwJobStatus>, cancel_requested: bool) -> String {
        match job {
            None | Some(KwJobStatus::Idle) => "idle".to_string(),
            Some(KwJobStatus::Running { phase, branch, .. }) => {
                let phase = match phase {
                    KwPhase::Building => "building",
                    KwPhase::Deploying => "deploying",
                };
                if cancel_requested {
                    format!("cancelling {phase} {branch}")
                } else {
                    format!("{phase} {branch}")
                }
            }
            Some(KwJobStatus::Succeeded {
                branch, warnings, ..
            }) => match warnings.len() {
                0 => format!("succeeded on {branch}"),
                1 => format!("succeeded on {branch} with 1 warning"),
                count => format!("succeeded on {branch} with {count} warnings"),
            },
            Some(KwJobStatus::Failed {
                phase: KwPhase::Deploying,
                exit_code,
                ..
            }) => match exit_code {
                Some(code) => match KwJobStatus::find_deploy_exit_hint(*code) {
                    Some(hint) => format!("failed during deploy (exit {code}: {hint})"),
                    None => format!("failed during deploy (exit {code})"),
                },
                None => "failed during deploy (exit unknown)".to_string(),
            },
            Some(KwJobStatus::Failed { exit_code, .. }) => match exit_code {
                Some(code) => format!("failed (exit {code})"),
                None => "failed (exit unknown)".to_string(),
            },
            Some(KwJobStatus::Cancelled { .. }) => "cancelled".to_string(),
        }
    }
}

impl From<&AppPopup> for PopupViewModel {
    fn from(popup: &AppPopup) -> Self {
        match popup {
            AppPopup::Info {
                title,
                body,
                scroll,
                dimensions,
                ..
            } => PopupViewModel {
                title: title.clone(),
                body: PopupViewBody::Text(body.clone()),
                scroll_offset: *scroll,
                dimensions: *dimensions,
            },
            AppPopup::Help {
                title,
                description,
                formatted_keybinds,
                scroll,
                dimensions,
                ..
            } => PopupViewModel {
                title: title.clone().unwrap_or_else(|| "Help".to_string()),
                body: PopupViewBody::Keybinds {
                    description: description.clone(),
                    formatted_keybinds: formatted_keybinds.clone(),
                },
                scroll_offset: *scroll,
                dimensions: *dimensions,
            },
            AppPopup::ReviewTrailers {
                reviewed_by,
                tested_by,
                acked_by,
                scroll,
                dimensions,
                ..
            } => PopupViewModel {
                title: "Code-Review Trailers".to_string(),
                body: PopupViewBody::ReviewTrailers {
                    reviewed_by: reviewed_by.clone(),
                    tested_by: tested_by.clone(),
                    acked_by: acked_by.clone(),
                },
                scroll_offset: *scroll,
                dimensions: *dimensions,
            },
            AppPopup::Confirm {
                title,
                body,
                options,
                selected,
                dimensions,
            } => PopupViewModel {
                title: title.clone(),
                body: PopupViewBody::Confirm {
                    body: body.clone(),
                    options: options.iter().map(|(label, _)| label.clone()).collect(),
                    selected: *selected,
                },
                scroll_offset: (0, 0),
                dimensions: *dimensions,
            },
        }
    }
}

#[cfg(test)]
mod tests {

    mod helpers {
        use super::super::*;
        use crate::{
            app::models::kw_ops::KwOpsState,
            kw::models::readiness::{KwReadiness, KwVersionCheck},
        };
        use crate::{
            app::{
                screens::{
                    bookmarked::BookmarkedPatchsetsState, mail_list::MailingListSelectionState,
                },
                state::{
                    AppState, ConfigUiState, KwUiState, LoreUiState, NavigationState, UserLoreState,
                },
            },
            config::{ConfigSnapshot, ConfigState},
            kw::status::{KwJobKind, KwJobStatus, KwStatusSnapshot},
            lore::domain::mailing_list::MailingList,
        };
        use std::{collections::HashMap, path::PathBuf};

        pub(super) fn app_state_with_kw(status: Option<KwStatusSnapshot>) -> AppState {
            let dummy_list = MailingList::new("test-list", "Test list");
            AppState {
                navigation: NavigationState {
                    current_screen: CurrentScreen::MailingListSelection,
                },
                lore: LoreUiState {
                    mailing_list_selection: MailingListSelectionState {
                        mailing_lists: vec![dummy_list.clone()],
                        target_list: String::new(),
                        possible_mailing_lists: vec![dummy_list],
                        highlighted_list_index: 0,
                    },
                    latest_patchsets: None,
                    details: None,
                },
                user_state: UserLoreState {
                    bookmarked_patchsets: BookmarkedPatchsetsState {
                        bookmarked_patchsets: vec![],
                        patchset_index: 0,
                    },
                    reviewed_patchsets: HashMap::new(),
                },
                config_state: ConfigUiState { edit_config: None },
                config: ConfigSnapshot::from(&ConfigState::default()),
                popup: None,
                kw: KwUiState { status, ops: None },
            }
        }

        pub(super) fn sample_kw_ops(branch: Option<&str>) -> KwOpsState {
            KwOpsState::new(
                "[PATCH] test".to_string(),
                "http://lore.example/123".to_string(),
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
                    current_branch: branch.map(str::to_string),
                    deploy_remote: Err(RemoteRefusal::NoRemotesConfigured),
                    boot_once: BootOnceState::Unknown,
                },
            )
        }

        pub(super) fn sample_remote() -> KwRemote {
            KwRemote {
                name: "dut".to_string(),
                hostname: "box".to_string(),
                port: 22,
                user: Some("root".to_string()),
            }
        }

        pub(super) fn succeeded_deploy(warnings: &[&str]) -> KwStatusSnapshot {
            KwStatusSnapshot {
                job: KwJobStatus::Succeeded {
                    kind: KwJobKind::Deploy,
                    kernel_tree_id: "mainline".to_string(),
                    branch: "patchset-x".to_string(),
                    log_path: PathBuf::from("/tmp/deploy.log"),
                    warnings: warnings.iter().map(|w| w.to_string()).collect(),
                },
                restore_branch: None,
            }
        }
    }
    use helpers::*;
    use std::path::PathBuf;

    use crate::{
        app::{models::kw_ops::KwOpsState, screens::edit_config::EditConfigState},
        kw::models::readiness::{KwReadiness, KwVersionCheck},
    };
    use crate::{
        config::{ConfigSnapshot, ConfigState},
        kw::status::{KwJobKind, KwJobStatus, KwPhase, KwStatusSnapshot},
    };

    use super::*;

    #[test]
    fn running_job_projects_a_global_indicator() {
        let vm = AppViewModel::from(&app_state_with_kw(Some(KwStatusSnapshot {
            job: KwJobStatus::Running {
                kind: KwJobKind::Build,
                phase: KwPhase::Building,
                kernel_tree_id: "mainline".to_string(),
                branch: "patchset-x".to_string(),
                log_path: PathBuf::from("/tmp/build.log"),
            },
            restore_branch: Some("master".to_string()),
        })));

        assert_eq!(Some("kw: building patchset-x".to_string()), vm.kw_running);
    }

    #[test]
    fn confirm_popup_projects_labels_without_actions() {
        let mut state = app_state_with_kw(None);
        state.popup = Some(AppPopup::quit_while_job_running());
        let vm = AppViewModel::from(&state);
        let popup = vm.popup.expect("confirm popup should project");
        assert_eq!("Cancel job and quit?", popup.title);
        let PopupViewBody::Confirm {
            options, selected, ..
        } = popup.body
        else {
            panic!("expected Confirm projection");
        };
        assert_eq!(
            vec!["Cancel and quit".to_string(), "Wait".to_string()],
            options
        );
        assert_eq!(1, selected);
    }

    #[test]
    fn edit_config_projects_none_placeholder_on_the_tree_row() {
        let mut state = app_state_with_kw(None);
        state.navigation.current_screen = CurrentScreen::EditConfig;
        let mut config = ConfigState::default();
        config.kernel_trees.insert(
            "linux".into(),
            serde_json::from_value(serde_json::json!({
                "path": "/linux",
                "branch": "master"
            }))
            .expect("json parses"),
        );
        state.config = ConfigSnapshot::from(&config);
        let mut edit = EditConfigState::new(&state.config);
        while edit.highlighted() != 11 {
            edit.highlight_next();
        }
        edit.toggle_editing();
        state.config_state.edit_config = Some(edit);

        let ScreenViewModel::EditConfig(vm) = AppViewModel::from(&state).screen else {
            panic!("expected EditConfig projection");
        };
        assert!(vm.editing_tree_selector);
        assert_eq!("<none>", vm.entries[11].edit_cursor_value);
        assert_eq!("<none>", vm.entries[11].value);
    }

    #[test]
    fn boot_once_popup_projects_back_out_as_default() {
        let mut state = app_state_with_kw(None);
        state.popup = Some(AppPopup::boot_once_warning());
        let vm = AppViewModel::from(&state);
        let popup = vm.popup.expect("boot-once popup should project");
        assert_eq!("Boot into new kernel once?", popup.title);
        let PopupViewBody::Confirm {
            options, selected, ..
        } = popup.body
        else {
            panic!("expected Confirm projection");
        };
        assert_eq!(vec!["Back out".to_string(), "Proceed".to_string()], options);
        assert_eq!(0, selected);
    }

    #[test]
    fn kw_ops_command_strips_reserved_extras() {
        let mut state = app_state_with_kw(None);
        state.navigation.current_screen = CurrentScreen::KwOps;
        let mut ops = KwOpsState::new(
            "[PATCH] test".to_string(),
            "http://lore.example/123".to_string(),
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
                current_branch: Some("feature".to_string()),
                deploy_remote: Err(RemoteRefusal::NoRemotesConfigured),
                boot_once: BootOnceState::Unknown,
            },
        );
        ops.extra_args = "--verbose --clean --from-sha abc --doc".to_string();
        state.kw.ops = Some(ops);

        let ScreenViewModel::KwOps(vm) = AppViewModel::from(&state).screen else {
            panic!("expected KwOps projection");
        };
        assert_eq!("kw build --verbose", vm.command);
        assert_eq!("feature", vm.branch);
        assert_eq!("available (b)", vm.start_label);
        assert_eq!(
            "no remotes configured; configure a remote with `kw remote --set-default` or edit `.kw/remote.config`",
            vm.remote
        );
        assert_eq!("unknown (confirm before deploy)", vm.boot_once);
        assert_eq!("(no remote)", vm.deploy_command);
        assert_eq!("unavailable (no remotes configured)", vm.deploy_label);
        assert_eq!("unavailable (no remotes configured)", vm.build_deploy_label);
    }

    #[test]
    fn cleared_readable_branch_does_not_claim_detached_head() {
        let mut state = app_state_with_kw(None);
        state.navigation.current_screen = CurrentScreen::KwOps;
        let mut ops = sample_kw_ops(Some("feature"));
        ops.branch.clear();
        state.kw.ops = Some(ops);

        let ScreenViewModel::KwOps(vm) = AppViewModel::from(&state).screen else {
            panic!("expected KwOps projection");
        };
        assert_eq!(None, vm.branch_guidance);
        assert_eq!("unavailable (set a branch first)", vm.start_label);
        assert_eq!("unavailable (set a branch first)", vm.deploy_label);
        assert_eq!("unavailable (set a branch first)", vm.build_deploy_label);
    }

    #[test]
    fn detached_head_projects_branch_guidance() {
        let mut state = app_state_with_kw(None);
        state.navigation.current_screen = CurrentScreen::KwOps;
        state.kw.ops = Some(sample_kw_ops(None));

        let ScreenViewModel::KwOps(vm) = AppViewModel::from(&state).screen else {
            panic!("expected KwOps projection");
        };
        assert!(vm
            .branch_guidance
            .as_deref()
            .is_some_and(|text| text.contains("detached or unverifiable")));
    }

    #[test]
    fn command_preview_tracks_in_progress_extra_args() {
        let mut state = app_state_with_kw(None);
        state.navigation.current_screen = CurrentScreen::KwOps;
        let mut ops = sample_kw_ops(Some("feature"));
        ops.extra_args = "--verbose".to_string();
        ops.highlight_next();
        ops.begin_edit();
        ops.append_edit(' ');
        ops.append_edit('-');
        ops.append_edit('j');
        ops.append_edit('8');
        state.kw.ops = Some(ops);

        let ScreenViewModel::KwOps(vm) = AppViewModel::from(&state).screen else {
            panic!("expected KwOps projection");
        };
        assert_eq!("kw build --verbose -j8", vm.command);
        assert_eq!("--verbose -j8", vm.extra_args);
    }

    #[test]
    fn start_requested_projects_as_busy() {
        let mut state = app_state_with_kw(None);
        state.navigation.current_screen = CurrentScreen::KwOps;
        let mut ops = sample_kw_ops(Some("feature"));
        ops.start_requested = true;
        state.kw.ops = Some(ops);

        let ScreenViewModel::KwOps(vm) = AppViewModel::from(&state).screen else {
            panic!("expected KwOps projection");
        };
        assert_eq!("starting…", vm.job_status);
        assert_eq!("unavailable (a job is already running)", vm.start_label);
        assert_eq!("unavailable (a job is already running)", vm.deploy_label);
        assert_eq!(
            "unavailable (a job is already running)",
            vm.build_deploy_label
        );
    }

    #[test]
    fn deploy_alone_refusal_disables_deploy_but_not_build_then_deploy() {
        let mut state = app_state_with_kw(None);
        state.navigation.current_screen = CurrentScreen::KwOps;
        let mut ops = sample_kw_ops(Some("feature"));
        ops.readiness.deploy_remote = Ok(sample_remote());
        ops.readiness.deploy_alone = Err(DeployAloneRefusal::NoBuildRecord);
        ops.readiness.boot_once = BootOnceState::On;
        state.kw.ops = Some(ops);

        let ScreenViewModel::KwOps(vm) = AppViewModel::from(&state).screen else {
            panic!("expected KwOps projection");
        };
        assert_eq!("root@box:22", vm.remote);
        assert_eq!("on (confirm before deploy)", vm.boot_once);
        assert_eq!(
            "kw deploy --remote root@box:22 --no-reboot --force",
            vm.deploy_command
        );
        assert_eq!("unavailable (no build recorded)", vm.deploy_label);
        assert_eq!("available (D)", vm.build_deploy_label);
    }

    #[test]
    fn matching_record_and_remote_enable_deploy_actions() {
        let mut state = app_state_with_kw(None);
        state.navigation.current_screen = CurrentScreen::KwOps;
        let mut ops = sample_kw_ops(Some("feature"));
        ops.readiness.deploy_remote = Ok(sample_remote());
        ops.readiness.deploy_alone = Ok(());
        ops.readiness.boot_once = BootOnceState::Off;
        ops.extra_args = "--verbose --local --ccache".to_string();
        state.kw.ops = Some(ops);

        let ScreenViewModel::KwOps(vm) = AppViewModel::from(&state).screen else {
            panic!("expected KwOps projection");
        };
        assert_eq!("off", vm.boot_once);
        assert_eq!("available (d)", vm.deploy_label);
        assert_eq!("available (D)", vm.build_deploy_label);
        assert_eq!(
            "kw deploy --remote root@box:22 --no-reboot --force --verbose",
            vm.deploy_command
        );
    }

    #[test]
    fn boot_once_acknowledgement_does_not_change_deploy_availability() {
        let mut state = app_state_with_kw(None);
        state.navigation.current_screen = CurrentScreen::KwOps;
        let mut ops = sample_kw_ops(Some("feature"));
        ops.readiness.deploy_remote = Ok(sample_remote());
        ops.readiness.deploy_alone = Ok(());
        ops.readiness.boot_once = BootOnceState::Unknown;
        ops.boot_once_acknowledged = true;
        state.kw.ops = Some(ops);

        let ScreenViewModel::KwOps(vm) = AppViewModel::from(&state).screen else {
            panic!("expected KwOps projection");
        };
        assert_eq!("unknown (confirmed)", vm.boot_once);
        assert_eq!("available (d)", vm.deploy_label);
    }

    #[test]
    fn deploy_command_follows_reboot_and_force_config() {
        let mut state = app_state_with_kw(None);
        state.navigation.current_screen = CurrentScreen::KwOps;
        state.config = ConfigSnapshot::from(&ConfigState {
            kw_reboot_after_deploy: true,
            kw_deploy_force: false,
            ..Default::default()
        });
        let mut ops = sample_kw_ops(Some("feature"));
        ops.readiness.deploy_remote = Ok(sample_remote());
        state.kw.ops = Some(ops);

        let ScreenViewModel::KwOps(vm) = AppViewModel::from(&state).screen else {
            panic!("expected KwOps projection");
        };
        assert_eq!("kw deploy --remote root@box:22 --reboot", vm.deploy_command);
    }

    #[test]
    fn failed_deploy_projects_known_exit_hint() {
        let mut state = app_state_with_kw(Some(KwStatusSnapshot {
            job: KwJobStatus::Failed {
                kind: KwJobKind::Deploy,
                phase: KwPhase::Deploying,
                exit_code: Some(101),
                log_path: PathBuf::from("/tmp/deploy.log"),
                first_error: None,
            },
            restore_branch: None,
        }));
        state.navigation.current_screen = CurrentScreen::KwOps;
        let mut ops = sample_kw_ops(Some("feature"));
        ops.readiness.deploy_remote = Ok(sample_remote());
        state.kw.ops = Some(ops);

        let ScreenViewModel::KwOps(vm) = AppViewModel::from(&state).screen else {
            panic!("expected KwOps projection");
        };
        assert_eq!(
            "failed during deploy (exit 101: SSH unreachable after setup)",
            vm.job_status
        );
    }

    #[test]
    fn failed_build_keeps_the_generic_exit_line() {
        let mut state = app_state_with_kw(Some(KwStatusSnapshot {
            job: KwJobStatus::Failed {
                kind: KwJobKind::Build,
                phase: KwPhase::Building,
                exit_code: Some(1),
                log_path: PathBuf::from("/tmp/build.log"),
                first_error: None,
            },
            restore_branch: None,
        }));
        state.navigation.current_screen = CurrentScreen::KwOps;
        state.kw.ops = Some(sample_kw_ops(Some("feature")));

        let ScreenViewModel::KwOps(vm) = AppViewModel::from(&state).screen else {
            panic!("expected KwOps projection");
        };
        assert_eq!("failed (exit 1)", vm.job_status);
        assert_eq!(None, vm.first_error);
        assert_eq!(Some("/tmp/build.log".to_string()), vm.log_path);
    }

    #[test]
    fn failed_build_projects_first_error_and_log_path() {
        let mut state = app_state_with_kw(Some(KwStatusSnapshot {
            job: KwJobStatus::Failed {
                kind: KwJobKind::Build,
                phase: KwPhase::Building,
                exit_code: Some(2),
                log_path: PathBuf::from("/tmp/build.log"),
                first_error: Some("init/main.c:1:2: error: #error broken".to_string()),
            },
            restore_branch: None,
        }));
        state.navigation.current_screen = CurrentScreen::KwOps;
        state.kw.ops = Some(sample_kw_ops(Some("feature")));

        let ScreenViewModel::KwOps(vm) = AppViewModel::from(&state).screen else {
            panic!("expected KwOps projection");
        };
        assert_eq!("failed (exit 2)", vm.job_status);
        assert_eq!(
            Some("init/main.c:1:2: error: #error broken".to_string()),
            vm.first_error
        );
        assert_eq!(Some("/tmp/build.log".to_string()), vm.log_path);
        assert_eq!(None, vm.warnings);
    }

    #[test]
    fn deploy_with_warnings_projects_count_and_joined_warnings() {
        let mut state = app_state_with_kw(Some(succeeded_deploy(&[
            "update-initramfs: failed for /boot/initrd.img-7.2.0 with 1.",
            "GRUB did not list kernel 7.2.0",
        ])));
        state.navigation.current_screen = CurrentScreen::KwOps;
        state.kw.ops = Some(sample_kw_ops(Some("feature")));

        let ScreenViewModel::KwOps(vm) = AppViewModel::from(&state).screen else {
            panic!("expected KwOps projection");
        };
        assert_eq!("succeeded on patchset-x with 2 warnings", vm.job_status);
        assert_eq!(
            Some(
                "update-initramfs: failed for /boot/initrd.img-7.2.0 with 1. | \
                 GRUB did not list kernel 7.2.0"
                    .to_string()
            ),
            vm.warnings
        );
        assert_eq!(None, vm.first_error);
        assert_eq!(Some("/tmp/deploy.log".to_string()), vm.log_path);
    }

    #[test]
    fn clean_success_projects_no_warning_rows() {
        let mut state = app_state_with_kw(Some(succeeded_deploy(&[])));
        state.navigation.current_screen = CurrentScreen::KwOps;
        state.kw.ops = Some(sample_kw_ops(Some("feature")));

        let ScreenViewModel::KwOps(vm) = AppViewModel::from(&state).screen else {
            panic!("expected KwOps projection");
        };
        assert_eq!("succeeded on patchset-x", vm.job_status);
        assert_eq!(None, vm.warnings);

        state.kw.status = Some(succeeded_deploy(&["GRUB did not list kernel 7.2.0"]));
        let ScreenViewModel::KwOps(vm) = AppViewModel::from(&state).screen else {
            panic!("expected KwOps projection");
        };
        assert_eq!("succeeded on patchset-x with 1 warning", vm.job_status);
    }

    #[test]
    fn idle_job_projects_no_log_path() {
        let mut state = app_state_with_kw(Some(KwStatusSnapshot::idle()));
        state.navigation.current_screen = CurrentScreen::KwOps;
        state.kw.ops = Some(sample_kw_ops(Some("feature")));

        let ScreenViewModel::KwOps(vm) = AppViewModel::from(&state).screen else {
            panic!("expected KwOps projection");
        };
        assert_eq!(None, vm.log_path);
        assert_eq!(None, vm.warnings);
        assert_eq!(None, vm.first_error);
    }

    #[test]
    fn idle_succeeded_and_missing_status_have_no_global_indicator() {
        assert_eq!(
            None,
            AppViewModel::from(&app_state_with_kw(None)).kw_running
        );
        assert_eq!(
            None,
            AppViewModel::from(&app_state_with_kw(Some(KwStatusSnapshot::idle()))).kw_running
        );
        assert_eq!(
            None,
            AppViewModel::from(&app_state_with_kw(Some(KwStatusSnapshot {
                job: KwJobStatus::Succeeded {
                    kind: KwJobKind::Build,
                    kernel_tree_id: "mainline".to_string(),
                    branch: "patchset-x".to_string(),
                    log_path: PathBuf::from("/tmp/build.log"),
                    warnings: Vec::new(),
                },
                restore_branch: Some("master".to_string()),
            })))
            .kw_running
        );
    }

    #[test]
    fn missing_kw_binary_disables_start_labels() {
        let mut state = app_state_with_kw(None);
        state.navigation.current_screen = CurrentScreen::KwOps;
        let mut ops = sample_kw_ops(Some("feature"));
        ops.readiness.kw_binary.available = false;
        ops.readiness.kw_binary.version_line = None;
        state.kw.ops = Some(ops);

        let ScreenViewModel::KwOps(vm) = AppViewModel::from(&state).screen else {
            panic!("expected KwOps projection");
        };
        assert_eq!("unavailable (kw not on PATH)", vm.start_label);
        assert_eq!("unavailable (kw not on PATH)", vm.deploy_label);
        assert_eq!("unavailable (kw not on PATH)", vm.build_deploy_label);
    }
}
