use color_eyre::eyre::bail;
use lore_peek::{
    lore_session::{
        self, LoreSession
    },
    lore_api_client::{
        BlockingLoreAPIClient, FailedFeedRequest
    },
    patch::Patch
};

pub const PAGE_SIZE: u32 = 30;
const PATCHSETS_CACHE_DIR: &str = "/home/davidbtadokoro/Desktop/patchsets";

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

    pub fn get_selected_patchset(self: &Self) -> Patch {
        let message_id: &str = self.lore_session
            .get_representative_patches_ids()
            .get(self.patchset_index as usize)
            .unwrap();

        self.lore_session
            .get_processed_patch(message_id)
            .unwrap()
            .clone()
    }

    pub fn get_current_patch_feed_page(self: &Self) -> Option<Vec<&Patch>> {
        self.lore_session.get_patch_feed_page(PAGE_SIZE, self.page_number)
    }
}

pub struct PatchsetDetailsAndActionsState {
    pub representative_patch: Patch,
    pub patches: Vec<String>,
    pub preview_index: u32,
    pub preview_scroll_offset: u32,
}

impl PatchsetDetailsAndActionsState {
    pub fn preview_next_patch(self: &mut Self) {
        if ((self.preview_index as usize) + 1) < self.patches.len() {
            self.preview_index += 1;
            self.preview_scroll_offset = 0;
        }
    }

    pub fn preview_previous_patch(self: &mut Self) {
        if (self.preview_index as usize) > 0 {
            self.preview_index -= 1;
            self.preview_scroll_offset = 0;
        }
    }

    pub fn preview_scroll_down(self: &mut Self) {
        let number_of_lines = self.patches[self.preview_index as usize].lines().count();
        if ((self.preview_scroll_offset as usize) + 1) <= number_of_lines {
            self.preview_scroll_offset += 1;
        }
    }

    pub fn preview_scroll_up(self: &mut Self) {
        if (self.preview_scroll_offset as usize) > 0 {
            self.preview_scroll_offset -= 1;
        }
    }
}

pub enum CurrentScreen {
    MailingListSelection,
    LatestPatchsets,
    PatchsetDetails,
}

pub struct App {
    pub current_screen: CurrentScreen,
    pub target_list: String,
    pub latest_patchsets_state: Option<LatestPatchsetsState>,
    pub patchset_details_and_actions_state: Option<PatchsetDetailsAndActionsState>,
}

impl App {
    pub fn new() -> App {
        App {
            current_screen: CurrentScreen::MailingListSelection,
            target_list: String::new(),
            latest_patchsets_state: None,
            patchset_details_and_actions_state: None,
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

    pub fn init_patchset_details_and_actions_state(self: &mut Self) -> color_eyre::Result<()> {
        let representative_patch = self.latest_patchsets_state
            .as_ref().unwrap().get_selected_patchset();
        let patchset_path: String;

        match lore_session::download_patchset(PATCHSETS_CACHE_DIR, &representative_patch) {
            Ok(result) => patchset_path = result,
            Err(io_error) => bail!("{io_error}"),
        }

        match lore_session::split_patchset(&patchset_path) {
            Ok(patches) => {
                self.patchset_details_and_actions_state = Some(
                    PatchsetDetailsAndActionsState {
                        representative_patch,
                        patches,
                        preview_index: 0,
                        preview_scroll_offset: 0,
                    }
                );
                Ok(())
            },
            Err(message) => bail!(message),
        }
    }

    pub fn reset_patchset_details_and_actions_state(self: &mut Self) {
        self.patchset_details_and_actions_state = None;
    }

    pub fn set_current_screen(self: &mut Self, new_current_screen: CurrentScreen) {
        self.current_screen = new_current_screen;
    }
}
