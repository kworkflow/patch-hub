use mockall::automock;

use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

use crate::{
    infrastructure::shell::ShellCommand,
    lore::{
        application::{
            cache::{BootstrapLoreData, CacheMode},
            dto::PatchsetDetails,
            errors::LoreError,
        },
        domain::{mailing_list::MailingList, patch::Patch},
    },
};

/// Single entry-point for all Lore-domain operations consumed by the UI.
///
/// Implemented synchronously in [`super::service::LoreService`].  The trait
/// boundary makes it trivial to swap in a mock for tests or an async actor
/// implementation in a later phase.
#[automock]
pub trait LoreServiceApi {
    // ── Mailing lists ─────────────────────────────────────────────────────────

    /// Return available mailing lists according to `mode`:
    ///
    /// * `UseCache`  — in-memory hit → disk fallback → error (no network)
    /// * `Refresh`   — unconditionally fetch from the network, persist, update cache
    /// * `Bypass`    — fetch from the network without reading or writing cache
    fn fetch_available_lists(&mut self, mode: CacheMode) -> Result<Vec<MailingList>, LoreError>;

    // ── User state ────────────────────────────────────────────────────────────

    fn load_bookmarked_patchsets(&self) -> Result<Vec<Patch>, LoreError>;
    fn save_bookmarked_patchsets(&self, patchsets: &[Patch]) -> Result<(), LoreError>;

    fn load_reviewed_patchsets(&self) -> Result<HashMap<String, HashSet<usize>>, LoreError>;
    fn save_reviewed_patchsets(
        &self,
        reviewed: &HashMap<String, HashSet<usize>>,
    ) -> Result<(), LoreError>;

    // ── Feed pagination ───────────────────────────────────────────────────────

    /// Return the patches for `page_number` (1-based) of `target_list`.
    ///
    /// Internally fetches more feed pages from the network as needed.
    /// Returns [`LoreError::EndOfFeed`] when the list is exhausted.
    fn fetch_next_patch_page(
        &mut self,
        target_list: &str,
        page_size: usize,
        page_number: usize,
    ) -> Result<Vec<Patch>, LoreError>;

    /// Discard the cached feed state for `target_list` so the next call to
    /// [`fetch_next_patch_page`] starts from the beginning.
    fn reset_feed_cursor(&mut self, target_list: &str);

    // ── Patchset details ──────────────────────────────────────────────────────

    /// Download and parse `representative_patch`, returning the full patchset
    /// data needed to populate the details screen.
    fn fetch_patchset_details(
        &self,
        representative_patch: &Patch,
    ) -> Result<PatchsetDetails, LoreError>;

    // ── Reply commands ────────────────────────────────────────────────────────

    /// Build the `git send-email` commands required to reply to the selected
    /// patches with a `Reviewed-by` trailer.
    ///
    /// Files are written under `tmp_dir`; the caller is responsible for
    /// spawning the returned commands interactively.
    fn prepare_reply_commands(
        &self,
        tmp_dir: &Path,
        target_list: &str,
        patches: &[String],
        patches_to_reply: &[bool],
        git_signature: &str,
        git_send_email_options: &str,
    ) -> Result<Vec<ShellCommand>, LoreError>;

    // ── Git helpers ───────────────────────────────────────────────────────────

    /// Return `(user.name, user.email)` from `git config` for the given repo
    /// path.  Pass an empty string to use the global git config.
    fn get_git_signature(&self, git_repo_path: &str) -> (String, String);

    // ── Bootstrap ─────────────────────────────────────────────────────────────

    /// Warm the bootstrap cache and return all data needed to initialise `App`.
    ///
    /// Internally calls `fetch_available_lists(UseCache)`,
    /// `load_bookmarked_patchsets`, and `load_reviewed_patchsets`.  Each
    /// failure is logged and replaced with an empty default so that the caller
    /// can treat this method as infallible in practice.
    fn warm_bootstrap_cache(&mut self) -> Result<BootstrapLoreData, LoreError>;
}
