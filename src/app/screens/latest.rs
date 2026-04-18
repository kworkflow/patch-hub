use color_eyre::eyre::bail;
use derive_getters::Getters;

use crate::lore::{
    lore_api_client::{ClientError, PatchFeedRequest},
    lore_session::{LoreSession, LoreSessionError},
    domain::patch::Patch,
};

#[derive(Getters)]
pub struct LatestPatchsets {
    lore_session: LoreSession,
    lore_api_client: Box<dyn PatchFeedRequest>,
    target_list: String,
    page_number: usize,
    patchset_index: usize,
    page_size: usize,
}

impl LatestPatchsets {
    pub fn new(
        target_list: String,
        page_size: usize,
        lore_api_client: Box<dyn PatchFeedRequest>,
    ) -> LatestPatchsets {
        LatestPatchsets {
            lore_session: LoreSession::new(target_list.clone()),
            lore_api_client,
            target_list,
            page_number: 1,
            patchset_index: 0,
            page_size,
        }
    }

    pub fn fetch_current_page(&mut self) -> color_eyre::Result<()> {
        if let Err(lore_session_error) = self.lore_session.process_n_representative_patches(
            self.lore_api_client.as_ref(),
            self.page_size * self.page_number,
        ) {
            match lore_session_error {
                LoreSessionError::FromLoreAPIClient(client_error) => match client_error {
                    ClientError::Net(_) => {
                        bail!("Failed to request feed\n{client_error:#?}")
                    }
                    ClientError::EndOfFeed => (),
                },
            }
        };
        Ok(())
    }

    pub fn select_below_patchset(&mut self) {
        if self.patchset_index + 1 < self.lore_session.representative_patches_ids().len()
            && self.patchset_index + 1 < self.page_size * self.page_number
        {
            self.patchset_index += 1;
        }
    }

    pub fn select_above_patchset(&mut self) {
        if self.patchset_index == 0 {
            return;
        }
        if self.patchset_index > self.page_size * (&self.page_number - 1) {
            self.patchset_index -= 1;
        }
    }

    pub fn increment_page(&mut self) {
        let patchsets_processed: usize = self.lore_session.representative_patches_ids().len();
        if self.page_size * self.page_number > patchsets_processed {
            return;
        }
        self.page_number += 1;
        self.patchset_index = self.page_size * (&self.page_number - 1);
    }

    pub fn decrement_page(&mut self) {
        if self.page_number == 1 {
            return;
        }
        self.page_number -= 1;
        self.patchset_index = self.page_size * (&self.page_number - 1);
    }

    pub fn get_selected_patchset(&self) -> Patch {
        let message_id: &str = self
            .lore_session
            .representative_patches_ids()
            .get(self.patchset_index)
            .unwrap();

        self.lore_session
            .get_processed_patch(message_id)
            .unwrap()
            .clone()
    }

    pub fn get_current_patch_feed_page(&self) -> Option<Vec<&Patch>> {
        self.lore_session
            .get_patch_feed_page(self.page_size, self.page_number)
    }

    pub fn processed_patchsets_count(&self) -> usize {
        self.lore_session.representative_patches_ids().len()
    }
}

#[cfg(test)]
mod tests {
    use crate::lore::lore_api_client::MockPatchFeedRequest;
    use std::fs;

    use super::*;

    #[test]
    fn test_fetch_current_page_success() {
        let src_path =
            "test_samples/lore_session/process_representative_patch/patch_feed_sample_1.xml";
        let target_list = "some-list";

        let mut lore_api_client = MockPatchFeedRequest::new();

        lore_api_client
            .expect_request_patch_feed()
            .withf(move |target_list_arg, min_index_arg| {
                target_list_arg == target_list && *min_index_arg == 0
            })
            .times(1)
            .returning(move |_, _| Ok(fs::read_to_string(src_path).unwrap()));

        let mut latest_patchsets =
            LatestPatchsets::new(target_list.to_string(), 0, Box::new(lore_api_client));
        latest_patchsets.page_size = 1;
        latest_patchsets.page_number = 1;

        let fetch_result = latest_patchsets.fetch_current_page();

        assert!(fetch_result.is_ok());
        assert_eq!(latest_patchsets.page_size, 1);
        assert_eq!(latest_patchsets.page_number, 1);
        assert_eq!(latest_patchsets.patchset_index, 0);

        assert_eq!(latest_patchsets.processed_patchsets_count(), 1);
    }

    #[test]
    fn test_fetch_current_page_end_of_feed() {
        let mut lore_api_client = MockPatchFeedRequest::new();
        let target_list = "some-list";

        lore_api_client
            .expect_request_patch_feed()
            .withf(move |target_list_arg, min_index_arg| {
                target_list_arg == target_list && *min_index_arg == 0
            })
            .times(1)
            .returning(move |_, _| Err(ClientError::EndOfFeed));

        let mut latest_patchsets =
            LatestPatchsets::new(target_list.to_string(), 0, Box::new(lore_api_client));
        latest_patchsets.page_size = 1;
        latest_patchsets.page_number = 1;

        assert_eq!(latest_patchsets.patchset_index, 0);
        assert_eq!(latest_patchsets.processed_patchsets_count(), 0);

        let fetch_result = latest_patchsets.fetch_current_page();

        assert!(fetch_result.is_ok());
        assert_eq!(latest_patchsets.patchset_index, 0);

        assert_eq!(latest_patchsets.processed_patchsets_count(), 0);
    }

    #[test]
    fn test_fetch_current_page_client_error() {
        let mut lore_api_client = MockPatchFeedRequest::new();
        let target_list = "some-list";

        lore_api_client
            .expect_request_patch_feed()
            .withf(move |target_list_arg, min_index_arg| {
                target_list_arg == target_list && *min_index_arg == 0
            })
            .times(1)
            .returning(move |_, _| {
                Err(ClientError::Net(
                    crate::infrastructure::net::NetError::HttpStatus {
                        code: 401,
                        message: "HTTP 401".to_string(),
                    },
                ))
            });

        let mut latest_patchsets =
            LatestPatchsets::new(target_list.to_string(), 0, Box::new(lore_api_client));
        latest_patchsets.page_size = 1;
        latest_patchsets.page_number = 1;

        let fetch_result = latest_patchsets.fetch_current_page();

        assert!(fetch_result.is_err());
        assert_eq!(latest_patchsets.patchset_index, 0);

        assert_eq!(latest_patchsets.processed_patchsets_count(), 0);
    }

    #[test]
    fn test_select_below_patchset() {
        // initializing LatestPatchsets so we can test select_below_patchset properly
        let mut latest_patchsets = {
            let target_list = "some-list";
            let page_size = 3;

            let src_path =
                "test_samples/lore_session/process_representative_patch/patch_feed_sample_2.xml";

            let mut lore_api_client = MockPatchFeedRequest::new();

            let target_list_string = target_list.to_string();
            lore_api_client
                .expect_request_patch_feed()
                .withf(move |target_list_arg, min_index_arg| {
                    target_list_arg == target_list_string && *min_index_arg == 0
                })
                .times(1)
                .returning(move |_, _| Ok(fs::read_to_string(src_path).unwrap()));
            LatestPatchsets::new(
                target_list.to_string(),
                page_size,
                Box::new(lore_api_client),
            )
        };

        // asserting we have patchsets to read
        latest_patchsets.fetch_current_page().expect("to fetch");
        assert_eq!(latest_patchsets.processed_patchsets_count(), 3);

        // test case 1: base case
        latest_patchsets.patchset_index = 0;
        latest_patchsets.page_size = 0;
        latest_patchsets.page_number = 1;

        latest_patchsets.select_below_patchset();
        assert_eq!(latest_patchsets.patchset_index(), 0);

        // test case 2: selecting below patchset is possible
        latest_patchsets.patchset_index = 0;
        latest_patchsets.page_size = 2;
        latest_patchsets.page_number = 1;

        latest_patchsets.select_below_patchset();
        assert_eq!(latest_patchsets.patchset_index(), 1);

        // test case 3: already on the bottom of the page
        latest_patchsets.patchset_index = 1;
        latest_patchsets.page_size = 2;
        latest_patchsets.page_number = 1;

        latest_patchsets.select_below_patchset();
        assert_eq!(latest_patchsets.patchset_index(), 1);

        // test case 4: incrementing page so we can select below patchset
        latest_patchsets.patchset_index = 1;
        latest_patchsets.page_size = 2;
        latest_patchsets.page_number = 2;

        latest_patchsets.select_below_patchset();
        assert_eq!(latest_patchsets.patchset_index(), 2);
    }

    #[test]
    fn test_select_above_patchset() {
        let mut latest_patchsets =
            LatestPatchsets::new("".to_string(), 0, Box::new(MockPatchFeedRequest::new()));

        // test case 1: base case
        latest_patchsets.patchset_index = 0;
        latest_patchsets.page_size = 0;
        latest_patchsets.page_number = 1;

        latest_patchsets.select_above_patchset();
        assert_eq!(latest_patchsets.patchset_index(), 0);

        // test case 2: patchset 0 is always the topmost
        latest_patchsets.patchset_index = 0;
        latest_patchsets.page_size = 10;
        latest_patchsets.page_number = 2;

        latest_patchsets.select_above_patchset();
        assert_eq!(latest_patchsets.patchset_index(), 0);

        // test case 3: current patchset is already the top of the page
        latest_patchsets.patchset_index = 2;
        latest_patchsets.page_size = 2;
        latest_patchsets.page_number = 2;

        latest_patchsets.select_above_patchset();
        assert_eq!(latest_patchsets.patchset_index(), 2);

        // test case 4: selecting above until the end of the page
        latest_patchsets.patchset_index = 24;
        latest_patchsets.page_size = 5;
        latest_patchsets.page_number = 5;

        latest_patchsets.select_above_patchset();
        assert_eq!(latest_patchsets.patchset_index(), 23);

        latest_patchsets.select_above_patchset();
        assert_eq!(latest_patchsets.patchset_index(), 22);

        latest_patchsets.select_above_patchset();
        assert_eq!(latest_patchsets.patchset_index(), 21);

        latest_patchsets.select_above_patchset();
        assert_eq!(latest_patchsets.patchset_index(), 20);

        latest_patchsets.select_above_patchset();
        assert_eq!(latest_patchsets.patchset_index(), 20);
    }

    #[test]
    fn test_increment_page() {
        // initializing LatestPatchsets so we can test increment_page properly
        let mut latest_patchsets = {
            let target_list = "some-list";
            let page_size = 3;

            let src_path =
                "test_samples/lore_session/process_representative_patch/patch_feed_sample_2.xml";

            let mut lore_api_client = MockPatchFeedRequest::new();

            let target_list_string = target_list.to_string();
            lore_api_client
                .expect_request_patch_feed()
                .withf(move |target_list_arg, min_index_arg| {
                    target_list_arg == target_list_string && *min_index_arg == 0
                })
                .times(1)
                .returning(move |_, _| Ok(fs::read_to_string(src_path).unwrap()));
            LatestPatchsets::new(
                target_list.to_string(),
                page_size,
                Box::new(lore_api_client),
            )
        };

        // asserting we have patchsets to read
        latest_patchsets.fetch_current_page().expect("to fetch");
        assert_eq!(latest_patchsets.processed_patchsets_count(), 3);

        // test case 1: success incrementing page
        latest_patchsets.patchset_index = 0;
        latest_patchsets.page_size = 1;
        latest_patchsets.page_number = 1;

        latest_patchsets.increment_page();
        assert_eq!(latest_patchsets.patchset_index, 1);
        assert_eq!(latest_patchsets.page_size, 1);
        assert_eq!(latest_patchsets.page_number, 2);

        // test case 3: patchset index is overwritten when page is incremented
        latest_patchsets.patchset_index = 999;
        latest_patchsets.page_size = 1;
        latest_patchsets.page_number = 1;

        latest_patchsets.increment_page();
        assert_eq!(latest_patchsets.patchset_index, 1);
        assert_eq!(latest_patchsets.page_size, 1);
        assert_eq!(latest_patchsets.page_number, 2);

        // test case 4: won't increment after max page
        latest_patchsets.patchset_index = 0;
        latest_patchsets.page_size = 10;
        latest_patchsets.page_number = 1;

        latest_patchsets.increment_page();
        assert_eq!(latest_patchsets.patchset_index, 0);
        assert_eq!(latest_patchsets.page_size, 10);
        assert_eq!(latest_patchsets.page_number, 1);

        // test case 5: sequencial increments
        latest_patchsets.patchset_index = 0;
        latest_patchsets.page_size = 1;
        latest_patchsets.page_number = 1;

        latest_patchsets.increment_page();
        assert_eq!(latest_patchsets.patchset_index, 1);
        assert_eq!(latest_patchsets.page_size, 1);
        assert_eq!(latest_patchsets.page_number, 2);

        latest_patchsets.increment_page();
        assert_eq!(latest_patchsets.patchset_index, 2);
        assert_eq!(latest_patchsets.page_size, 1);
        assert_eq!(latest_patchsets.page_number, 3);

        latest_patchsets.increment_page();
        assert_eq!(latest_patchsets.patchset_index, 3);
        assert_eq!(latest_patchsets.page_size, 1);
        assert_eq!(latest_patchsets.page_number, 4);

        latest_patchsets.increment_page();
        assert_eq!(latest_patchsets.patchset_index, 3);
        assert_eq!(latest_patchsets.page_size, 1);
        assert_eq!(latest_patchsets.page_number, 4);
    }

    #[test]
    fn test_decrement_page() {
        let mut latest_patchsets =
            LatestPatchsets::new("".to_string(), 0, Box::new(MockPatchFeedRequest::new()));

        // test case 1: already in the first page
        latest_patchsets.page_number = 1;
        latest_patchsets.page_size = 0;

        latest_patchsets.decrement_page();
        assert_eq!(latest_patchsets.page_number(), 1);
        assert_eq!(latest_patchsets.patchset_index(), 0);

        // test case 2: second page
        latest_patchsets.page_number = 2;
        latest_patchsets.page_size = 3;
        latest_patchsets.patchset_index = 9; // this doesn't matter, will be overwritten

        latest_patchsets.decrement_page();
        assert_eq!(latest_patchsets.page_number(), 1);
        assert_eq!(latest_patchsets.patchset_index(), 0);

        // test case 3: decrementing page until the first
        latest_patchsets.page_number = 5;
        latest_patchsets.page_size = 100;

        latest_patchsets.decrement_page();
        assert_eq!(latest_patchsets.page_number(), 4);
        assert_eq!(latest_patchsets.patchset_index(), 300);

        latest_patchsets.decrement_page();
        assert_eq!(latest_patchsets.page_number(), 3);
        assert_eq!(latest_patchsets.patchset_index(), 200);

        latest_patchsets.decrement_page();
        assert_eq!(latest_patchsets.page_number(), 2);
        assert_eq!(latest_patchsets.patchset_index(), 100);

        latest_patchsets.decrement_page();
        assert_eq!(latest_patchsets.page_number(), 1);
        assert_eq!(latest_patchsets.patchset_index(), 0);

        latest_patchsets.decrement_page();
        assert_eq!(latest_patchsets.page_number(), 1);
        assert_eq!(latest_patchsets.patchset_index(), 0);
    }

    #[test]
    #[should_panic]
    fn test_get_selected_patchset_before_fetching_page() {
        let latest_patchsets =
            LatestPatchsets::new("".to_string(), 3, Box::new(MockPatchFeedRequest::new()));

        let _patch = latest_patchsets.get_selected_patchset();
    }

    #[test]
    fn test_get_selected_patchset() {
        // initializing LatestPatchsets so we can test get_selected_patchset properly
        let mut latest_patchsets = {
            let target_list = "some-list";
            let page_size = 3;

            let src_path =
                "test_samples/lore_session/process_representative_patch/patch_feed_sample_2.xml";

            let mut lore_api_client = MockPatchFeedRequest::new();

            let target_list_string = target_list.to_string();
            lore_api_client
                .expect_request_patch_feed()
                .withf(move |target_list_arg, min_index_arg| {
                    target_list_arg == target_list_string && *min_index_arg == 0
                })
                .times(1)
                .returning(move |_, _| Ok(fs::read_to_string(src_path).unwrap()));
            LatestPatchsets::new(
                target_list.to_string(),
                page_size,
                Box::new(lore_api_client),
            )
        };
        // asserting we have patchsets to read
        latest_patchsets.fetch_current_page().expect("to fetch");
        assert_eq!(latest_patchsets.processed_patchsets_count(), 3);

        latest_patchsets.patchset_index = 0;
        let patch = latest_patchsets.get_selected_patchset();
        assert!(patch
            .message_id()
            .href
            .contains("1234.567-1-roberto@silva.br"));

        latest_patchsets.patchset_index = 1;
        let patch = latest_patchsets.get_selected_patchset();
        assert!(patch.message_id().href.contains("first-patch-lima@luma.rs"));

        latest_patchsets.patchset_index = 2;
        let patch = latest_patchsets.get_selected_patchset();
        assert!(patch
            .message_id()
            .href
            .contains("1234.567-1-john@johnson.com"));
    }

    #[test]
    #[should_panic]
    fn test_get_selected_patchset_invalid_index() {
        let mut latest_patchsets = {
            let target_list = "some-list";
            let page_size = 3;

            let src_path =
                "test_samples/lore_session/process_representative_patch/patch_feed_sample_2.xml";

            let mut lore_api_client = MockPatchFeedRequest::new();

            let target_list_string = target_list.to_string();
            lore_api_client
                .expect_request_patch_feed()
                .withf(move |target_list_arg, min_index_arg| {
                    target_list_arg == target_list_string && *min_index_arg == 0
                })
                .times(1)
                .returning(move |_, _| Ok(fs::read_to_string(src_path).unwrap()));
            LatestPatchsets::new(
                target_list.to_string(),
                page_size,
                Box::new(lore_api_client),
            )
        };
        // asserting we have patchsets to read
        latest_patchsets.fetch_current_page().expect("to fetch");
        assert_eq!(latest_patchsets.processed_patchsets_count(), 3);

        latest_patchsets.patchset_index = 999;
        let _patch = latest_patchsets.get_selected_patchset();
    }
}
