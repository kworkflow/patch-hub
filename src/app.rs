use std::collections::HashMap;
use color_eyre::eyre::bail;
use lore_peek::{
    lore_session::{
        self, LoreSession
    },
    lore_api_client::{
        BlockingLoreAPIClient, FailedFeedRequest
    },
    mailing_list::MailingList,
    patch::Patch
};

pub const PAGE_SIZE: u32 = 30;
const PATCHSETS_CACHE_DIR: &str = "/home/davidbtadokoro/Desktop/patchsets";
const BOOKMARKED_PATCHSETS_PATH: &str = "/home/davidbtadokoro/Desktop/patchsets/bookmarked_patchsets.json";
const MAILING_LISTS_PATH: &str = "/home/davidbtadokoro/Desktop/patchsets/mailing_lists.json";

pub struct BookmarkedPatchsetsState {
    pub bookmarked_patchsets: Vec<Patch>,
    pub patchset_index: u32,
}

impl BookmarkedPatchsetsState {
    pub fn select_below_patchset(self: &mut Self) {
        if (self.patchset_index as usize) + 1 < self.bookmarked_patchsets.len() {
            self.patchset_index += 1;
        }
    }

    pub fn select_above_patchset(self: &mut Self) {
        self.patchset_index = self.patchset_index.saturating_sub(1);
    }

    fn get_selected_patchset(self: &Self) -> Patch {
        self.bookmarked_patchsets
            .get(self.patchset_index as usize)
            .unwrap()
            .clone()
    }

    fn bookmark_selected_patch(self: &mut Self, patch_to_bookmark: &Patch) {
        if !self.bookmarked_patchsets.contains(patch_to_bookmark) {
            self.bookmarked_patchsets.push(patch_to_bookmark.clone());
        }
    }

    fn unbookmark_selected_patch(self: &mut Self, patch_to_unbookmark: &Patch) {
        if let Some(index) = self.bookmarked_patchsets.iter().position(
            |r| r == patch_to_unbookmark
        ) {
            self.bookmarked_patchsets.remove(index);
        }
    }
}

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
    pub patchset_actions: HashMap<PatchsetAction, bool>,
    pub last_screen: CurrentScreen,
}

#[derive(Hash, Eq, PartialEq)]
pub enum PatchsetAction {
    Bookmark,
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

    pub fn toggle_bookmark_action(self: &mut Self) {
        let bookmark_action = PatchsetAction::Bookmark;
        let current_bookmark_value = *self.patchset_actions.get(&bookmark_action).unwrap();
        self.patchset_actions.insert(bookmark_action, !current_bookmark_value);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CurrentScreen {
    MailingListSelection,
    BookmarkedPatchsets,
    LatestPatchsets,
    PatchsetDetails,
}

pub struct App {
    pub current_screen: CurrentScreen,
    pub mailing_lists: Vec<MailingList>,
    pub target_list: String,
    pub bookmarked_patchsets_state: BookmarkedPatchsetsState,
    pub latest_patchsets_state: Option<LatestPatchsetsState>,
    pub patchset_details_and_actions_state: Option<PatchsetDetailsAndActionsState>,
}

impl App {
    pub fn new() -> App {
        let mailing_lists: Vec<MailingList>;
        let bookmarked_patchsets: Vec<Patch>;

        match lore_session::load_available_lists(MAILING_LISTS_PATH) {
            Ok(vec_of_mailing_lists) => mailing_lists = vec_of_mailing_lists,
            Err(_) => mailing_lists = Vec::new(),
        }

        match lore_session::load_bookmarked_patchsets(BOOKMARKED_PATCHSETS_PATH) {
            Ok(vec_of_patchsets) => bookmarked_patchsets = vec_of_patchsets,
            Err(_) => bookmarked_patchsets = Vec::new(),
        }

        App {
            current_screen: CurrentScreen::MailingListSelection,
            mailing_lists,
            target_list: String::new(),
            latest_patchsets_state: None,
            patchset_details_and_actions_state: None,
            bookmarked_patchsets_state: BookmarkedPatchsetsState {
                bookmarked_patchsets,
                patchset_index: 0,
            },
        }
    }

    pub fn refresh_available_mailing_lists(self: &mut Self) -> color_eyre::Result<()> {
        let lore_api_client = BlockingLoreAPIClient::new();

        match lore_session::fetch_available_lists(&lore_api_client) {
            Ok(available_mailing_lists) => {
                self.mailing_lists = available_mailing_lists;
            },
            Err(failed_available_lists_request) => {
                bail!(format!("{failed_available_lists_request:#?}"));
            },
        };

        lore_session::save_available_lists(&self.mailing_lists, MAILING_LISTS_PATH)?;

        Ok(())
    }

    pub fn init_latest_patchsets_state(self: &mut Self) {
        if let None = self.latest_patchsets_state {
            self.latest_patchsets_state = Some(LatestPatchsetsState::new(self.target_list.clone()));
        }
    }

    pub fn reset_latest_patchsets_state(self: &mut Self) {
        self.latest_patchsets_state = None;
    }

    pub fn init_patchset_details_and_actions_state(self: &mut Self, current_screen: CurrentScreen) -> color_eyre::Result<()> {
        let representative_patch: Patch;
        let mut is_patchset_bookmarked = true;
        let patchset_path: String;

        match current_screen {
            CurrentScreen::BookmarkedPatchsets => {
                representative_patch = self.bookmarked_patchsets_state.get_selected_patchset();
            },
            CurrentScreen::LatestPatchsets => {
                representative_patch = self.latest_patchsets_state.as_ref().unwrap().get_selected_patchset();
                if !self.bookmarked_patchsets_state.bookmarked_patchsets.contains(&representative_patch) {
                    is_patchset_bookmarked = false;
                }
            },
            screen => bail!(format!("Invalid screen passed as argument {screen:?}"))
        };

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
                        patchset_actions: HashMap::from([
                            (PatchsetAction::Bookmark, is_patchset_bookmarked),
                        ]),
                        last_screen: current_screen,
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

    pub fn consolidate_patchset_actions(self: &mut Self) {
        let representative_patch = &self.patchset_details_and_actions_state
            .as_ref()
            .unwrap()
            .representative_patch;

        let should_bookmark_patchset = *self
            .patchset_details_and_actions_state.as_ref().unwrap()
            .patchset_actions.get(&PatchsetAction::Bookmark).unwrap();
        if should_bookmark_patchset {
            self.bookmarked_patchsets_state.bookmark_selected_patch(representative_patch);
        } else {
            self.bookmarked_patchsets_state.unbookmark_selected_patch(representative_patch);
        }
    }

    pub fn save_bookmarked_patchsets(self: &Self) -> color_eyre::Result<()> {
        lore_session::save_bookmarked_patchsets(
            &self.bookmarked_patchsets_state.bookmarked_patchsets, BOOKMARKED_PATCHSETS_PATH
        )?;
        Ok(())
    }

    pub fn set_current_screen(self: &mut Self, new_current_screen: CurrentScreen) {
        self.current_screen = new_current_screen;
    }
}
