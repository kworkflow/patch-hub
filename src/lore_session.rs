use crate::patch::{Patch, PatchFeed, PatchRegex};
use std::collections::HashMap;


pub struct LoreSession {
    representative_patches_ids: Vec<String>,
    processed_patches_map: HashMap<String, Patch>,
    patch_regex: PatchRegex,
}

impl LoreSession {
    pub fn new() -> LoreSession {
        LoreSession {
            representative_patches_ids: Vec::new(),
            processed_patches_map: HashMap::new(),
            patch_regex: PatchRegex::new(),
        }
    }

    pub fn get_representative_patches_ids(self: &Self) -> &Vec<String> {
        &self.representative_patches_ids
    }

    pub fn get_processed_patch(self: &Self, message_id: &str) -> Option<&Patch> {
        self.processed_patches_map.get(message_id)
    }

    pub fn process_patches(self: &mut Self, patch_feed: PatchFeed) -> Vec<String> {
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

    pub fn update_representative_patches(self: &mut Self, processed_patches_ids: Vec<String>) {
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
