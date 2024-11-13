use crate::{log_on_error, ui::popup::PopUp};
use ansi_to_tui::IntoText;
use color_eyre::eyre::bail;
use config::Config;
use logging::Logger;
use patch_hub::lore::{lore_api_client::BlockingLoreAPIClient, lore_session, patch::Patch};
use patch_renderer::{render_patch_preview, PatchRenderer};
use ratatui::text::Text;
use screens::{
    bookmarked::BookmarkedPatchsetsState,
    details_actions::{PatchsetAction, PatchsetDetailsAndActionsState},
    edit_config::EditConfigState,
    latest::LatestPatchsetsState,
    mail_list::MailingListSelectionState,
    CurrentScreen,
};
use std::collections::HashMap;

use crate::utils;

mod config;
pub mod logging;
pub mod patch_renderer;
pub mod screens;

pub struct App {
    pub current_screen: CurrentScreen,
    pub mailing_list_selection_state: MailingListSelectionState,
    pub bookmarked_patchsets_state: BookmarkedPatchsetsState,
    pub latest_patchsets_state: Option<LatestPatchsetsState>,
    pub patchset_details_and_actions_state: Option<PatchsetDetailsAndActionsState>,
    pub edit_config_state: Option<EditConfigState>,
    pub reviewed_patchsets: HashMap<String, Vec<usize>>,
    pub config: Config,
    pub lore_api_client: BlockingLoreAPIClient,
    pub popup: Option<Box<dyn PopUp>>,
}

impl App {
    pub fn new() -> App {
        let config: Config = Config::build();
        config.create_dirs();

        let mailing_lists =
            lore_session::load_available_lists(config.mailing_lists_path()).unwrap_or_default();

        let bookmarked_patchsets =
            lore_session::load_bookmarked_patchsets(config.bookmarked_patchsets_path())
                .unwrap_or_default();

        let reviewed_patchsets =
            lore_session::load_reviewed_patchsets(config.reviewed_patchsets_path())
                .unwrap_or_default();

        let lore_api_client = BlockingLoreAPIClient::default();

        // Initialize the logger before the app starts
        Logger::init_log_file(&config);
        Logger::info("patch-hub started");
        logging::garbage_collector::collect_garbage(&config);

        App {
            current_screen: CurrentScreen::MailingListSelection,
            mailing_list_selection_state: MailingListSelectionState {
                mailing_lists: mailing_lists.clone(),
                target_list: String::new(),
                possible_mailing_lists: mailing_lists,
                highlighted_list_index: 0,
                mailing_lists_path: config.mailing_lists_path().to_string(),
                lore_api_client: lore_api_client.clone(),
            },
            latest_patchsets_state: None,
            patchset_details_and_actions_state: None,
            edit_config_state: None,
            bookmarked_patchsets_state: BookmarkedPatchsetsState {
                bookmarked_patchsets,
                patchset_index: 0,
            },
            reviewed_patchsets,
            config,
            lore_api_client,
            popup: None,
        }
    }

    pub fn init_latest_patchsets_state(&mut self) {
        // the target mailing list for "latest patchsets" is the highlighted
        // entry in the possible lists of "mailing list selection"
        let list_index = self.mailing_list_selection_state.highlighted_list_index;
        let target_list = self.mailing_list_selection_state.possible_mailing_lists[list_index]
            .name()
            .to_string();
        self.latest_patchsets_state = Some(LatestPatchsetsState::new(
            target_list,
            self.config.page_size(),
            self.lore_api_client.clone(),
        ));
    }

    pub fn reset_latest_patchsets_state(&mut self) {
        self.latest_patchsets_state = None;
    }

    pub fn init_patchset_details_and_actions_state(
        &mut self,
        current_screen: CurrentScreen,
    ) -> color_eyre::Result<()> {
        let representative_patch: Patch;
        let mut is_patchset_bookmarked = true;

        match current_screen {
            CurrentScreen::BookmarkedPatchsets => {
                representative_patch = self.bookmarked_patchsets_state.get_selected_patchset();
            }
            CurrentScreen::LatestPatchsets => {
                representative_patch = self
                    .latest_patchsets_state
                    .as_ref()
                    .unwrap()
                    .get_selected_patchset();
                if !self
                    .bookmarked_patchsets_state
                    .bookmarked_patchsets
                    .contains(&representative_patch)
                {
                    is_patchset_bookmarked = false;
                }
            }
            screen => bail!(format!("Invalid screen passed as argument {screen:?}")),
        };

        let patchset_path: String = match log_on_error!(lore_session::download_patchset(
            self.config.patchsets_cache_dir(),
            &representative_patch,
        )) {
            Ok(result) => result,
            Err(io_error) => bail!("{io_error}"),
        };

        match log_on_error!(lore_session::split_patchset(&patchset_path)) {
            Ok(raw_patches) => {
                let mut patches_preview: Vec<Text> = Vec::new();
                for raw_patch in &raw_patches {
                    let raw_patch = raw_patch.replace('\t', "        ");
                    let patch_preview =
                        match render_patch_preview(&raw_patch, self.config.patch_renderer()) {
                            Ok(render) => render,
                            Err(_) => {
                                Logger::error(
                                    "Failed to render patch preview with external program",
                                );
                                raw_patch
                            }
                        }
                        .into_text()?;
                    patches_preview.push(patch_preview);
                }
                self.patchset_details_and_actions_state = Some(PatchsetDetailsAndActionsState {
                    representative_patch,
                    raw_patches,
                    patches_preview,
                    preview_index: 0,
                    preview_scroll_offset: 0,
                    preview_pan: 0,
                    preview_fullscreen: false,
                    patchset_actions: HashMap::from([
                        (PatchsetAction::Bookmark, is_patchset_bookmarked),
                        (PatchsetAction::ReplyWithReviewedBy, false),
                    ]),
                    last_screen: current_screen,
                    lore_api_client: self.lore_api_client.clone(),
                });
                Ok(())
            }
            Err(message) => bail!(message),
        }
    }

    pub fn reset_patchset_details_and_actions_state(&mut self) {
        self.patchset_details_and_actions_state = None;
    }

    pub fn consolidate_patchset_actions(&mut self) -> color_eyre::Result<()> {
        let representative_patch = &self
            .patchset_details_and_actions_state
            .as_ref()
            .unwrap()
            .representative_patch;

        let should_bookmark_patchset = *self
            .patchset_details_and_actions_state
            .as_ref()
            .unwrap()
            .patchset_actions
            .get(&PatchsetAction::Bookmark)
            .unwrap();
        if should_bookmark_patchset {
            self.bookmarked_patchsets_state
                .bookmark_selected_patch(representative_patch);
        } else {
            self.bookmarked_patchsets_state
                .unbookmark_selected_patch(representative_patch);
        }

        lore_session::save_bookmarked_patchsets(
            &self.bookmarked_patchsets_state.bookmarked_patchsets,
            self.config.bookmarked_patchsets_path(),
        )?;

        let should_reply_with_reviewed_by = *self
            .patchset_details_and_actions_state
            .as_ref()
            .unwrap()
            .patchset_actions
            .get(&PatchsetAction::ReplyWithReviewedBy)
            .unwrap();
        if should_reply_with_reviewed_by {
            let successful_indexes = self
                .patchset_details_and_actions_state
                .as_ref()
                .unwrap()
                .reply_patchset_with_reviewed_by("all", self.config.git_send_email_options())?;

            if !successful_indexes.is_empty() {
                self.reviewed_patchsets.insert(
                    representative_patch.message_id().href.clone(),
                    successful_indexes,
                );

                lore_session::save_reviewed_patchsets(
                    &self.reviewed_patchsets,
                    self.config.reviewed_patchsets_path(),
                )?;
            }

            self.patchset_details_and_actions_state
                .as_mut()
                .unwrap()
                .toggle_action(PatchsetAction::ReplyWithReviewedBy);
        }

        Ok(())
    }

    pub fn init_edit_config_state(&mut self) {
        self.edit_config_state = Some(EditConfigState::new(&self.config));
    }

    pub fn reset_edit_config_state(&mut self) {
        self.edit_config_state = None;
    }

    pub fn consolidate_edit_config(&mut self) {
        // TODO: Handle invalid values!
        if let Some(edit_config) = &mut self.edit_config_state {
            if let Ok(page_size) = edit_config.page_size() {
                self.config.set_page_size(page_size)
            }
            if let Ok(cache_dir) = edit_config.cache_dir() {
                self.config.set_cache_dir(cache_dir)
            }
            if let Ok(data_dir) = edit_config.data_dir() {
                self.config.set_data_dir(data_dir)
            }
            if let Ok(git_send_email_option) = edit_config.git_send_email_option() {
                self.config.set_git_send_email_option(git_send_email_option)
            }
            if let Ok(patch_renderer) = edit_config.extract_patch_renderer() {
                self.config.set_patch_renderer(patch_renderer.into())
            }
            if let Ok(max_log_age) = edit_config.max_log_age() {
                self.config.set_max_log_age(max_log_age)
            }
        }
    }

    pub fn set_current_screen(&mut self, new_current_screen: CurrentScreen) {
        self.current_screen = new_current_screen;
    }

    /// Check if the external dependencies are installed
    ///
    /// If soft dependencies are missing, the application can still run and
    /// their absence will only be logged
    pub fn check_external_deps(&self) -> bool {
        let mut app_can_run = true;

        if !utils::binary_exists("b4") {
            Logger::error("b4 is not installed, patchsets cannot be downloaded");
            app_can_run = false;
        }

        if !utils::binary_exists("git") {
            Logger::warn("git is not installed, send-email won't work");
        }

        match self.config.patch_renderer() {
            PatchRenderer::Bat => {
                if !utils::binary_exists("bat") {
                    Logger::warn("bat is not installed, patch rendering will fallback to default");
                }
            }
            PatchRenderer::Delta => {
                if !utils::binary_exists("delta") {
                    Logger::warn(
                        "delta is not installed, patch rendering will fallback to default",
                    );
                }
            }
            PatchRenderer::DiffSoFancy => {
                if !utils::binary_exists("diff-so-fancy") {
                    Logger::warn(
                        "diff-so-fancy is not installed, patch rendering will fallback to default",
                    );
                }
            }
            _ => {}
        }

        app_can_run
    }
}
