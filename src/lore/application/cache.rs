use std::{
    collections::{HashMap, HashSet},
    time::{Duration, SystemTime},
};

use crate::lore::{
    application::dto::PatchTagSummary,
    domain::{mailing_list::MailingList, patch::Patch, patchset::PatchFeedIndex},
};

// ── Cache mode ────────────────────────────────────────────────────────────────

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

// ── TTL configuration ─────────────────────────────────────────────────────────

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

// ── Generic cache policy trait ────────────────────────────────────────────────

/// Uniform cache interface (`get`, `put`, `invalidate`, `clear`) for Lore cache stores.
#[allow(dead_code)]
pub trait CachePolicy<K, V> {
    fn get(&self, key: &K) -> Option<&V>;
    fn put(&mut self, key: K, value: V);
    fn invalidate(&mut self, key: &K);
    fn clear(&mut self);
}

// ── Mailing lists cache ───────────────────────────────────────────────────────

pub struct MailingListsCacheEntry {
    pub lists: Vec<MailingList>,
    pub fetched_at: SystemTime,
}

impl MailingListsCacheEntry {
    pub fn new(lists: Vec<MailingList>) -> Self {
        MailingListsCacheEntry {
            lists,
            fetched_at: SystemTime::now(),
        }
    }

    pub fn is_stale(&self, ttl: Duration) -> bool {
        self.fetched_at.elapsed().map(|e| e > ttl).unwrap_or(true)
    }
}

// ── Feed cache ────────────────────────────────────────────────────────────────

/// A feed cache entry wraps the domain-level `PatchFeedIndex` with TTL metadata.
///
/// `PatchFeedIndex` (in `domain/patchset.rs`) remains a pure domain type;
/// all cache concerns live here.
pub struct FeedCacheEntry {
    pub index: PatchFeedIndex,
    pub fetched_at: SystemTime,
}

impl FeedCacheEntry {
    pub fn new(index: PatchFeedIndex) -> Self {
        FeedCacheEntry {
            index,
            fetched_at: SystemTime::now(),
        }
    }

    pub fn is_stale(&self, ttl: Duration) -> bool {
        self.fetched_at.elapsed().map(|e| e > ttl).unwrap_or(true)
    }
}

// ── Patchset cache ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct PatchsetCacheKey {
    pub message_id: String,
    pub version: usize,
}

impl PatchsetCacheKey {
    pub fn from_patch(patch: &Patch) -> Self {
        PatchsetCacheKey {
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

impl PatchsetCacheEntry {
    pub fn new(
        patchset_path: String,
        raw_patches: Vec<String>,
        tag_summary: Vec<PatchTagSummary>,
    ) -> Self {
        PatchsetCacheEntry {
            patchset_path,
            raw_patches,
            tag_summary,
            fetched_at: SystemTime::now(),
        }
    }

    pub fn is_stale(&self, ttl: Duration) -> bool {
        self.fetched_at.elapsed().map(|e| e > ttl).unwrap_or(true)
    }
}

// ── Aggregate cache ───────────────────────────────────────────────────────────

pub struct LoreCache {
    pub lists: Option<MailingListsCacheEntry>,
    pub feeds: HashMap<String, FeedCacheEntry>,
    pub patchsets: HashMap<PatchsetCacheKey, PatchsetCacheEntry>,
}

impl LoreCache {
    pub fn new() -> Self {
        LoreCache {
            lists: None,
            feeds: HashMap::new(),
            patchsets: HashMap::new(),
        }
    }
}

impl Default for LoreCache {
    fn default() -> Self {
        Self::new()
    }
}

// ── Bootstrap result ──────────────────────────────────────────────────────────

/// Data returned by `LoreService::warm_bootstrap_cache`, used to initialise
/// `App` without it knowing about persistence paths or network policy.
#[derive(Default)]
pub struct BootstrapLoreData {
    pub mailing_lists: Vec<MailingList>,
    pub bookmarks: Vec<Patch>,
    pub reviewed: HashMap<String, HashSet<usize>>,
}
