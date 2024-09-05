use std::{collections::HashMap, path::Path, process::Command};
use color_eyre::eyre::bail;
use config::Config;
use patch_hub::{
    lore_session::{
        self, LoreSession
    },
    lore_api_client::{
        BlockingLoreAPIClient, FailedFeedRequest
    },
    mailing_list::MailingList,
    patch::Patch
};

mod config;

pub struct BookmarkedPatchsetsState {
    pub bookmarked_patchsets: Vec<Patch>,
    pub patchset_index: u32,
}

impl BookmarkedPatchsetsState {
    pub fn select_below_patchset(&mut self) {
        if (self.patchset_index as usize) + 1 < self.bookmarked_patchsets.len() {
            self.patchset_index += 1;
        }
    }

    pub fn select_above_patchset(&mut self) {
        self.patchset_index = self.patchset_index.saturating_sub(1);
    }

    fn get_selected_patchset(&self) -> Patch {
        self.bookmarked_patchsets
            .get(self.patchset_index as usize)
            .unwrap()
            .clone()
    }

    fn bookmark_selected_patch(&mut self, patch_to_bookmark: &Patch) {
        if !self.bookmarked_patchsets.contains(patch_to_bookmark) {
            self.bookmarked_patchsets.push(patch_to_bookmark.clone());
        }
    }

    fn unbookmark_selected_patch(&mut self, patch_to_unbookmark: &Patch) {
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
    target_list: String,
    page_number: u32,
    patchset_index: u32,
    page_size: u32,
}

impl LatestPatchsetsState {
    pub fn new(target_list: String, page_size: u32) -> LatestPatchsetsState {
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
        if let Err(failed_feed_request) = self
            .lore_session.process_n_representative_patches(&self.lore_api_client, self.page_size * self.page_number) {
            match failed_feed_request {
                FailedFeedRequest::UnknownError(error) => bail!("[FailedFeedRequest::UnknownError]\n*\tFailed to request feed\n*\t{error:#?}"),
                FailedFeedRequest::StatusNotOk(feed_response) => bail!("[FailedFeedRequest::StatusNotOk]\n*\tRequest returned with non-OK status\n*\t{feed_response:#?}"),
                FailedFeedRequest::EndOfFeed => (),
            }
        };
        Ok(())
    }

    pub fn select_below_patchset(&mut self) {
        if self.patchset_index + 1 < self.lore_session.get_representative_patches_ids().len() as u32 {
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
        let patchsets_processed: u32 = self.lore_session.get_representative_patches_ids().len().try_into().unwrap();
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

    pub fn get_page_number(&self) -> u32 {
        self.page_number
    }

    pub fn get_patchset_index(&self) -> u32 {
        self.patchset_index
    }

    pub fn get_selected_patchset(&self) -> Patch {
        let message_id: &str = self.lore_session
            .get_representative_patches_ids()
            .get(self.patchset_index as usize)
            .unwrap();

        self.lore_session
            .get_processed_patch(message_id)
            .unwrap()
            .clone()
    }

    pub fn get_current_patch_feed_page(&self) -> Option<Vec<&Patch>> {
        self.lore_session.get_patch_feed_page(self.page_size, self.page_number)
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
    ReplyWithReviewedBy,
}

impl PatchsetDetailsAndActionsState {
    pub fn preview_next_patch(&mut self) {
        if ((self.preview_index as usize) + 1) < self.patches.len() {
            self.preview_index += 1;
            self.preview_scroll_offset = 0;
        }
    }

    pub fn preview_previous_patch(&mut self) {
        if (self.preview_index as usize) > 0 {
            self.preview_index -= 1;
            self.preview_scroll_offset = 0;
        }
    }

    pub fn preview_scroll_down(&mut self) {
        let number_of_lines = self.patches[self.preview_index as usize].lines().count();
        if ((self.preview_scroll_offset as usize) + 1) <= number_of_lines {
            self.preview_scroll_offset += 1;
        }
    }

    pub fn preview_scroll_up(&mut self) {
        if (self.preview_scroll_offset as usize) > 0 {
            self.preview_scroll_offset -= 1;
        }
    }

    pub fn toggle_bookmark_action(&mut self) {
        self.toggle_action(PatchsetAction::Bookmark);
    }

    pub fn toggle_reply_with_reviewed_by_action(&mut self) {
        self.toggle_action(PatchsetAction::ReplyWithReviewedBy);
    }

    fn toggle_action(&mut self, patchset_action: PatchsetAction) {
        let current_value = *self.patchset_actions.get(&patchset_action).unwrap();
        self.patchset_actions.insert(patchset_action, !current_value);
    }

    pub fn actions_require_user_io(&self) -> bool {
        *self.patchset_actions.get(&PatchsetAction::ReplyWithReviewedBy).unwrap()
    }

    pub fn reply_patchset_with_reviewed_by(&self, target_list: &str) -> color_eyre::Result<Vec<u32>> {
        let lore_api_client = BlockingLoreAPIClient::new();
        let (git_user_name, git_user_email) = lore_session::get_git_signature("");
        let mut successful_indexes = Vec::new();

        if git_user_name.is_empty() || git_user_email.is_empty() {
            println!("`git config user.name` or `git config user.email` not set\nAborting...");
            return Ok(successful_indexes);
        }

        let tmp_dir = Command::new("mktemp")
            .arg("--directory")
            .output()
            .unwrap();
        let tmp_dir = Path::new(
            std::str::from_utf8(&tmp_dir.stdout).unwrap().trim()
        );

        let git_reply_commands = match lore_session::prepare_reply_patchset_with_reviewed_by(
            &lore_api_client, tmp_dir, target_list,
            &self.patches, &format!("{git_user_name} <{git_user_email}>")
        ) {
            Ok(commands_vector) => commands_vector,
            Err(failed_patch_html_request) => {
                bail!(format!("{failed_patch_html_request:#?}"));
            },
        };

        for (index, mut command) in git_reply_commands.into_iter().enumerate() {
            let mut child = command.spawn().unwrap();
            let exit_status = child.wait().unwrap();
            if exit_status.success() {
                successful_indexes.push(index as u32);
            }
        }

        Ok(successful_indexes)
    }
}

pub struct MailingListSelectionState {
    pub mailing_lists: Vec<MailingList>,
    pub target_list: String,
    pub possible_mailing_lists: Vec<MailingList>,
    pub highlighted_list_index: u32,
    pub mailing_lists_path: String,
}

impl MailingListSelectionState {
    pub fn refresh_available_mailing_lists(&mut self) -> color_eyre::Result<()> {
        let lore_api_client = BlockingLoreAPIClient::new();

        match lore_session::fetch_available_lists(&lore_api_client) {
            Ok(available_mailing_lists) => {
                self.mailing_lists = available_mailing_lists;
            },
            Err(failed_available_lists_request) => {
                bail!(format!("{failed_available_lists_request:#?}"));
            },
        };

        self.clear_target_list();

        lore_session::save_available_lists(
            &self.mailing_lists,
            &self.mailing_lists_path
        )?;

        Ok(())
    }


    pub fn remove_last_target_list_char(&mut self) {
        if !self.target_list.is_empty() {
            self.target_list.pop();
            self.process_possible_mailing_lists();
        }
    }

    pub fn push_char_to_target_list(&mut self, ch: char) {
        self.target_list.push(ch);
        self.process_possible_mailing_lists();
    }

    pub fn clear_target_list(&mut self) {
        self.target_list.clear();
        self.process_possible_mailing_lists();
    }

    fn process_possible_mailing_lists(&mut self) {
        let mut possible_mailing_lists: Vec<MailingList> = Vec::new();

        for mailing_list in &self.mailing_lists {
            if mailing_list.get_name().starts_with(&self.target_list) {
                possible_mailing_lists.push(mailing_list.clone());
            }
        }

        self.possible_mailing_lists = possible_mailing_lists;
        self.highlighted_list_index = 0;
    }

    pub fn highlight_below_list(&mut self) {
        if (self.highlighted_list_index as usize) + 1 < self.possible_mailing_lists.len() {
            self.highlighted_list_index += 1;
        }
    }

    pub fn highlight_above_list(&mut self) {
        self.highlighted_list_index = self.highlighted_list_index.saturating_sub(1);
    }

    pub fn has_valid_target_list(&self) -> bool {
        let list_length = self.possible_mailing_lists.len(); // Possible mailing list length
        let list_index = self.highlighted_list_index as usize; // Index of the selected mailing list

        if list_index < list_length {
            return true;
        }
        false
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
    pub mailing_list_selection_state: MailingListSelectionState,
    pub bookmarked_patchsets_state: BookmarkedPatchsetsState,
    pub latest_patchsets_state: Option<LatestPatchsetsState>,
    pub patchset_details_and_actions_state: Option<PatchsetDetailsAndActionsState>,
    pub reviewed_patchsets: HashMap<String, Vec<u32>>,
    pub config: Config,
}

impl App {
    pub fn new() -> App {
        let config: Config = Config::build();

        let mailing_lists = lore_session::load_available_lists(&config.mailing_lists_path).unwrap_or_default();

        let bookmarked_patchsets = lore_session::load_bookmarked_patchsets(&config.bookmarked_patchsets_path).unwrap_or_default();

        let reviewed_patchsets = lore_session::load_reviewed_patchsets(&config.reviewed_patchsets_path).unwrap_or_default();

        App {
            current_screen: CurrentScreen::MailingListSelection,
            mailing_list_selection_state: MailingListSelectionState {
                mailing_lists: mailing_lists.clone(),
                target_list: String::new(),
                possible_mailing_lists: mailing_lists,
                highlighted_list_index: 0,
                mailing_lists_path: config.mailing_lists_path.clone(),
            },
            latest_patchsets_state: None,
            patchset_details_and_actions_state: None,
            bookmarked_patchsets_state: BookmarkedPatchsetsState {
                bookmarked_patchsets,
                patchset_index: 0,
            },
            reviewed_patchsets,
            config
        }
    }

    pub fn init_latest_patchsets_state(&mut self) {
        // the target mailing list for "latest patchsets" is the highlighted
        // entry in the possible lists of "mailing list selection"
        let list_index = self.mailing_list_selection_state
            .highlighted_list_index as usize;
        let target_list = self.mailing_list_selection_state
            .possible_mailing_lists[list_index]
            .get_name().to_string();

        self.latest_patchsets_state = Some(
            LatestPatchsetsState::new(target_list, self.config.page_size)
        );
    }

    pub fn reset_latest_patchsets_state(&mut self) {
        self.latest_patchsets_state = None;
    }

    pub fn init_patchset_details_and_actions_state(&mut self, current_screen: CurrentScreen) -> color_eyre::Result<()> {
        let representative_patch: Patch;
        let mut is_patchset_bookmarked = true;
        

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

        let patchset_path: String = match lore_session::download_patchset(&self.config.patchsets_cache_dir, &representative_patch) {
            Ok(result) => result,
            Err(io_error) => bail!("{io_error}"),
        };

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
                            (PatchsetAction::ReplyWithReviewedBy, false),
                        ]),
                        last_screen: current_screen,
                    }
                );
                Ok(())
            },
            Err(message) => bail!(message),
        }
    }

    pub fn reset_patchset_details_and_actions_state(&mut self) {
        self.patchset_details_and_actions_state = None;
    }

    pub fn consolidate_patchset_actions(&mut self) -> color_eyre::Result<()> {
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

        lore_session::save_bookmarked_patchsets(
            &self.bookmarked_patchsets_state.bookmarked_patchsets, &self.config.bookmarked_patchsets_path
        )?;

        let should_reply_with_reviewed_by = *self
            .patchset_details_and_actions_state.as_ref().unwrap()
            .patchset_actions.get(&PatchsetAction::ReplyWithReviewedBy).unwrap();
        if should_reply_with_reviewed_by {
            let successful_indexes = self.patchset_details_and_actions_state
                .as_ref()
                .unwrap()
                .reply_patchset_with_reviewed_by("all")?;

            if !successful_indexes.is_empty() {
                self.reviewed_patchsets.insert(
                    representative_patch.get_message_id().href.clone(),
                    successful_indexes,
                );

                lore_session::save_reviewed_patchsets(
                    &self.reviewed_patchsets,
                    &self.config.reviewed_patchsets_path
                )?;
            }

            self.patchset_details_and_actions_state
                .as_mut()
                .unwrap()
                .toggle_action(PatchsetAction::ReplyWithReviewedBy);
        }
        
        Ok(())
    }

    pub fn set_current_screen(&mut self, new_current_screen: CurrentScreen) {
        self.current_screen = new_current_screen;
    }
}
