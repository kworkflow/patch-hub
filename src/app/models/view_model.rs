use ratatui::text::Text;

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
    pub editing_tree_selector: bool,
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
    pub remote: String,
    pub boot_once: String,
    pub deploy_command: String,
    pub deploy_label: String,
    pub build_deploy_label: String,
    pub branch_guidance: Option<String>,
    /// Deploy warnings of the last job, joined with ` | `.
    pub warnings: Option<String>,
    /// First error line of the last failed job's log.
    pub first_error: Option<String>,
    /// Full log of the current or last job; the pane only shows its tail.
    pub log_path: Option<String>,
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
    // Boxed to keep the enum small (clippy::large_enum_variant): KwOps
    // carries one owned string per dashboard row.
    KwOps(Box<KwOpsViewModel>),
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
