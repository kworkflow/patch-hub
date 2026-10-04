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
/// `UseCache` returns fresh memory, else disk, else empty, and hits the
/// network only when data is missing. `Refresh` discards the cache, fetches,
/// and persists. `Bypass` fetches and does not touch the in-memory cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheMode {
    UseCache,
    Refresh,
    #[expect(dead_code)]
    Bypass,
}

/// Per-data-type TTL injected into `LoreService`. Defaults: mailing lists
/// 24 h (they change rarely), feed pages 1 h (the user refreshes them),
/// patchsets never expire (they are immutable).
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

/// Data returned by `LoreService::warm_bootstrap_cache`, which initialises
/// `App` without it knowing about persistence paths or network policy.
#[derive(Default)]
pub struct BootstrapLoreData {
    pub mailing_lists: Vec<MailingList>,
    pub bookmarks: Vec<Patch>,
    pub reviewed: HashMap<String, HashSet<usize>>,
}
