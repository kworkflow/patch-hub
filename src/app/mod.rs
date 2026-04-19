pub mod config;
pub mod cover_renderer;
pub mod errors;
pub mod patch_renderer;
pub mod screens;
pub mod state;

use ansi_to_tui::IntoText;
use color_eyre::eyre::{bail, eyre};
use ratatui::text::Text;
use tracing::{event, Level};

use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

use crate::{
    infrastructure::{
        env::EnvTrait,
        file_system::FileSystemTrait,
        monitoring::logging::garbage_collector::collect_garbage,
        render::RenderServiceApi,
        shell::{ShellCommand, ShellTrait},
    },
    lore::{
        application::{api::LoreServiceApi, cache::CacheMode, errors::LoreError},
        domain::patch::{Author, Patch},
    },
    ui::popup::info_popup::InfoPopUp,
};

use config::Config;
use screens::{
    bookmarked::BookmarkedPatchsetsState,
    details_actions::{PatchsetAction, PatchsetDetailsState},
    edit_config::EditConfigState,
    latest::LatestPatchsetsState,
    mail_list::MailingListSelectionState,
    CurrentScreen,
};
pub use state::{AppState, ConfigUiState, LoreUiState, NavigationState, UserLoreState};

/// Injected capabilities used by `App` orchestration (not screen state).
pub struct AppServices {
    pub lore: Box<dyn LoreServiceApi>,
    pub render: Box<dyn RenderServiceApi>,
    pub shell: Box<dyn ShellTrait>,
    pub fs: Box<dyn FileSystemTrait>,
    pub env: Box<dyn EnvTrait>,
}

/// Result type signalling whether a patchset was successfully loaded.
pub enum B4Result {
    PatchFound,
    PatchNotFound(String),
}

/// Central orchestrator: holds explicit [`AppState`] and [`AppServices`].
pub struct App {
    pub state: AppState,
    pub services: AppServices,
}

impl App {
    /// Creates a new instance of `App`. It dynamically loads configurations
    /// based on precedence (see [crate::app::Config::build]), app data
    /// (available mailing lists, bookmarked patchsets, reviewed patchsets)
    ///
    /// # Returns
    ///
    /// `App` instance with loading configurations and app data.
    pub fn new(
        config: Config,
        fs: Box<dyn FileSystemTrait>,
        shell: Box<dyn ShellTrait>,
        env: Box<dyn EnvTrait>,
        mut lore_service: Box<dyn LoreServiceApi>,
        render: Box<dyn RenderServiceApi>,
    ) -> color_eyre::Result<Self> {
        let bootstrap = lore_service.warm_bootstrap_cache().unwrap_or_default();

        event!(Level::INFO, "patch-hub started");
        collect_garbage(&config);

        Ok(App {
            state: AppState {
                navigation: NavigationState {
                    current_screen: CurrentScreen::MailingListSelection,
                },
                lore: LoreUiState {
                    mailing_list_selection: MailingListSelectionState {
                        mailing_lists: bootstrap.mailing_lists.clone(),
                        target_list: String::new(),
                        possible_mailing_lists: bootstrap.mailing_lists,
                        highlighted_list_index: 0,
                    },
                    latest_patchsets: None,
                    details: None,
                },
                user_state: UserLoreState {
                    bookmarked_patchsets: BookmarkedPatchsetsState {
                        bookmarked_patchsets: bootstrap.bookmarks,
                        patchset_index: 0,
                    },
                    reviewed_patchsets: bootstrap.reviewed,
                },
                config_state: ConfigUiState { edit_config: None },
                config,
                popup: None,
            },
            services: AppServices {
                lore: lore_service,
                render,
                shell,
                fs,
                env,
            },
        })
    }

    /// Initializes [`LoreUiState::latest_patchsets`] from the currently selected
    /// mailing list.
    pub fn init_latest_patchsets(&mut self) {
        let list_index = self
            .state
            .lore
            .mailing_list_selection
            .highlighted_list_index;
        let target_list = self
            .state
            .lore
            .mailing_list_selection
            .possible_mailing_lists[list_index]
            .name()
            .to_string();
        self.state.lore.latest_patchsets = Some(LatestPatchsetsState::new(
            target_list,
            self.state.config.page_size(),
        ));
    }

    /// Sets [`LoreUiState::latest_patchsets`] to `None`.
    pub fn reset_latest_patchsets(&mut self) {
        self.state.lore.latest_patchsets = None;
    }

    /// Fetches (or re-fetches) the current page of latest patchsets from Lore.
    pub fn fetch_latest_current_page(&mut self) -> color_eyre::Result<()> {
        let lore = self.services.lore.as_mut();
        let latest_patchsets = &mut self.state.lore.latest_patchsets;
        if let Some(patchsets) = latest_patchsets.as_mut() {
            patchsets.fetch_current_page(lore, CacheMode::UseCache)
        } else {
            Ok(())
        }
    }

    /// Refreshes available mailing lists and updates [`LoreUiState::mailing_list_selection`].
    pub fn refresh_mailing_lists(&mut self) -> color_eyre::Result<()> {
        self.state
            .lore
            .mailing_list_selection
            .refresh_available_mailing_lists(self.services.lore.as_mut(), CacheMode::Refresh)
    }

    /// Loads patchset details into [`LoreUiState::details`].
    pub fn init_details_actions(&mut self) -> color_eyre::Result<B4Result> {
        let representative_patch: Patch;
        let mut is_patchset_bookmarked = true;

        match &self.state.navigation.current_screen {
            CurrentScreen::BookmarkedPatchsets => {
                representative_patch = self
                    .state
                    .user_state
                    .bookmarked_patchsets
                    .get_selected_patchset();
            }
            CurrentScreen::LatestPatchsets => {
                representative_patch = self
                    .state
                    .lore
                    .latest_patchsets
                    .as_ref()
                    .unwrap()
                    .get_selected_patchset();
                if !self
                    .state
                    .user_state
                    .bookmarked_patchsets
                    .bookmarked_patchsets
                    .contains(&representative_patch)
                {
                    is_patchset_bookmarked = false;
                }
            }
            screen => bail!(format!("Invalid screen passed as argument {screen:?}")),
        };

        let details = match self
            .services
            .lore
            .fetch_patchset_details(&representative_patch, CacheMode::UseCache)
        {
            Ok(d) => d,
            Err(LoreError::PatchNotFound(err)) => return Ok(B4Result::PatchNotFound(err)),
            Err(e) => bail!("{e:#?}"),
        };

        let preview_lines = self
            .services
            .render
            .render_patchset_preview(
                &details.raw_patches,
                self.state.config.patch_renderer(),
                self.state.config.cover_renderer(),
            )
            .map_err(|e| eyre!("{e}"))?;

        let mut patches_preview: Vec<Text> = Vec::new();
        let mut reviewed_by: Vec<HashSet<Author>> = Vec::new();
        let mut tested_by: Vec<HashSet<Author>> = Vec::new();
        let mut acked_by: Vec<HashSet<Author>> = Vec::new();

        for (line, tag_summary) in preview_lines.iter().zip(details.tag_summary.iter()) {
            reviewed_by.push(tag_summary.reviewed_by.clone());
            tested_by.push(tag_summary.tested_by.clone());
            acked_by.push(tag_summary.acked_by.clone());
            patches_preview.push(line.as_str().into_text()?);
        }

        let has_cover_letter = representative_patch.number_in_series() == 0;
        let patches_to_reply = vec![false; details.raw_patches.len()];

        self.state.lore.details = Some(PatchsetDetailsState {
            representative_patch,
            raw_patches: details.raw_patches,
            patchset_path: details.patchset_path,
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
                (PatchsetAction::Apply, false),
            ]),
            reviewed_by,
            tested_by,
            acked_by,
            last_screen: self.state.navigation.current_screen.clone(),
        });

        Ok(B4Result::PatchFound)
    }

    /// Clears [`LoreUiState::details`].
    pub fn reset_details_actions(&mut self) {
        self.state.lore.details = None;
    }

    /// Consolidates patchset actions from the details screen.
    ///
    /// # Panics
    ///
    /// Panics if [`LoreUiState::details`] is `None`.
    pub fn consolidate_patchset_actions(&mut self) -> color_eyre::Result<()> {
        let details = self.state.lore.details.as_ref().unwrap();
        let representative_patch = details.representative_patch.clone();
        let patchset_actions = details.patchset_actions.clone();
        let raw_patches = details.raw_patches.clone();
        let patches_to_reply = details.patches_to_reply.clone();

        if let Some(true) = patchset_actions.get(&PatchsetAction::Bookmark) {
            self.state
                .user_state
                .bookmarked_patchsets
                .bookmark_selected_patch(&representative_patch);
        } else {
            self.state
                .user_state
                .bookmarked_patchsets
                .unbookmark_selected_patch(&representative_patch);
        }

        self.services
            .lore
            .save_bookmarked_patchsets(
                &self
                    .state
                    .user_state
                    .bookmarked_patchsets
                    .bookmarked_patchsets,
            )
            .map_err(|e| eyre!("{e:#?}"))?;

        if let Some(true) = patchset_actions.get(&PatchsetAction::ReplyWithReviewedBy) {
            let mut successful_indexes = self
                .state
                .user_state
                .reviewed_patchsets
                .remove(&representative_patch.message_id().href)
                .unwrap_or_default();

            let (git_user_name, git_user_email) = self.services.lore.get_git_signature("");

            if git_user_name.is_empty() || git_user_email.is_empty() {
                println!("`git config user.name` or `git config user.email` not set\nAborting...");
            } else {
                let mktemp_cmd = ShellCommand::new("mktemp").arg("--directory");
                let tmp_out = self
                    .services
                    .shell
                    .execute(&mktemp_cmd)
                    .map_err(|e| eyre!("failed to create temp directory: {}", e))?;
                let tmp_dir_str = std::str::from_utf8(&tmp_out.stdout)
                    .map_err(|e| eyre!("invalid utf-8 in temp dir path: {}", e))?
                    .trim()
                    .to_string();
                let tmp_dir = Path::new(&tmp_dir_str);

                let git_signature = format!("{git_user_name} <{git_user_email}>");
                let git_reply_commands = self
                    .services
                    .lore
                    .prepare_reply_commands(
                        tmp_dir,
                        "all",
                        &raw_patches,
                        &patches_to_reply,
                        &git_signature,
                        self.state.config.git_send_email_options(),
                    )
                    .map_err(|e| eyre!("{e:#?}"))?;

                let reply_indexes: Vec<usize> = patches_to_reply
                    .iter()
                    .enumerate()
                    .filter_map(|(i, &val)| if val { Some(i) } else { None })
                    .collect();
                for (i, command) in git_reply_commands.into_iter().enumerate() {
                    let success = self
                        .services
                        .shell
                        .spawn_interactive(&command)
                        .unwrap_or(false);
                    if success {
                        successful_indexes.insert(reply_indexes[i]);
                    }
                }
            }

            self.state.user_state.reviewed_patchsets.insert(
                representative_patch.message_id().href.clone(),
                successful_indexes,
            );

            self.services
                .lore
                .save_reviewed_patchsets(&self.state.user_state.reviewed_patchsets)
                .map_err(|e| eyre!("{e:#?}"))?;

            self.state
                .lore
                .details
                .as_mut()
                .unwrap()
                .reset_reply_with_reviewed_by_action();
        }

        if let Some(true) = self
            .state
            .lore
            .details
            .as_ref()
            .unwrap()
            .patchset_actions
            .get(&PatchsetAction::Apply)
        {
            let popup = match self.state.lore.details.as_ref().unwrap().apply_patchset(
                &*self.services.fs,
                &*self.services.shell,
                &self.state.config,
            ) {
                Ok(msg) => InfoPopUp::generate_info_popup("Patchset Apply Success", &msg),
                Err(msg) => InfoPopUp::generate_info_popup("Patchset Apply Fail", &msg),
            };

            self.state.popup = Some(popup);

            self.state
                .lore
                .details
                .as_mut()
                .unwrap()
                .toggle_apply_action();
        }

        Ok(())
    }

    /// Opens the edit-config screen from current [`Config`].
    pub fn init_edit_config(&mut self) {
        self.state.config_state.edit_config = Some(EditConfigState::new(&self.state.config));
    }

    pub fn reset_edit_config(&mut self) {
        self.state.config_state.edit_config = None;
    }

    /// Applies edited values from [`ConfigUiState::edit_config`] into [`AppState::config`].
    pub fn consolidate_edit_config(&mut self) {
        // TODO: Handle invalid values!
        if let Some(edit_config) = &mut self.state.config_state.edit_config {
            if let Ok(page_size) = edit_config.page_size() {
                self.state.config.set_page_size(page_size)
            }
            if let Ok(cache_dir) = edit_config.cache_dir(&*self.services.fs) {
                self.state.config.set_cache_dir(cache_dir)
            }
            if let Ok(data_dir) = edit_config.data_dir(&*self.services.fs) {
                self.state.config.set_data_dir(data_dir)
            }
            if let Ok(git_send_email_option) = edit_config.git_send_email_option() {
                self.state
                    .config
                    .set_git_send_email_option(git_send_email_option)
            }
            if let Ok(git_am_option) = edit_config.git_am_option() {
                self.state.config.set_git_am_option(git_am_option)
            }
            if let Ok(patch_renderer) = edit_config.extract_patch_renderer() {
                self.state.config.set_patch_renderer(patch_renderer.into())
            }
            if let Ok(cover_renderer) = edit_config.extract_cover_renderer() {
                self.state.config.set_cover_renderer(cover_renderer.into())
            }
            if let Ok(max_log_age) = edit_config.max_log_age() {
                self.state.config.set_max_log_age(max_log_age)
            }
        }
    }

    pub fn set_current_screen(&mut self, new_current_screen: CurrentScreen) {
        self.state.navigation.current_screen = new_current_screen;
    }
}
