//! Owned, typed presentation projections.
//!
//! [`AppViewModel`] is built from [`super::state::AppState`] by
//! [`project_state`], which is called via [`super::App::present`].
//!
//! These types sit at the *application* layer: they represent what `App` knows
//! about the presentation before `UiCore` translates them into paint-ready
//! `UiScene` nodes.

use ansi_to_tui::IntoText;
use ratatui::text::Text;

use super::{
    popup::AppPopup,
    screens::{details_actions::PatchsetAction, kw_ops::KwOpsFocus, CurrentScreen},
    state::AppState,
};
use crate::kw::{
    argv,
    readiness::TreeReadiness,
    status::{KwJobStatus, KwStatusSnapshot},
};

/// One mailing list entry shown in the selection list.
#[derive(Clone, Debug)]
pub struct MailingListEntry {
    pub name: String,
    pub description: String,
}

/// Match state of the user's mailing-list filter string.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TargetListStatus {
    /// Nothing typed yet.
    Empty,
    /// The typed string exactly names an existing list.
    ExactMatch,
    /// The typed string is a prefix of at least one existing list.
    PrefixMatch,
    /// The typed string matches no list.
    NoMatch,
}

/// A single row in a paginated patchset list (Latest or Bookmarked).
#[derive(Clone, Debug)]
pub struct PatchSummaryRow {
    pub title: String,
    pub author_name: String,
    pub version: usize,
    pub total_in_series: usize,
    /// Absolute index across pages.
    pub absolute_index: usize,
}

/// Pre-computed counts of code-review trailers for the currently previewed patch.
#[derive(Clone, Debug)]
pub struct TagTrailerCounts {
    pub reviewed_by: usize,
    pub tested_by: usize,
    pub acked_by: usize,
}

/// One row in the edit-config form.
#[derive(Clone, Debug)]
pub struct ConfigEntryRow {
    pub label: String,
    pub value: String,
    pub is_highlighted: bool,
    /// Whether this row is currently being typed into.
    pub is_editing: bool,
    /// In-progress text while editing; empty when not editing this row.
    pub edit_cursor_value: String,
}

#[derive(Clone, Debug)]
pub struct MailingListSelectionViewModel {
    pub entries: Vec<MailingListEntry>,
    pub highlighted_index: usize,
    pub target_list: String,
    pub target_list_status: TargetListStatus,
}

#[derive(Clone, Debug)]
pub struct BookmarkedViewModel {
    pub rows: Vec<PatchSummaryRow>,
    pub selected_index: usize,
}

#[derive(Clone, Debug)]
pub struct LatestPatchsetsViewModel {
    pub rows: Vec<PatchSummaryRow>,
    pub selected_index: usize,
    pub page_number: usize,
    pub target_list: String,
}

#[derive(Clone, Debug)]
pub struct PatchsetDetailsViewModel {
    pub patch_title: String,
    pub author_name: String,
    pub version: usize,
    pub patch_count: usize,
    pub last_updated: String,
    pub tag_trailer_counts: TagTrailerCounts,
    /// Pre-computed `"(0, 2, …)"` string when patches are staged for reply.
    /// `None` when nothing is staged.
    pub staged_to_reply: Option<String>,
    /// ANSI-rendered diff/cover text for each patch entry.
    pub preview_entries: Vec<Text<'static>>,
    pub preview_index: usize,
    pub preview_scroll_offset: usize,
    pub preview_pan: usize,
    pub preview_fullscreen: bool,
    /// Pre-computed title for the preview pane, including the `[REVIEWED-BY]`
    /// or `[REVIEWED-BY]*` suffix when applicable.
    pub preview_title: String,
    pub is_bookmarked: bool,
    pub is_apply_staged: bool,
    /// Whether the patch at `preview_index` is staged for a Reviewed-by reply.
    pub is_current_patch_reply_staged: bool,
}

#[derive(Clone, Debug)]
pub struct EditConfigViewModel {
    pub entries: Vec<ConfigEntryRow>,
    pub is_editing_mode: bool,
}

/// KwOps dashboard. Labels only; actions stay in AppState.
#[derive(Clone, Debug)]
pub struct KwOpsViewModel {
    pub patchset_title: String,
    pub message_id: String,
    pub kernel_tree_id: String,
    pub tree_path: String,
    pub branch: String,
    pub extra_args: String,
    pub branch_focused: bool,
    pub extras_focused: bool,
    pub editing: bool,
    pub kw_binary: String,
    pub tree_readiness: String,
    pub output_dir: String,
    pub job_status: String,
    pub command: String,
    pub start_label: String,
    pub cancel_label: String,
    pub restore_label: String,
    pub deploy_placeholder: String,
    pub branch_guidance: Option<String>,
    pub log_tail: String,
}

#[derive(Clone, Debug)]
pub enum PopupViewBody {
    Text(String),
    Keybinds {
        description: Option<String>,
        formatted_keybinds: String,
    },
    ReviewTrailers {
        reviewed_by: String,
        tested_by: String,
        acked_by: String,
    },
    /// Choice labels only; the selected action stays in `AppPopup`.
    Confirm {
        body: String,
        options: Vec<String>,
        selected: usize,
    },
}

#[derive(Clone, Debug)]
pub struct PopupViewModel {
    pub title: String,
    pub body: PopupViewBody,
    pub scroll_offset: (u16, u16),
    /// `(width_percent, height_percent)` of the terminal area.
    pub dimensions: (u16, u16),
}

#[derive(Clone, Debug)]
pub enum ScreenViewModel {
    MailingListSelection(MailingListSelectionViewModel),
    Bookmarked(BookmarkedViewModel),
    Latest(LatestPatchsetsViewModel),
    PatchsetDetails(PatchsetDetailsViewModel),
    EditConfig(EditConfigViewModel),
    KwOps(KwOpsViewModel),
}

/// Owned, typed projection of [`AppState`] for one TUI frame.
///
/// Built by [`project_state`]; consumed by `UiCore::build_scene`.
#[derive(Clone, Debug)]
pub struct AppViewModel {
    pub screen: ScreenViewModel,
    pub popup: Option<PopupViewModel>,
    /// Compact running-job copy for the nav bar. `None` when idle or
    /// after a terminal outcome — those belong on KwOps, not globally.
    pub kw_running: Option<String>,
}

/// Projects `state` into an owned [`AppViewModel`].
///
/// Called via [`super::App::present`].
pub fn project_state(state: &AppState) -> AppViewModel {
    let screen = project_screen(state);
    let popup = state.popup.as_ref().map(project_popup);
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
fn project_screen(state: &AppState) -> ScreenViewModel {
    match state.navigation.current_screen {
        CurrentScreen::MailingListSelection => {
            ScreenViewModel::MailingListSelection(project_mail_list(state))
        }
        CurrentScreen::BookmarkedPatchsets => {
            ScreenViewModel::Bookmarked(project_bookmarked(state))
        }
        CurrentScreen::LatestPatchsets => ScreenViewModel::Latest(project_latest(state)),
        CurrentScreen::PatchsetDetails => ScreenViewModel::PatchsetDetails(project_details(state)),
        CurrentScreen::EditConfig => ScreenViewModel::EditConfig(project_edit_config(state)),
        CurrentScreen::KwOps => ScreenViewModel::KwOps(project_kw_ops(state)),
    }
}

fn project_mail_list(state: &AppState) -> MailingListSelectionViewModel {
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

fn project_bookmarked(state: &AppState) -> BookmarkedViewModel {
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

fn project_latest(state: &AppState) -> LatestPatchsetsViewModel {
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

fn project_details(state: &AppState) -> PatchsetDetailsViewModel {
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
        let numbers: Vec<String> = details
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
            .collect();
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

fn project_edit_config(state: &AppState) -> EditConfigViewModel {
    let ec = state
        .config_state
        .edit_config
        .as_ref()
        .expect("EditConfig must be initialised before projecting");

    let is_editing_mode = ec.is_editing();
    let highlighted = ec.highlighted();

    let entries = (0..ec.config_count())
        .filter_map(|i| {
            ec.config(i).map(|(label, value)| {
                let is_highlighted = i == highlighted;
                let is_editing = is_editing_mode && is_highlighted;
                let edit_cursor_value = if is_editing {
                    ec.curr_edit().to_string()
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
    }
}

fn project_kw_ops(state: &AppState) -> KwOpsViewModel {
    let ops = state
        .kw
        .ops
        .as_ref()
        .expect("KwOps must be initialised before projecting");
    let running = matches!(
        state.kw.status.as_ref().map(|status| &status.job),
        Some(KwJobStatus::Running { .. })
    );
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
    let start_label = if running || start_requested {
        "unavailable (a job is already running)".to_string()
    } else if ops.branch.trim().is_empty() {
        "unavailable (set a branch first)".to_string()
    } else {
        "available (b)".to_string()
    };
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
        kw_binary: format_kw_binary(&ops.readiness.kw_binary),
        tree_readiness: format_tree_readiness(&ops.readiness.tree),
        output_dir: ops
            .readiness
            .output_dir
            .as_ref()
            .map_or_else(|| "(none)".to_string(), |path| path.display().to_string()),
        job_status: if start_requested && !running {
            "starting…".to_string()
        } else {
            format_job_status(
                state.kw.status.as_ref().map(|status| &status.job),
                ops.cancel_requested,
            )
        },
        command: format!(
            "kw {}",
            argv::build_argv(&ops.extra_arg_tokens_for_preview()).join(" ")
        ),
        start_label,
        cancel_label,
        restore_label,
        deploy_placeholder: "not available yet".to_string(),
        branch_guidance: if ops.head_unreadable && ops.branch.trim().is_empty() {
            Some(
                "HEAD is detached or unverifiable; type a branch before starting a build."
                    .to_string(),
            )
        } else {
            None
        },
        log_tail,
    }
}

fn format_kw_binary(probe: &crate::kw::readiness::KwBinaryProbe) -> String {
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
                crate::kw::status::KwPhase::Building => "building",
                crate::kw::status::KwPhase::Deploying => "deploying",
            };
            if cancel_requested {
                format!("cancelling {phase} {branch}")
            } else {
                format!("{phase} {branch}")
            }
        }
        Some(KwJobStatus::Succeeded { branch, .. }) => format!("succeeded on {branch}"),
        Some(KwJobStatus::Failed { exit_code, .. }) => match exit_code {
            Some(code) => format!("failed (exit {code})"),
            None => "failed (exit unknown)".to_string(),
        },
        Some(KwJobStatus::Cancelled { .. }) => "cancelled".to_string(),
    }
}

fn project_popup(popup: &AppPopup) -> PopupViewModel {
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

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, path::PathBuf};

    use crate::{
        app::{
            screens::{bookmarked::BookmarkedPatchsetsState, mail_list::MailingListSelectionState},
            state::{
                AppState, ConfigUiState, KwUiState, LoreUiState, NavigationState, UserLoreState,
            },
        },
        config::ConfigState,
        kw::status::{KwJobKind, KwJobStatus, KwPhase, KwStatusSnapshot},
        lore::domain::mailing_list::MailingList,
    };

    use super::*;

    fn app_state_with_kw(status: Option<KwStatusSnapshot>) -> AppState {
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
            config: ConfigState::default().to_snapshot(),
            popup: None,
            kw: KwUiState { status, ops: None },
        }
    }

    #[test]
    fn running_job_projects_a_global_indicator() {
        let vm = project_state(&app_state_with_kw(Some(KwStatusSnapshot {
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
        let vm = project_state(&state);
        let popup = vm.popup.expect("confirm popup should project");
        assert_eq!("Cancel build and quit?", popup.title);
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
    fn kw_ops_command_strips_reserved_extras() {
        let mut state = app_state_with_kw(None);
        state.navigation.current_screen = CurrentScreen::KwOps;
        let mut ops = crate::app::screens::kw_ops::KwOpsState::new(
            "[PATCH] test".to_string(),
            "http://lore.example/123".to_string(),
            "linux".to_string(),
            serde_json::from_value(serde_json::json!({
                "path": "/kernel",
                "branch": "main"
            }))
            .unwrap(),
            crate::kw::readiness::KwReadiness {
                kw_binary: crate::kw::readiness::KwBinaryProbe {
                    available: true,
                    version_line: Some("kw, version 0.10.0".to_string()),
                    check: crate::kw::readiness::KwVersionCheck::Meets,
                },
                tree: crate::kw::readiness::TreeReadiness::Ready {
                    arch: Some("x86_64".to_string()),
                },
                output_dir: None,
                kernel_image: None,
                build_record: None,
                latest_build: None,
                deploy_alone: Err(crate::kw::readiness::DeployAloneRefusal::NoBuildRecord),
                current_branch: Some("feature".to_string()),
                deploy_remote: Err(crate::kw::remote::RemoteRefusal::NoRemotesConfigured),
                boot_once: crate::kw::readiness::BootOnceState::Unknown,
            },
        );
        ops.extra_args = "--verbose --clean --from-sha abc --doc".to_string();
        state.kw.ops = Some(ops);

        let ScreenViewModel::KwOps(vm) = project_state(&state).screen else {
            panic!("expected KwOps projection");
        };
        assert_eq!("kw build --verbose", vm.command);
        assert_eq!("feature", vm.branch);
        assert_eq!("available (b)", vm.start_label);
        assert_eq!("not available yet", vm.deploy_placeholder);
    }

    fn sample_kw_ops(branch: Option<&str>) -> crate::app::screens::kw_ops::KwOpsState {
        crate::app::screens::kw_ops::KwOpsState::new(
            "[PATCH] test".to_string(),
            "http://lore.example/123".to_string(),
            "linux".to_string(),
            serde_json::from_value(serde_json::json!({
                "path": "/kernel",
                "branch": "main"
            }))
            .unwrap(),
            crate::kw::readiness::KwReadiness {
                kw_binary: crate::kw::readiness::KwBinaryProbe {
                    available: true,
                    version_line: Some("kw, version 0.10.0".to_string()),
                    check: crate::kw::readiness::KwVersionCheck::Meets,
                },
                tree: crate::kw::readiness::TreeReadiness::Ready {
                    arch: Some("x86_64".to_string()),
                },
                output_dir: None,
                kernel_image: None,
                build_record: None,
                latest_build: None,
                deploy_alone: Err(crate::kw::readiness::DeployAloneRefusal::NoBuildRecord),
                current_branch: branch.map(str::to_string),
                deploy_remote: Err(crate::kw::remote::RemoteRefusal::NoRemotesConfigured),
                boot_once: crate::kw::readiness::BootOnceState::Unknown,
            },
        )
    }

    #[test]
    fn cleared_readable_branch_does_not_claim_detached_head() {
        let mut state = app_state_with_kw(None);
        state.navigation.current_screen = CurrentScreen::KwOps;
        let mut ops = sample_kw_ops(Some("feature"));
        ops.branch.clear();
        state.kw.ops = Some(ops);

        let ScreenViewModel::KwOps(vm) = project_state(&state).screen else {
            panic!("expected KwOps projection");
        };
        assert_eq!(None, vm.branch_guidance);
        assert_eq!("unavailable (set a branch first)", vm.start_label);
    }

    #[test]
    fn detached_head_projects_branch_guidance() {
        let mut state = app_state_with_kw(None);
        state.navigation.current_screen = CurrentScreen::KwOps;
        state.kw.ops = Some(sample_kw_ops(None));

        let ScreenViewModel::KwOps(vm) = project_state(&state).screen else {
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

        let ScreenViewModel::KwOps(vm) = project_state(&state).screen else {
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

        let ScreenViewModel::KwOps(vm) = project_state(&state).screen else {
            panic!("expected KwOps projection");
        };
        assert_eq!("starting…", vm.job_status);
        assert_eq!("unavailable (a job is already running)", vm.start_label);
    }

    #[test]
    fn idle_succeeded_and_missing_status_have_no_global_indicator() {
        assert_eq!(None, project_state(&app_state_with_kw(None)).kw_running);
        assert_eq!(
            None,
            project_state(&app_state_with_kw(Some(KwStatusSnapshot::idle()))).kw_running
        );
        assert_eq!(
            None,
            project_state(&app_state_with_kw(Some(KwStatusSnapshot {
                job: KwJobStatus::Succeeded {
                    kind: KwJobKind::Build,
                    kernel_tree_id: "mainline".to_string(),
                    branch: "patchset-x".to_string(),
                    log_path: PathBuf::from("/tmp/build.log"),
                },
                restore_branch: Some("master".to_string()),
            })))
            .kw_running
        );
    }
}
