pub mod commands;
pub mod cover_renderer;
pub mod errors;
pub mod patch_renderer;
pub mod screens;
pub mod state;
pub mod view_model;

use color_eyre::eyre::{bail, eyre};
use tracing::{event, Level};

use std::path::PathBuf;

use crate::{
    config::ConfigServiceApi,
    infrastructure::{
        env::EnvTrait,
        file_system::FileSystemTrait,
        monitoring::logging::garbage_collector::collect_garbage,
        shell::{ShellCommand, ShellTrait},
    },
    lore::{
        application::{
            cache::{BootstrapLoreData, CacheMode},
            errors::LoreError,
            handle::LoreApiHandle,
        },
        domain::patch::Patch,
    },
    render::{handle::RenderHandle, RenderPatchsetRequest},
    ui::popup::info_popup::InfoPopUp,
};
use screens::{
    bookmarked::BookmarkedPatchsetsState,
    details_actions::{PatchsetAction, PatchsetDetailsState},
    edit_config::EditConfigState,
    latest::LatestPatchsetsState,
    mail_list::MailingListSelectionState,
    CurrentScreen,
};
pub use state::{AppState, ConfigUiState, LoreUiState, NavigationState, UserLoreState};
pub use view_model::AppViewModel;

/// Injected capabilities used by `App` orchestration (not screen state).
pub struct AppServices {
    pub lore_api: LoreApiHandle,
    pub render: RenderHandle,
    pub shell: Box<dyn ShellTrait>,
    pub fs: Box<dyn FileSystemTrait>,
    pub env: Box<dyn EnvTrait>,
    pub config: Box<dyn ConfigServiceApi>,
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
    /// Creates a new instance of `App`. Configuration comes from [`ConfigServiceApi::snapshot`];
    /// Lore bootstrap uses already-warmed cache from `lore_service`.
    ///
    /// # Returns
    ///
    /// `App` instance with loading configurations and app data.
    pub fn new(
        config_service: Box<dyn ConfigServiceApi>,
        bootstrap: BootstrapLoreData,
        fs: Box<dyn FileSystemTrait>,
        shell: Box<dyn ShellTrait>,
        env: Box<dyn EnvTrait>,
        lore_api: LoreApiHandle,
        render: RenderHandle,
    ) -> color_eyre::Result<Self> {
        let config = config_service.snapshot();

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
                lore_api,
                render,
                shell,
                fs,
                env,
                config: config_service,
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
    pub async fn fetch_latest_current_page(&mut self) -> color_eyre::Result<()> {
        let lore_api = &self.services.lore_api;
        let latest_patchsets = &mut self.state.lore.latest_patchsets;
        if let Some(patchsets) = latest_patchsets.as_mut() {
            patchsets
                .fetch_current_page(lore_api, CacheMode::UseCache)
                .await
        } else {
            Ok(())
        }
    }

    /// Refreshes available mailing lists and updates [`LoreUiState::mailing_list_selection`].
    pub async fn refresh_mailing_lists(&mut self) -> color_eyre::Result<()> {
        self.state
            .lore
            .mailing_list_selection
            .refresh_available_mailing_lists(&self.services.lore_api, CacheMode::Refresh)
            .await
    }

    /// Loads patchset details into [`LoreUiState::details`].
    pub async fn open_patchset_details(&mut self) -> color_eyre::Result<B4Result> {
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
            .lore_api
            .fetch_patchset_details(representative_patch.clone(), CacheMode::UseCache)
            .await
        {
            Ok(d) => d,
            Err(LoreError::PatchNotFound(err)) => return Ok(B4Result::PatchNotFound(err)),
            Err(e) => bail!("{e:#?}"),
        };

        let render_request = RenderPatchsetRequest::new(
            details.raw_patches.clone(),
            *self.state.config.patch_renderer(),
            *self.state.config.cover_renderer(),
        );
        let rendered_preview = self
            .services
            .render
            .render_patchset_preview(render_request)
            .await
            .map_err(|e| eyre!("{e}"))?;

        self.state.lore.details = Some(PatchsetDetailsState::from_rendered_preview(
            representative_patch,
            details,
            rendered_preview,
            is_patchset_bookmarked,
            self.state.navigation.current_screen.clone(),
        )?);

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
    pub async fn consolidate_patchset_actions(&mut self) -> color_eyre::Result<()> {
        self.sync_patchset_bookmark().await?;
        self.execute_reviewed_reply().await?;
        self.execute_apply_patchset();
        Ok(())
    }

    async fn sync_patchset_bookmark(&mut self) -> color_eyre::Result<()> {
        let details = self.state.lore.details.as_ref().unwrap();
        let representative_patch = &details.representative_patch;
        let patchset_actions = &details.patchset_actions;

        if let Some(true) = patchset_actions.get(&PatchsetAction::Bookmark) {
            self.state
                .user_state
                .bookmarked_patchsets
                .bookmark_selected_patch(representative_patch);
        } else {
            self.state
                .user_state
                .bookmarked_patchsets
                .unbookmark_selected_patch(representative_patch);
        }

        self.services
            .lore_api
            .save_bookmarks(
                self.state
                    .user_state
                    .bookmarked_patchsets
                    .bookmarked_patchsets
                    .clone(),
            )
            .await
            .map_err(|e| eyre!("{e:#?}"))?;
        Ok(())
    }

    async fn execute_reviewed_reply(&mut self) -> color_eyre::Result<()> {
        let details = self.state.lore.details.as_ref().unwrap();
        let representative_patch = details.representative_patch.clone();
        let patchset_actions = &details.patchset_actions;
        let raw_patches = details.raw_patches.clone();
        let patches_to_reply = details.patches_to_reply.clone();

        if let Some(true) = patchset_actions.get(&PatchsetAction::ReplyWithReviewedBy) {
            let mut successful_indexes = self
                .state
                .user_state
                .reviewed_patchsets
                .remove(&representative_patch.message_id().href)
                .unwrap_or_default();

            let (git_user_name, git_user_email) = self
                .services
                .lore_api
                .get_git_signature(String::new())
                .await
                .map_err(|e| eyre!("{e:#?}"))?;

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
                let tmp_dir = PathBuf::from(tmp_dir_str);

                let git_signature = format!("{git_user_name} <{git_user_email}>");
                let git_reply_commands = self
                    .services
                    .lore_api
                    .prepare_reply_commands(
                        tmp_dir,
                        "all".to_string(),
                        raw_patches,
                        patches_to_reply.clone(),
                        git_signature,
                        self.state.config.git_send_email_options().to_string(),
                    )
                    .await
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
                .lore_api
                .save_reviewed(self.state.user_state.reviewed_patchsets.clone())
                .await
                .map_err(|e| eyre!("{e:#?}"))?;

            self.state
                .lore
                .details
                .as_mut()
                .unwrap()
                .reset_reply_with_reviewed_by_action();
        }
        Ok(())
    }

    fn execute_apply_patchset(&mut self) {
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
    }

    /// Opens the edit-config screen from the current configuration snapshot.
    pub fn init_edit_config(&mut self) {
        self.state.config_state.edit_config = Some(EditConfigState::new(&self.state.config));
    }

    pub fn reset_edit_config(&mut self) {
        self.state.config_state.edit_config = None;
    }

    /// Applies edited values from [`ConfigUiState::edit_config`] into [`AppState::config`].
    pub fn consolidate_edit_config(&mut self) -> color_eyre::Result<()> {
        if let Some(edit_config) = &self.state.config_state.edit_config {
            let draft = edit_config.to_update_draft();
            let validated = self.services.config.validate_update(draft)?;
            let snapshot = self.services.config.apply_update(validated)?;
            self.state.config = snapshot;
        }
        Ok(())
    }

    pub fn set_current_screen(&mut self, new_current_screen: CurrentScreen) {
        self.state.navigation.current_screen = new_current_screen;
    }

    /// Borrows state for one UI frame without passing [`App`] into `ui/`.
    #[must_use]
    pub fn to_view_model(&self) -> AppViewModel<'_> {
        AppViewModel { state: &self.state }
    }
}
