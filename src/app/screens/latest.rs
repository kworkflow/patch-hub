use color_eyre::eyre::bail;

use crate::lore::{
    application::{cache::CacheMode, errors::LoreError, handle::LoreApiHandle},
    domain::patch::Patch,
};

pub struct LatestPatchsetsState {
    target_list: String,
    page_number: usize,
    /// Index of the selected patchset within the current page (0-based).
    patchset_index: usize,
    page_size: usize,
    /// The currently loaded page of patches.
    current_page: Vec<Patch>,
}

impl LatestPatchsetsState {
    pub fn new(target_list: String, page_size: usize) -> LatestPatchsetsState {
        LatestPatchsetsState {
            target_list,
            page_number: 1,
            patchset_index: 0,
            page_size,
            current_page: Vec::new(),
        }
    }

    pub fn target_list(&self) -> &str {
        &self.target_list
    }

    pub fn page_number(&self) -> usize {
        self.page_number
    }

    pub fn patchset_index(&self) -> usize {
        self.patchset_index
    }

    #[allow(dead_code)]
    pub fn page_size(&self) -> usize {
        self.page_size
    }

    pub async fn fetch_current_page(
        &mut self,
        lore_api: &LoreApiHandle,
        mode: CacheMode,
    ) -> color_eyre::Result<()> {
        match lore_api
            .fetch_feed_page(
                self.target_list.clone(),
                self.page_size,
                self.page_number,
                mode,
            )
            .await
        {
            Ok(patches) => {
                self.current_page = patches;
            }
            Err(LoreError::EndOfFeed) => {}
            Err(e) => bail!("{e:#?}"),
        }
        Ok(())
    }

    pub fn select_below_patchset(&mut self) {
        if self.patchset_index + 1 < self.current_page.len() {
            self.patchset_index += 1;
        }
    }

    pub fn select_above_patchset(&mut self) {
        self.patchset_index = self.patchset_index.saturating_sub(1);
    }

    pub fn increment_page(&mut self) {
        if self.current_page.len() < self.page_size {
            return;
        }
        self.page_number += 1;
        self.patchset_index = 0;
    }

    pub fn decrement_page(&mut self) {
        if self.page_number == 1 {
            return;
        }
        self.page_number -= 1;
        self.patchset_index = 0;
    }

    pub fn get_selected_patchset(&self) -> Patch {
        self.current_page.get(self.patchset_index).unwrap().clone()
    }

    pub fn get_current_patch_feed_page(&self) -> Option<Vec<&Patch>> {
        if self.current_page.is_empty() {
            None
        } else {
            Some(self.current_page.iter().collect())
        }
    }

    pub fn processed_patchsets_count(&self) -> usize {
        self.current_page.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use crate::{
        infrastructure::{file_system::MockFileSystemTrait, shell::MockShellTrait},
        lore::{
            application::{actor::LoreApiActor, cache::CacheTtl, service::LoreService},
            infrastructure::{
                http_lore_client::{
                    LoreHttpError, MockFeedGateway, MockListsGateway, MockPatchHtmlGateway,
                },
                patchset_fetcher::MockPatchsetFetcher,
                patchset_parser::MockPatchsetParser,
                persistence::{MockMailingListsCacheStore, MockUserLoreStateStore},
            },
        },
    };

    fn make_patch(msg_id: &str) -> Patch {
        serde_json::from_value(serde_json::json!({
            "title": "test patch",
            "author": { "name": "Test", "email": "test@test.com" },
            "link": { "@href": msg_id },
            "updated": "2023-01-01"
        }))
        .unwrap()
    }

    fn make_handle(feed_gateway: MockFeedGateway) -> LoreApiHandle {
        let service = LoreService::new(
            Arc::new(MockListsGateway::new()),
            Arc::new(feed_gateway),
            Arc::new(MockPatchHtmlGateway::new()),
            Arc::new(MockMailingListsCacheStore::new()),
            Arc::new(MockUserLoreStateStore::new()),
            Arc::new(MockPatchsetFetcher::new()),
            Arc::new(MockPatchsetParser::new()),
            Arc::new(MockFileSystemTrait::new()),
            Arc::new(MockShellTrait::new()),
            CacheTtl::default(),
        );
        LoreApiActor::spawn(service)
    }

    fn patch_feed_response() -> String {
        r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <entry>
    <title>test patch</title>
    <author><name>Test</name><email>test@test.com</email></author>
    <link href="id-1"/>
    <updated>2023-01-01</updated>
  </entry>
  <entry>
    <title>test patch 2</title>
    <author><name>Test</name><email>test@test.com</email></author>
    <link href="id-2"/>
    <updated>2023-01-01</updated>
  </entry>
</feed>"#
            .to_string()
    }

    #[tokio::test]
    async fn test_fetch_current_page_success() {
        let mut feed_gateway = MockFeedGateway::new();
        feed_gateway
            .expect_fetch_patch_feed_page()
            .times(1)
            .returning(|_, _| Ok(patch_feed_response()));

        let handle = make_handle(feed_gateway);
        let mut lp = LatestPatchsetsState::new("some-list".to_string(), 2);
        let result = lp.fetch_current_page(&handle, CacheMode::UseCache).await;

        assert!(result.is_ok());
        assert_eq!(lp.processed_patchsets_count(), 2);
    }

    #[tokio::test]
    async fn test_fetch_current_page_end_of_feed() {
        let mut feed_gateway = MockFeedGateway::new();
        feed_gateway
            .expect_fetch_patch_feed_page()
            .times(1)
            .returning(|_, _| Err(LoreHttpError::EndOfFeed));

        let handle = make_handle(feed_gateway);
        let mut lp = LatestPatchsetsState::new("some-list".to_string(), 5);
        let result = lp.fetch_current_page(&handle, CacheMode::UseCache).await;

        assert!(result.is_ok());
        assert_eq!(lp.processed_patchsets_count(), 0);
    }

    #[tokio::test]
    async fn test_fetch_current_page_error() {
        use crate::infrastructure::net::NetError;

        let mut feed_gateway = MockFeedGateway::new();
        feed_gateway
            .expect_fetch_patch_feed_page()
            .times(1)
            .returning(|_, _| {
                Err(LoreHttpError::Net(NetError::HttpStatus {
                    code: 500,
                    message: "Internal Server Error".to_string(),
                }))
            });

        let handle = make_handle(feed_gateway);
        let mut lp = LatestPatchsetsState::new("some-list".to_string(), 5);
        let result = lp.fetch_current_page(&handle, CacheMode::UseCache).await;

        assert!(result.is_err());
    }

    #[test]
    fn test_select_below_patchset() {
        let mut lp = LatestPatchsetsState::new("some-list".to_string(), 3);
        lp.current_page = vec![make_patch("a"), make_patch("b"), make_patch("c")];
        lp.patchset_index = 0;

        lp.select_below_patchset();
        assert_eq!(lp.patchset_index(), 1);

        lp.select_below_patchset();
        assert_eq!(lp.patchset_index(), 2);

        // Already at the bottom, should not move
        lp.select_below_patchset();
        assert_eq!(lp.patchset_index(), 2);
    }

    #[test]
    fn test_select_below_patchset_empty_page() {
        let mut lp = LatestPatchsetsState::new("some-list".to_string(), 3);
        lp.patchset_index = 0;
        lp.select_below_patchset();
        assert_eq!(lp.patchset_index(), 0);
    }

    #[test]
    fn test_select_above_patchset() {
        let mut lp = LatestPatchsetsState::new("some-list".to_string(), 3);
        lp.current_page = vec![make_patch("a"), make_patch("b"), make_patch("c")];
        lp.patchset_index = 2;

        lp.select_above_patchset();
        assert_eq!(lp.patchset_index(), 1);

        lp.select_above_patchset();
        assert_eq!(lp.patchset_index(), 0);

        // Already at the top, should not go negative
        lp.select_above_patchset();
        assert_eq!(lp.patchset_index(), 0);
    }

    #[test]
    fn test_increment_page_full_page() {
        let mut lp = LatestPatchsetsState::new("some-list".to_string(), 3);
        lp.current_page = vec![make_patch("a"), make_patch("b"), make_patch("c")];
        lp.patchset_index = 2;

        lp.increment_page();
        assert_eq!(lp.page_number(), 2);
        assert_eq!(lp.patchset_index(), 0);
    }

    #[test]
    fn test_increment_page_partial_page() {
        let mut lp = LatestPatchsetsState::new("some-list".to_string(), 3);
        lp.current_page = vec![make_patch("a"), make_patch("b")]; // 2 < page_size=3

        lp.increment_page();
        assert_eq!(lp.page_number(), 1); // no increment
    }

    #[test]
    fn test_increment_page_sequential() {
        let mut lp = LatestPatchsetsState::new("some-list".to_string(), 1);
        lp.current_page = vec![make_patch("a")];

        lp.increment_page();
        assert_eq!(lp.page_number(), 2);
        assert_eq!(lp.patchset_index(), 0);

        lp.current_page = vec![make_patch("b")];
        lp.increment_page();
        assert_eq!(lp.page_number(), 3);
    }

    #[test]
    fn test_decrement_page() {
        let mut lp = LatestPatchsetsState::new("some-list".to_string(), 3);

        // Already on page 1, no change
        lp.decrement_page();
        assert_eq!(lp.page_number(), 1);
        assert_eq!(lp.patchset_index(), 0);

        // On page 3, decrement resets patchset_index to 0
        lp.page_number = 3;
        lp.patchset_index = 2;
        lp.decrement_page();
        assert_eq!(lp.page_number(), 2);
        assert_eq!(lp.patchset_index(), 0);

        lp.decrement_page();
        assert_eq!(lp.page_number(), 1);
        assert_eq!(lp.patchset_index(), 0);
    }

    #[test]
    #[should_panic]
    fn test_get_selected_patchset_before_fetching_page() {
        let lp = LatestPatchsetsState::new("some-list".to_string(), 3);
        let _patch = lp.get_selected_patchset();
    }

    #[test]
    fn test_get_selected_patchset() {
        let mut lp = LatestPatchsetsState::new("some-list".to_string(), 3);
        lp.current_page = vec![make_patch("id-1"), make_patch("id-2"), make_patch("id-3")];

        lp.patchset_index = 0;
        assert!(lp
            .get_selected_patchset()
            .message_id()
            .href
            .contains("id-1"));

        lp.patchset_index = 1;
        assert!(lp
            .get_selected_patchset()
            .message_id()
            .href
            .contains("id-2"));

        lp.patchset_index = 2;
        assert!(lp
            .get_selected_patchset()
            .message_id()
            .href
            .contains("id-3"));
    }

    #[test]
    #[should_panic]
    fn test_get_selected_patchset_invalid_index() {
        let mut lp = LatestPatchsetsState::new("some-list".to_string(), 3);
        lp.current_page = vec![make_patch("id-1")];
        lp.patchset_index = 99;
        let _patch = lp.get_selected_patchset();
    }
}
