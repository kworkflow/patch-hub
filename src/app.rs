use crate::{log_on_error, ui::popup::PopUp};
use ansi_to_tui::IntoText;
use color_eyre::eyre::bail;
use config::Config;
use cover_renderer::render_cover;
use logging::Logger;
use patch_hub::lore::{
    lore_api_client::BlockingLoreAPIClient,
    lore_session,
    patch::{Author, Patch},
};
use patch_renderer::{render_patch_preview, PatchRenderer};
use ratatui::text::Text;
use screens::{
    bookmarked::BookmarkedPatchsets,
    details_actions::{DetailsActions, PatchsetAction},
    edit_config::EditConfig,
    latest::LatestPatchsets,
    mail_list::MailingListSelection,
    CurrentScreen,
};
use std::collections::{HashMap, HashSet};

use crate::utils;

mod config;
pub mod cover_renderer;
pub mod logging;
pub mod patch_renderer;
pub mod screens;

/// Type that represents the overall state of the application. It can be viewed
/// as the **Model** component of `patch-hub`.
pub struct App {
    /// The current active screen
    pub current_screen: CurrentScreen,
    /// Screen to navigate and select the mailing lists archived on Lore
    pub mailing_list_selection: MailingListSelection,
    /// Screen with listing patchsets that were previously bookmarked
    pub bookmarked_patchsets: BookmarkedPatchsets,
    /// Screen with paginated listing of latest patchsets from a target list
    pub latest_patchsets: Option<LatestPatchsets>,
    /// Screen with details (metadata and previewing) and runnable actions of individual patchset
    pub details_actions: Option<DetailsActions>,
    /// Screen to edit configurations of the app
    pub edit_config: Option<EditConfig>,
    /// Database to track patchsets `Reviewed-by` state
    pub reviewed_patchsets: HashMap<String, HashSet<usize>>,
    /// Configurations of the app
    pub config: Config,
    /// Client to handle Lore API requests and responses
    pub lore_api_client: BlockingLoreAPIClient,
    pub popup: Option<Box<dyn PopUp>>,
}

impl App {
    /// Creates a new instance of `App`. It dynamically loads configurations
    /// based on precedence (see [crate::app::Config::build]), app data
    /// (available mailing lists, bookmarked patchsets, reviewed patchsets), and
    /// initializes the Logger (see [crate::app::logging::Logger])
    ///
    /// # Returns
    ///
    /// `App` instance with loading configurations and app data.
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
            mailing_list_selection: MailingListSelection {
                mailing_lists: mailing_lists.clone(),
                target_list: String::new(),
                possible_mailing_lists: mailing_lists,
                highlighted_list_index: 0,
                mailing_lists_path: config.mailing_lists_path().to_string(),
                lore_api_client: lore_api_client.clone(),
            },
            latest_patchsets: None,
            details_actions: None,
            edit_config: None,
            bookmarked_patchsets: BookmarkedPatchsets {
                bookmarked_patchsets,
                patchset_index: 0,
            },
            reviewed_patchsets,
            config,
            lore_api_client,
            popup: None,
        }
    }

    /// Initializes field [App::latest_patchsets], from currently selected
    /// mailing list in [App::mailing_list_selection].
    pub fn init_latest_patchsets(&mut self) {
        // the target mailing list for "latest patchsets" is the highlighted
        // entry in the possible lists of "mailing list selection"
        let list_index = self.mailing_list_selection.highlighted_list_index;
        let target_list = self.mailing_list_selection.possible_mailing_lists[list_index]
            .name()
            .to_string();
        self.latest_patchsets = Some(LatestPatchsets::new(
            target_list,
            self.config.page_size(),
            self.lore_api_client.clone(),
        ));
    }

    /// Sets field [App::latest_patchsets] to `None`.
    pub fn reset_latest_patchsets(&mut self) {
        self.latest_patchsets = None;
    }

    /// Initializes field [App::details_actions], from currently selected
    /// patchset in [App::bookmarked_patchsets] or [App::latest_patchsets],
    /// depending on the value of [App::current_screen].
    pub fn init_details_actions(&mut self) -> color_eyre::Result<()> {
        let representative_patch: Patch;
        let mut is_patchset_bookmarked = true;
        let mut reviewed_by = Vec::new();
        let mut tested_by = Vec::new();
        let mut acked_by = Vec::new();

        match &self.current_screen {
            CurrentScreen::BookmarkedPatchsets => {
                representative_patch = self.bookmarked_patchsets.get_selected_patchset();
            }
            CurrentScreen::LatestPatchsets => {
                representative_patch = self
                    .latest_patchsets
                    .as_ref()
                    .unwrap()
                    .get_selected_patchset();
                if !self
                    .bookmarked_patchsets
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

                    let (raw_cover, raw_patch) = lore_session::split_cover(&raw_patch);

                    let mut authors_reviewed_by = HashSet::new();
                    let mut authors_tested_by = HashSet::new();
                    let mut authors_acked_by = HashSet::new();

                    let mut map = [
                        ("Reviewed-by:", &mut authors_reviewed_by),
                        ("Tested-by:", &mut authors_tested_by),
                        ("Acked-by:", &mut authors_acked_by),
                    ];

                    for line in raw_cover.lines() {
                        for (prefix, authors) in map.iter_mut() {
                            if let Some(stripped) = line.trim_start().strip_prefix(*prefix) {
                                let parts: Vec<&str> = stripped.trim().split('<').collect();
                                if parts.len() == 2 {
                                    let name = parts[0].trim().to_string();
                                    let email = parts[1].trim_end_matches('>').trim().to_string();
                                    authors.insert(Author { name, email });
                                }
                                break; // Avoid unnecessary checks once a match is found
                            }
                        }
                    }
                    reviewed_by.push(authors_reviewed_by);
                    tested_by.push(authors_tested_by);
                    acked_by.push(authors_acked_by);

                    let rendered_cover = match render_cover(raw_cover, self.config.cover_renderer())
                    {
                        Ok(render) => render,
                        Err(_) => {
                            Logger::error("Failed to render cover preview with external program");
                            raw_cover.to_string()
                        }
                    };

                    let rendered_patch =
                        match render_patch_preview(raw_patch, self.config.patch_renderer()) {
                            Ok(render) => render,
                            Err(_) => {
                                Logger::error(
                                    "Failed to render patch preview with external program",
                                );
                                raw_patch.to_string()
                            }
                        };

                    patches_preview
                        .push(format!("{}---\n{}", rendered_cover, rendered_patch).into_text()?);
                }
                let has_cover_letter = representative_patch.number_in_series() == 0;
                let patches_to_reply = vec![false; raw_patches.len()];
                self.details_actions = Some(DetailsActions {
                    representative_patch,
                    raw_patches,
                    patches_preview,
                    patches_to_reply,
                    has_cover_letter,
                    preview_index: 0,
                    preview_scroll_offset: 0,
                    preview_pan: 0,
                    preview_fullscreen: false,
                    patchset_actions: HashMap::from([
                        (PatchsetAction::Bookmark, is_patchset_bookmarked),
                        (PatchsetAction::ReplyWithReviewedBy, false),
                    ]),
                    reviewed_by,
                    tested_by,
                    acked_by,
                    last_screen: self.current_screen.clone(),
                    lore_api_client: self.lore_api_client.clone(),
                    patchset_path,
                });
                Ok(())
            }
            Err(message) => bail!(message),
        }
    }

    /// Sets field [App::details_actions] to `None`.
    pub fn reset_details_actions(&mut self) {
        self.details_actions = None;
    }

    /// Determines and consolidates all actions (if any) to take for the current
    /// patchset stored in `details_actions`.
    ///
    /// # Panics
    ///
    /// This function will panic if `details_actions` is
    /// `None`.
    pub fn consolidate_patchset_actions(&mut self) -> color_eyre::Result<()> {
        let details_actions = self.details_actions.as_ref().unwrap();
        let representative_patch = &details_actions.representative_patch;
        let actions = &details_actions.patchset_actions;

        if let Some(true) = actions.get(&PatchsetAction::Bookmark) {
            self.bookmarked_patchsets
                .bookmark_selected_patch(representative_patch);
        } else {
            self.bookmarked_patchsets
                .unbookmark_selected_patch(representative_patch);
        }

        lore_session::save_bookmarked_patchsets(
            &self.bookmarked_patchsets.bookmarked_patchsets,
            self.config.bookmarked_patchsets_path(),
        )?;

        if let Some(true) = actions.get(&PatchsetAction::ReplyWithReviewedBy) {
            let mut successful_indexes = self
                .reviewed_patchsets
                .remove(&representative_patch.message_id().href)
                .unwrap_or_default();
            details_actions.reply_patchset_with_reviewed_by(
                "all",
                self.config.git_send_email_options(),
                &mut successful_indexes,
            )?;
            self.reviewed_patchsets.insert(
                representative_patch.message_id().href.clone(),
                successful_indexes,
            );

            lore_session::save_reviewed_patchsets(
                &self.reviewed_patchsets,
                self.config.reviewed_patchsets_path(),
            )?;

            self.details_actions
                .as_mut()
                .unwrap()
                .reset_reply_with_reviewed_by_action();
        }

        Ok(())
    }

    /// Initializes field [App::edit_config], using values from [App::config].
    pub fn init_edit_config(&mut self) {
        self.edit_config = Some(EditConfig::new(&self.config));
    }

    /// Sets field [App::edit_config] to `None`.
    pub fn reset_edit_config(&mut self) {
        self.edit_config = None;
    }

    /// Based on the edited config values from [App::edit_config], commit them
    /// to field [App::config].
    pub fn consolidate_edit_config(&mut self) {
        // TODO: Handle invalid values!
        if let Some(edit_config) = &mut self.edit_config {
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
            if let Ok(git_am_option) = edit_config.git_am_option() {
                self.config.set_git_am_option(git_am_option)
            }
            if let Ok(patch_renderer) = edit_config.extract_patch_renderer() {
                self.config.set_patch_renderer(patch_renderer.into())
            }
            if let Ok(cover_renderer) = edit_config.extract_cover_renderer() {
                self.config.set_cover_renderer(cover_renderer.into())
            }
            if let Ok(max_log_age) = edit_config.max_log_age() {
                self.config.set_max_log_age(max_log_age)
            }
        }
    }

    /// Change the current active screen in [App::current_screen].
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
