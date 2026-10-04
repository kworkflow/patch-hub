use std::{
    collections::HashMap,
    time::{Duration, SystemTime},
};

use crate::lore::{
    application::dto::PatchTagSummary,
    domain::{mailing_list::MailingList, patch::Patch, patchset::PatchFeedIndex},
};

use crate::lore::application::models::cache::{
    FeedCacheEntry, LoreCache, MailingListsCacheEntry, PatchsetCacheEntry, PatchsetCacheKey,
};

// ── Mailing lists cache ───────────────────────────────────────────────────────

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

impl PatchsetCacheKey {
    pub fn from_patch(patch: &Patch) -> Self {
        PatchsetCacheKey {
            message_id: patch.message_id().href.clone(),
            version: patch.version(),
        }
    }
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
