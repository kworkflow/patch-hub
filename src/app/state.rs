use std::collections::{HashMap, HashSet};

use crate::{
    app::{
        popup::AppPopup,
        screens::{
            bookmarked::BookmarkedPatchsetsState, details_actions::PatchsetDetailsState,
            edit_config::EditConfigState, latest::LatestPatchsetsState,
            mail_list::MailingListSelectionState, CurrentScreen,
        },
    },
    config::ConfigSnapshot,
    kw::status::KwStatusSnapshot,
};

/// Navigation-only state: which screen is active.
#[derive(Clone)]
pub struct NavigationState {
    pub current_screen: CurrentScreen,
}

/// Lore-related UI: mailing list picker, feed, patchset details.
#[derive(Clone)]
pub struct LoreUiState {
    pub mailing_list_selection: MailingListSelectionState,
    pub latest_patchsets: Option<LatestPatchsetsState>,
    pub details: Option<PatchsetDetailsState>,
}

/// User-owned Lore data (bookmarks and review markers).
#[derive(Clone)]
pub struct UserLoreState {
    pub bookmarked_patchsets: BookmarkedPatchsetsState,
    pub reviewed_patchsets: HashMap<String, HashSet<usize>>,
}

/// Edit-config screen state (transient form).
#[derive(Clone)]
pub struct ConfigUiState {
    pub edit_config: Option<EditConfigState>,
}

/// Projection of kw job status owned by AppActor, not the job itself.
///
/// `status` is `None` when no KwActor is attached (non-unix, or the
/// watch subscription failed) rather than a fabricated idle job.
#[derive(Clone, Debug, Default)]
pub struct KwUiState {
    pub status: Option<KwStatusSnapshot>,
}

/// All application state grouped as the App actor's state.
#[derive(Clone)]
pub struct AppState {
    pub navigation: NavigationState,
    pub lore: LoreUiState,
    pub user_state: UserLoreState,
    pub config_state: ConfigUiState,
    pub config: ConfigSnapshot,
    pub popup: Option<AppPopup>,
    pub kw: KwUiState,
}
