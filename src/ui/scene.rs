//! Scene types produced by [`super::core::UiCore`] and consumed by
//! [`super::painter`].
//!
//! A `UiScene` is a fully projected, paint-ready snapshot of application
//! state. Nothing inside this module reads from `App`, `AppState`, or any
//! actor handle — it is pure presentation data.

use ratatui::text::{Span, Text};

// Shared presentation-row types live in the app view-model layer. Re-export
// them here so callers within `ui/` only import from `scene`.
pub use crate::app::view_model::{
    ConfigEntryRow, MailingListEntry, PatchSummaryRow, TagTrailerCounts,
};

// ---------------------------------------------------------------------------
// Mailing-list selection
// ---------------------------------------------------------------------------

/// Scene for the mailing-list selection screen.
#[derive(Clone, Debug)]
pub struct MailingListScene {
    pub entries: Vec<MailingListEntry>,
    pub highlighted_index: usize,
}

// ---------------------------------------------------------------------------
// Bookmarked patchsets
// ---------------------------------------------------------------------------

/// Scene for the bookmarked-patchsets screen.
#[derive(Clone, Debug)]
pub struct BookmarkedScene {
    pub rows: Vec<PatchSummaryRow>,
    pub selected_index: usize,
}

// ---------------------------------------------------------------------------
// Latest patchsets
// ---------------------------------------------------------------------------

/// Scene for the latest-patchsets screen.
#[derive(Clone, Debug)]
pub struct LatestScene {
    pub rows: Vec<PatchSummaryRow>,
    pub selected_index: usize,
}

// ---------------------------------------------------------------------------
// Patchset details
// ---------------------------------------------------------------------------

/// Scene for the patchset-details-and-actions screen.
#[derive(Clone, Debug)]
pub struct PatchsetDetailsScene {
    pub patch_title: String,
    pub author_name: String,
    pub version: usize,
    pub patch_count: usize,
    pub last_updated: String,
    /// Trailer counts for the currently previewed patch.
    pub tag_trailer_counts: TagTrailerCounts,
    /// Pre-computed "(0, 2, ...)" string shown when at least one patch is
    /// staged for reply. `None` when nothing is staged.
    pub staged_to_reply: Option<String>,
    /// ANSI-rendered diff/cover text for each patch entry.
    pub preview_entries: Vec<Text<'static>>,
    pub preview_index: usize,
    pub preview_scroll_offset: usize,
    pub preview_pan: usize,
    pub preview_fullscreen: bool,
    /// Title shown above the preview pane, including the `[REVIEWED-BY]` or
    /// `[REVIEWED-BY]*` suffix when applicable. Pre-computed by `App::present`.
    pub preview_title: String,
    pub is_bookmarked: bool,
    pub is_apply_staged: bool,
    /// Whether the patch at `preview_index` is staged for a Reviewed-by reply.
    pub is_current_patch_reply_staged: bool,
}

// ---------------------------------------------------------------------------
// Edit config
// ---------------------------------------------------------------------------

/// Scene for the edit-configuration screen.
#[derive(Clone, Debug)]
pub struct EditConfigScene {
    pub entries: Vec<ConfigEntryRow>,
}

// ---------------------------------------------------------------------------
// Discriminated body
// ---------------------------------------------------------------------------

/// Which screen's scene the body carries.
#[derive(Clone, Debug)]
pub enum UiBody {
    MailingListSelection(MailingListScene),
    Bookmarked(BookmarkedScene),
    Latest(LatestScene),
    PatchsetDetails(PatchsetDetailsScene),
    EditConfig(EditConfigScene),
}

// ---------------------------------------------------------------------------
// Navigation bar
// ---------------------------------------------------------------------------

/// Pre-computed navigation-bar content.
///
/// `mode_spans` is a list of styled text spans that together form the left
/// section (mode/context text). `keys_hint` is the right section.
/// Both are built with owned strings so the scene is `'static`-compatible.
#[derive(Clone, Debug)]
pub struct NavigationBarScene {
    pub mode_spans: Vec<Span<'static>>,
    pub keys_hint: Span<'static>,
}

// ---------------------------------------------------------------------------
// Popup
// ---------------------------------------------------------------------------

/// Structured body for each popup variant.
#[derive(Clone, Debug)]
pub enum PopupBody {
    /// Plain informational text (apply result, bookmark confirmation, …).
    Text(String),
    /// Help popup with optional description and pre-formatted keybind table.
    Keybinds {
        description: Option<String>,
        formatted_keybinds: String,
    },
    /// Code-review-trailer popup with one section per trailer type.
    ReviewTrailers {
        reviewed_by: String,
        tested_by: String,
        acked_by: String,
    },
}

/// Fully projected popup ready to be painted.
#[derive(Clone, Debug)]
pub struct PopupScene {
    pub title: String,
    pub body: PopupBody,
    pub scroll_offset: (u16, u16),
    /// `(width_percent, height_percent)` of the terminal area.
    pub dimensions: (u16, u16),
}

// ---------------------------------------------------------------------------
// Top-level scene
// ---------------------------------------------------------------------------

/// The complete, paint-ready visual snapshot for one TUI frame.
#[derive(Clone, Debug)]
pub struct UiScene {
    pub body: UiBody,
    pub navigation: NavigationBarScene,
    pub popup: Option<PopupScene>,
}
