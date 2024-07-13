use color_eyre::eyre::bail;
use lore_peek::{
    lore_session::LoreSession,
    patch::Patch,
    lore_api_client::{
        BlockingLoreAPIClient, FailedFeedRequest
    },
};

pub const PAGE_SIZE: u32 = 30;

pub struct LatestPatchsetsState {
    lore_session: LoreSession,
    lore_api_client: BlockingLoreAPIClient,
    page_number: u32,
    patchset_index: u32,
}

impl LatestPatchsetsState {
    pub fn new(target_list: String) -> LatestPatchsetsState {
        LatestPatchsetsState {
            lore_session: LoreSession::new(target_list),
            lore_api_client: BlockingLoreAPIClient::new(),
            page_number: 1,
            patchset_index: 0,
        }
    }

    pub fn fetch_current_page(self: &mut Self) -> color_eyre::Result<()> {
        if let Err(failed_feed_request) = self
            .lore_session.process_n_representative_patches(&self.lore_api_client, PAGE_SIZE * &self.page_number) {
            match failed_feed_request {
                FailedFeedRequest::UnknownError(error) => bail!("[FailedFeedRequest::UnknownError]\n*\tFailed to request feed\n*\t{error:#?}"),
                FailedFeedRequest::StatusNotOk(feed_response) => bail!("[FailedFeedRequest::StatusNotOk]\n*\tRequest returned with non-OK status\n*\t{feed_response:#?}"),
                FailedFeedRequest::EndOfFeed => (),
            }
        };
        Ok(())
    }

    pub fn select_below_patchset(self: &mut Self) {
        if self.patchset_index + 1 < PAGE_SIZE * &self.page_number {
            self.patchset_index += 1;
        }
    }

    pub fn select_above_patchset(self: &mut Self) {
        if self.patchset_index == 0 {
            return;
        }
        if self.patchset_index - 1 >= PAGE_SIZE * (&self.page_number - 1) {
            self.patchset_index -= 1;
        }
    }

    pub fn increment_page(self: &mut Self) {
        let patchsets_processed: u32 = self.lore_session.get_representative_patches_ids().len().try_into().unwrap();
        if PAGE_SIZE * self.page_number > patchsets_processed {
            return;
        }
        self.page_number += 1; 
        self.patchset_index = PAGE_SIZE * (&self.page_number - 1);
    }

    pub fn decrement_page(self: &mut Self) {
        if self.page_number == 1 {
            return;
        } 
        self.page_number -= 1; 
        self.patchset_index = PAGE_SIZE * (&self.page_number - 1);
    }

    pub fn get_page_number(self: &Self) -> u32 {
        self.page_number
    }

    pub fn get_patchset_index(self: &Self) -> u32 {
        self.patchset_index
    }

    pub fn get_current_patch_feed_page(self: &Self) -> Option<Vec<&Patch>> {
        self.lore_session.get_patch_feed_page(PAGE_SIZE, self.page_number)
    }
}

pub enum CurrentScreen {
    MailingListSelection,
    LatestPatchsets,
}

pub struct App {
    pub current_screen: CurrentScreen,
    pub target_list: String,
    pub latest_patchsets_state: Option<LatestPatchsetsState>
}

impl App {
    pub fn new() -> App {
        App {
            current_screen: CurrentScreen::MailingListSelection,
            target_list: String::new(),
            latest_patchsets_state: None,
        }
    }

    pub fn init_latest_patchsets_state(self: &mut Self) {
        if let None = self.latest_patchsets_state {
            self.latest_patchsets_state = Some(LatestPatchsetsState::new(self.target_list.clone()));
        }
    }

    pub fn reset_latest_patchsets_state(self: &mut Self) {
        self.latest_patchsets_state = None;
    }

    pub fn set_current_screen(self: &mut Self, new_current_screen: CurrentScreen) {
        self.current_screen = new_current_screen;
    }
}
