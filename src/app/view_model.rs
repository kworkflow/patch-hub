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
    screens::{details_actions::PatchsetAction, CurrentScreen},
    state::AppState,
};

// ---------------------------------------------------------------------------
// Shared row / helper types
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Per-screen view models
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Popup view model
// ---------------------------------------------------------------------------

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
}

#[derive(Clone, Debug)]
pub struct PopupViewModel {
    pub title: String,
    pub body: PopupViewBody,
    pub scroll_offset: (u16, u16),
    /// `(width_percent, height_percent)` of the terminal area.
    pub dimensions: (u16, u16),
}

// ---------------------------------------------------------------------------
// Discriminated screen enum
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub enum ScreenViewModel {
    MailingListSelection(MailingListSelectionViewModel),
    Bookmarked(BookmarkedViewModel),
    Latest(LatestPatchsetsViewModel),
    PatchsetDetails(PatchsetDetailsViewModel),
    EditConfig(EditConfigViewModel),
}

// ---------------------------------------------------------------------------
// Top-level view model
// ---------------------------------------------------------------------------

/// Owned, typed projection of [`AppState`] for one TUI frame.
///
/// Built by [`project_state`]; consumed by `UiCore::build_scene`.
#[derive(Clone, Debug)]
pub struct AppViewModel {
    pub screen: ScreenViewModel,
    pub popup: Option<PopupViewModel>,
}

// ---------------------------------------------------------------------------
// Projection – public entry point
// ---------------------------------------------------------------------------

/// Projects `state` into an owned [`AppViewModel`].
///
/// Called via [`super::App::present`].
pub fn project_state(state: &AppState) -> AppViewModel {
    let screen = project_screen(state);
    let popup = state.popup.as_ref().map(project_popup);
    AppViewModel { screen, popup }
}

// ---------------------------------------------------------------------------
// Private per-screen projectors
// ---------------------------------------------------------------------------

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
    }
}
