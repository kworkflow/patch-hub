use std::{
    collections::{HashMap, HashSet},
    time::{Duration, SystemTime},
};

use crate::lore::{
    application::dto::PatchTagSummary,
    domain::{mailing_list::MailingList, patch::Patch, patchset::PatchFeedIndex},
};

/// Caller intent for a cache-backed `LoreService` operation.
///
/// * `UseCache`  — return in-memory data if present and not stale; fall back to
///   disk; return an error/empty if neither is available. Never hits the network
///   unless the data is genuinely missing.
/// * `Refresh`   — discard any cached data and unconditionally fetch from the
///   network, then persist the result.
/// * `Bypass`    — fetch from the network and return the result without reading
///   or writing the in-memory cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheMode {
    UseCache,
    Refresh,
    #[expect(dead_code)]
    Bypass,
}

/// Per-data-type TTL configuration injected into `LoreService`.
///
/// Defaults are chosen conservatively:
/// * mailing lists change rarely → 24 h
/// * feed pages are refreshed by the user explicitly → 1 h
/// * patchsets are immutable historical artefacts → never expire
pub struct CacheTtl {
    pub mailing_lists: Duration,
    pub feed: Duration,
    pub patchset: Duration,
}

impl Default for CacheTtl {
    fn default() -> Self {
        CacheTtl {
            mailing_lists: Duration::from_secs(86_400),
            feed: Duration::from_secs(3_600),
            patchset: Duration::MAX,
        }
    }
}

pub struct MailingListsCacheEntry {
    pub lists: Vec<MailingList>,
    pub fetched_at: SystemTime,
}

/// A feed cache entry wraps the domain-level `PatchFeedIndex` with TTL metadata.
///
/// `PatchFeedIndex` (in `domain/patchset.rs`) remains a pure domain type;
/// all cache concerns live here.
pub struct FeedCacheEntry {
    pub index: PatchFeedIndex,
    pub fetched_at: SystemTime,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct PatchsetCacheKey {
    pub message_id: String,
    pub version: usize,
}

impl From<&Patch> for PatchsetCacheKey {
    fn from(patch: &Patch) -> Self {
        Self {
            message_id: patch.message_id().href.clone(),
            version: patch.version(),
        }
    }
}

pub struct PatchsetCacheEntry {
    pub patchset_path: String,
    pub raw_patches: Vec<String>,
    pub tag_summary: Vec<PatchTagSummary>,
    pub fetched_at: SystemTime,
}

pub struct LoreCache {
    pub lists: Option<MailingListsCacheEntry>,
    pub feeds: HashMap<String, FeedCacheEntry>,
    pub patchsets: HashMap<PatchsetCacheKey, PatchsetCacheEntry>,
}

/// Data returned by `LoreService::warm_bootstrap_cache`, used to initialise
/// `App` without it knowing about persistence paths or network policy.
#[derive(Default)]
pub struct BootstrapLoreData {
    pub mailing_lists: Vec<MailingList>,
    pub bookmarks: Vec<Patch>,
    pub reviewed: HashMap<String, HashSet<usize>>,
}
