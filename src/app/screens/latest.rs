use color_eyre::eyre::bail;
use patch_hub::{
    lore_api_client::{BlockingLoreAPIClient, FailedFeedRequest},
    lore_session::LoreSession,
    patch::Patch,
};

pub struct LatestPatchsetsState {
    lore_session: LoreSession,
    lore_api_client: BlockingLoreAPIClient,
    target_list: String,
    page_number: usize,
    patchset_index: usize,
    page_size: usize,
}

impl LatestPatchsetsState {
    pub fn new(target_list: String, page_size: usize) -> LatestPatchsetsState {
        LatestPatchsetsState {
            lore_session: LoreSession::new(target_list.clone()),
            lore_api_client: BlockingLoreAPIClient::new(),
            target_list,
            page_number: 1,
            patchset_index: 0,
            page_size,
        }
    }

    pub fn fetch_current_page(&mut self) -> color_eyre::Result<()> {
        if let Err(failed_feed_request) = self.lore_session.process_n_representative_patches(
            &self.lore_api_client,
            self.page_size * self.page_number,
        ) {
            match failed_feed_request {
                FailedFeedRequest::UnknownError(error) => bail!("[FailedFeedRequest::UnknownError]\n*\tFailed to request feed\n*\t{error:#?}"),
                FailedFeedRequest::StatusNotOk(feed_response) => bail!("[FailedFeedRequest::StatusNotOk]\n*\tRequest returned with non-OK status\n*\t{feed_response:#?}"),
                FailedFeedRequest::EndOfFeed => (),
            }
        };
        Ok(())
    }

    pub fn select_below_patchset(&mut self) {
        if self.patchset_index + 1 < self.lore_session.get_representative_patches_ids().len()
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
        let patchsets_processed: usize = self.lore_session.get_representative_patches_ids().len();
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

    pub fn get_target_list(&self) -> &str {
        &self.target_list
    }

    pub fn get_page_number(&self) -> usize {
        self.page_number
    }

    pub fn get_patchset_index(&self) -> usize {
        self.patchset_index
    }

    pub fn get_selected_patchset(&self) -> Patch {
        let message_id: &str = self
            .lore_session
            .get_representative_patches_ids()
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
}
