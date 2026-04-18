use std::collections::HashMap;

use crate::lore::domain::patch::{Patch, PatchFeed, PatchRegex};

const LORE_PAGE_SIZE: usize = 200;

/// Tracks feed pagination state for a single mailing list target.
///
/// Replaces the state that was previously mixed into [`LoreSession`].
pub struct PatchFeedIndex {
    // TODO: used by the actor model (Phase 6) to identify which list this index belongs to.
    #[allow(dead_code)]
    target_list: String,
    next_offset: usize,
    representative_patch_ids: Vec<String>,
    patches_by_id: HashMap<String, Patch>,
    patch_regex: PatchRegex,
}

impl PatchFeedIndex {
    pub fn new(target_list: String) -> Self {
        PatchFeedIndex {
            target_list,
            next_offset: 0,
            representative_patch_ids: Vec::new(),
            patches_by_id: HashMap::new(),
            patch_regex: PatchRegex::new(),
        }
    }

    // TODO: used by the actor model (Phase 6) for random-access patch lookup by message ID.
    #[allow(dead_code)]
    pub fn target_list(&self) -> &str {
        &self.target_list
    }

    pub fn next_offset(&self) -> usize {
        self.next_offset
    }

    pub fn representative_patch_ids(&self) -> &[String] {
        &self.representative_patch_ids
    }

    // TODO: used by the actor model (Phase 6) for random-access patch lookup by message ID.
    #[allow(dead_code)]
    pub fn get_patch(&self, id: &str) -> Option<&Patch> {
        self.patches_by_id.get(id)
    }

    /// Process a page of [`PatchFeed`] entries, deduplicating and tracking
    /// which patches are "representative" (series cover or standalone patch).
    pub fn process_feed_page(&mut self, feed: PatchFeed) {
        let new_ids = self.ingest_patches(feed);
        self.update_representative_ids(new_ids);
    }

    /// Advance the feed offset by one page (ready for the next request).
    pub fn advance_offset(&mut self) {
        self.next_offset += LORE_PAGE_SIZE;
    }

    /// Return the patches for a given `page_number` (1-based), each page
    /// holding at most `page_size` patches.  Returns `None` when the
    /// requested page starts beyond what has been processed so far.
    pub fn get_page(&self, page_size: usize, page_number: usize) -> Option<Vec<&Patch>> {
        if self.representative_patch_ids.is_empty() {
            return None;
        }

        let max_index = self.representative_patch_ids.len() - 1;
        let lower_end = page_size * (page_number - 1);
        let mut upper_end = page_size * page_number;

        if max_index < lower_end {
            return None;
        }

        if max_index < upper_end - 1 {
            upper_end = max_index + 1;
        }

        let page: Vec<&Patch> = (lower_end..upper_end)
            .filter_map(|i| self.patches_by_id.get(&self.representative_patch_ids[i]))
            .collect();

        Some(page)
    }

    fn ingest_patches(&mut self, feed: PatchFeed) -> Vec<String> {
        let mut new_ids = Vec::new();
        for mut patch in feed.patches().clone() {
            patch.update_patch_metadata(&self.patch_regex);
            if !self.patches_by_id.contains_key(&patch.message_id().href) {
                new_ids.push(patch.message_id().href.clone());
                self.patches_by_id
                    .insert(patch.message_id().href.clone(), patch);
            }
        }
        new_ids
    }

    fn update_representative_ids(&mut self, new_ids: Vec<String>) {
        for id in new_ids {
            let patch = self.patches_by_id.get(&id).unwrap();
            let number_in_series = patch.number_in_series();

            if number_in_series > 1 {
                continue;
            }

            if number_in_series == 1 {
                if let Some(in_reply_to) = patch.in_reply_to() {
                    if let Some(referenced) = self.patches_by_id.get(&in_reply_to.href) {
                        if referenced.number_in_series() == 0
                            && patch.version() == referenced.version()
                        {
                            continue;
                        }
                    }
                }
            }

            self.representative_patch_ids.push(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::lore::infrastructure::parsers::parse_patch_feed;

    fn feed_from_file(path: &str) -> PatchFeed {
        let xml = fs::read_to_string(path).unwrap();
        parse_patch_feed(&xml).unwrap()
    }

    #[test]
    fn new_starts_with_empty_state() {
        let idx = PatchFeedIndex::new("linux-kernel".to_string());
        assert_eq!("linux-kernel", idx.target_list());
        assert_eq!(0, idx.next_offset());
        assert!(idx.representative_patch_ids().is_empty());
    }

    #[test]
    fn process_feed_page_extracts_representative_patch() {
        let mut idx = PatchFeedIndex::new("some-list".to_string());
        let feed = feed_from_file(
            "test_samples/lore_session/process_representative_patch/patch_feed_sample_1.xml",
        );

        idx.process_feed_page(feed);

        assert_eq!(1, idx.representative_patch_ids().len());
        let id = &idx.representative_patch_ids()[0];
        assert!(id.contains("1234.567-1-john@johnson.com"));
        let patch = idx.get_patch(id).unwrap();
        assert_eq!("some/subsystem: Do this and that", patch.title());
        assert_eq!(1, patch.version());
    }

    #[test]
    fn process_feed_page_extracts_multiple_representative_patches() {
        let mut idx = PatchFeedIndex::new("some-list".to_string());
        let feed = feed_from_file(
            "test_samples/lore_session/process_representative_patch/patch_feed_sample_2.xml",
        );

        idx.process_feed_page(feed);

        assert_eq!(3, idx.representative_patch_ids().len());
    }

    #[test]
    fn process_feed_page_deduplicates() {
        let mut idx = PatchFeedIndex::new("some-list".to_string());
        let feed = feed_from_file(
            "test_samples/lore_session/process_representative_patch/patch_feed_sample_1.xml",
        );
        idx.process_feed_page(feed.clone());
        idx.process_feed_page(feed);

        assert_eq!(1, idx.representative_patch_ids().len());
    }

    #[test]
    fn advance_offset_increments_by_page_size() {
        let mut idx = PatchFeedIndex::new("list".to_string());
        assert_eq!(0, idx.next_offset());
        idx.advance_offset();
        assert_eq!(200, idx.next_offset());
        idx.advance_offset();
        assert_eq!(400, idx.next_offset());
    }

    #[test]
    fn get_page_returns_none_when_empty() {
        let idx = PatchFeedIndex::new("list".to_string());
        assert!(idx.get_page(10, 1).is_none());
    }

    #[test]
    fn get_page_returns_correct_patches() {
        let mut idx = PatchFeedIndex::new("some-list".to_string());
        let feed = feed_from_file(
            "test_samples/lore_session/process_representative_patch/patch_feed_sample_2.xml",
        );
        idx.process_feed_page(feed);

        let page = idx.get_page(2, 1).unwrap();
        assert_eq!(2, page.len());

        let page2 = idx.get_page(2, 2).unwrap();
        assert_eq!(1, page2.len());

        assert!(idx.get_page(2, 3).is_none());
    }
}
