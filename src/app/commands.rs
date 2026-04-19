//! High-level application intents. Intended to become the App actor message
//! protocol in later phases; most handlers still call `App` methods directly.

use crate::{app::screens::CurrentScreen, lore::domain::patch::Patch};

/// Sub-actions performed during [`crate::app::App::consolidate_patchset_actions`].
#[allow(dead_code)] // reserved for future command dispatch
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatchsetCommand {
    SyncBookmark,
    ReplyWithReviewedBy,
    Apply,
}

/// Top-level commands the application may process (placeholder for future wiring).
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum AppCommand {
    NavigateTo(CurrentScreen),
    SelectMailingList(String),
    FetchNextPage,
    OpenPatchset(Patch),
}
