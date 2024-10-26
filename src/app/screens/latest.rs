use color_eyre::eyre::bail;
use derive_getters::Getters;
use patch_hub::lore::{
    lore_api_client::{BlockingLoreAPIClient, ClientError},
    lore_session::{LoreSession, LoreSessionError},
    patch::Patch,
};

#[derive(Getters)]
pub struct LatestPatchsetsState {
    lore_session: LoreSession,
    lore_api_client: BlockingLoreAPIClient,
    target_list: String,
    page_number: usize,
    patchset_index: usize,
    page_size: usize,
}

impl LatestPatchsetsState {
    pub fn new(
        target_list: String,
        page_size: usize,
        lore_api_client: BlockingLoreAPIClient,
    ) -> LatestPatchsetsState {
        LatestPatchsetsState {
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
            &self.lore_api_client,
            self.page_size * self.page_number,
        ) {
            match lore_session_error {
                LoreSessionError::FromLoreAPIClient(client_error) => match client_error {
                    ClientError::FromReqwest(_) | ClientError::UnexpectedResponse(_, _) => {
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
