use std::collections::{HashMap, HashSet};

use crate::{
    app::screens::{
        bookmarked::BookmarkedPatchsetsState, details_actions::PatchsetDetailsState,
        edit_config::EditConfigState, latest::LatestPatchsetsState,
        mail_list::MailingListSelectionState, CurrentScreen,
    },
    config::ConfigSnapshot,
    ui::popup::PopUp,
};

/// Navigation-only state: which screen is active.
pub struct NavigationState {
    pub current_screen: CurrentScreen,
}

/// Lore-related UI: mailing list picker, feed, patchset details.
pub struct LoreUiState {
    pub mailing_list_selection: MailingListSelectionState,
    pub latest_patchsets: Option<LatestPatchsetsState>,
    pub details: Option<PatchsetDetailsState>,
}

/// User-owned Lore data (bookmarks and review markers).
pub struct UserLoreState {
    pub bookmarked_patchsets: BookmarkedPatchsetsState,
    pub reviewed_patchsets: HashMap<String, HashSet<usize>>,
}

/// Edit-config screen state (transient form).
pub struct ConfigUiState {
    pub edit_config: Option<EditConfigState>,
}

/// All application state grouped for the future App actor.
pub struct AppState {
    pub navigation: NavigationState,
    pub lore: LoreUiState,
    pub user_state: UserLoreState,
    pub config_state: ConfigUiState,
    pub config: ConfigSnapshot,
    pub popup: Option<Box<dyn PopUp>>,
}
