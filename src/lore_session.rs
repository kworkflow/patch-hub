use crate::patch::{Patch, PatchFeed, PatchRegex};
use crate::lore_api_client::{FailedFeedResquest, LoreAPIClient};
use std::collections::HashMap;
use serde_xml_rs::from_str;

const LORE_PAGE_SIZE: u32 = 200;

pub struct LoreSession {
    representative_patches_ids: Vec<String>,
    processed_patches_map: HashMap<String, Patch>,
    patch_regex: PatchRegex,
    target_list: String,
    min_index: u32,
}

impl LoreSession {
    pub fn new(target_list: String) -> LoreSession {
        LoreSession {
            target_list: target_list,
            representative_patches_ids: Vec::new(),
            processed_patches_map: HashMap::new(),
            patch_regex: PatchRegex::new(),
            min_index: 0,
        }
    }

    pub fn get_representative_patches_ids(self: &Self) -> &Vec<String> {
        &self.representative_patches_ids
    }

    pub fn get_processed_patch(self: &Self, message_id: &str) -> Option<&Patch> {
        self.processed_patches_map.get(message_id)
    }

    pub fn process_n_representative_patches(self: &mut Self,n: u32) {
        let mut patch_feed: PatchFeed;
        let mut processed_patches_ids: Vec<String>;

        while self.representative_patches_ids.len() < usize::try_from(n).unwrap() {
            match LoreAPIClient::request_patch_feed(&self.target_list, self.min_index) {
                Ok(feed_response_body) => patch_feed = from_str(&feed_response_body).unwrap(),
                Err(failed_feed_request) => match failed_feed_request {
                    FailedFeedResquest::UnknowError(error) => panic!("{error:#?}"),
                    FailedFeedResquest::StatusNotOk(status_code) => panic!("Lore request returned status code {status_code}"),
                    FailedFeedResquest::EndOfFeed => break,
                },
            }

            processed_patches_ids = self.process_patches(patch_feed);
            self.update_representative_patches(processed_patches_ids);

            self.min_index = self.min_index + LORE_PAGE_SIZE;
        }
    }

    fn process_patches(self: &mut Self, patch_feed: PatchFeed) -> Vec<String> {
        let mut processed_patches_ids: Vec<String> = Vec::new();

        for mut patch in patch_feed.get_patches() {
            patch.update_patch_metadata(&self.patch_regex);

            if !self.processed_patches_map.contains_key(&patch.get_message_id().href) {
                processed_patches_ids.push(patch.get_message_id().href.clone());
                self.processed_patches_map.insert(patch.get_message_id().href.clone(), patch);
            }
        }

        processed_patches_ids
    }

    fn update_representative_patches(self: &mut Self, processed_patches_ids: Vec<String>) {
        let mut patch: &Patch;
        let mut patch_number_in_series: u32;

        for message_id in processed_patches_ids {
            patch = self.processed_patches_map.get(&message_id).unwrap();
            patch_number_in_series = patch.get_number_in_series();

            if patch_number_in_series > 1 {
                continue;
            }
            
            if patch_number_in_series == 1 {
                if let Some(in_reply_to) = &patch.get_in_reply_to() {
                    if let Some(patch_in_reply_to) = self.processed_patches_map.get(&in_reply_to.href) {
                        if (patch_in_reply_to.get_number_in_series() == 0) &&
                            (patch.get_version() == patch_in_reply_to.get_version())
                        {
                            continue;
                        };
                    };
                };
            }

            self.representative_patches_ids.push(patch.get_message_id().href.clone());
        }
    }
}
