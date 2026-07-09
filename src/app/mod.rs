//! Application orchestration: state, screen flows, view-model projection, and the
//! central [`AppActor`](crate::app::actor::AppActor) run loop.
//!
//! [`App`] holds [`AppState`] (navigation, lore UI state, user data, config
//! snapshot) and [`AppServices`] (typed handles to
//! [`LoreApiHandle`](crate::lore::application::handle::LoreApiHandle),
//! [`RenderHandle`](crate::render::handle::RenderHandle), plus injected
//! infrastructure traits). Screen-specific input is dispatched from
//! [`AppActor`](crate::app::actor::AppActor) into [`crate::app::flows`];
//! presentation data crosses the UI boundary only through [`AppViewModel`] via
//! [`App::present`].
pub(crate) mod actions;
pub mod actor;
pub(crate) mod dependencies;
pub mod errors;
pub(crate) mod flows;
pub mod handle;
pub mod input;
pub(crate) mod loading;
pub mod popup;
pub mod screens;
pub mod state;
pub mod updates;
pub mod view_model;

use color_eyre::{
    eyre::{bail, eyre},
    Result,
};
use tracing::{debug, event, info, warn, Level};

use crate::{
    app::actions::{
        apply::{apply_patchset, ApplyPatchsetRequest},
        reviewed_reply::{
            execute_reviewed_reply as execute_reviewed_reply_action, ReviewedReplyRequest,
        },
    },
    config::{ConfigHandle, ConfigSnapshot},
    infrastructure::{
        file_system::FileSystemTrait, monitoring::logging::garbage_collector::collect_garbage,
        shell::ShellTrait,
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
    pub config: ConfigHandle,
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
    /// Creates a new instance of `App`.
    ///
    /// Configuration starts from the already-bootstrapped snapshot owned by the
    /// Config actor. Lore bootstrap uses already-warmed cache from `lore_service`.
    ///
    /// # Returns
    ///
    /// `App` instance with loading configurations and app data.
    pub fn new(
        config: ConfigSnapshot,
        config_handle: ConfigHandle,
        bootstrap: BootstrapLoreData,
        fs: Box<dyn FileSystemTrait>,
        shell: Box<dyn ShellTrait>,
        lore_api: LoreApiHandle,
        render: RenderHandle,
    ) -> Result<Self> {
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
                config: config_handle,
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
    pub async fn fetch_latest_current_page(&mut self) -> Result<()> {
        let lore_api = &self.services.lore_api;
        let latest_patchsets = &mut self.state.lore.latest_patchsets;
        if let Some(patchsets) = latest_patchsets.as_mut() {
            let list = patchsets.target_list().to_string();
            let page = patchsets.page_number();
            debug!(list, page, "fetching latest patchsets page");
            let result = patchsets
                .fetch_current_page(lore_api, CacheMode::UseCache)
                .await;
            match &result {
                Ok(()) => debug!(list, page, "latest patchsets page fetched"),
                Err(e) => warn!(list, page, error = %e, "failed to fetch latest patchsets page"),
            }
            result
        } else {
            Ok(())
        }
    }

    /// Refreshes available mailing lists and updates [`LoreUiState::mailing_list_selection`].
    pub async fn refresh_mailing_lists(&mut self) -> Result<()> {
        debug!("refreshing mailing lists");
        let result = self
            .state
            .lore
            .mailing_list_selection
            .refresh_available_mailing_lists(&self.services.lore_api, CacheMode::Refresh)
            .await;
        match &result {
            Ok(()) => debug!("mailing lists refreshed"),
            Err(e) => warn!(error = %e, "failed to refresh mailing lists"),
        }
        result
    }

    /// Loads patchset details into [`LoreUiState::details`].
    pub async fn open_patchset_details(&mut self) -> Result<B4Result> {
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
                    .expect(
                        "invariant: latest_patchsets must be initialised before opening details",
                    )
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

        let msg_id = &representative_patch.message_id().href;
        debug!(msg_id, "fetching patchset details from LoreAPI");

        let details = match self
            .services
            .lore_api
            .fetch_patchset_details(representative_patch.clone(), CacheMode::UseCache)
            .await
        {
            Ok(d) => d,
            Err(LoreError::PatchNotFound(err)) => {
                warn!(msg_id, reason = err, "patchset not found");
                return Ok(B4Result::PatchNotFound(err));
            }
            Err(e) => bail!("{e:#?}"),
        };

        debug!(
            msg_id,
            patches = details.raw_patches.len(),
            "rendering patchset preview"
        );
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

        debug!(msg_id, "patchset details loaded");
        self.state.lore.details = Some(PatchsetDetailsState::from_rendered_preview(
            representative_patch,
            details,
            rendered_preview,
            is_patchset_bookmarked,
            self.state.navigation.current_screen.clone(),
        ));

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
    pub async fn consolidate_patchset_actions(&mut self) -> Result<()> {
        debug!("consolidating patchset actions");
        self.sync_patchset_bookmark().await?;
        self.execute_reviewed_reply().await?;
        self.execute_apply_patchset();
        debug!("patchset actions consolidated");
        Ok(())
    }

    async fn sync_patchset_bookmark(&mut self) -> Result<()> {
        let details = self
            .state
            .lore
            .details
            .as_ref()
            .expect("invariant: details must be loaded before consolidating patchset actions");
        let representative_patch = &details.representative_patch;
        let patchset_actions = &details.patchset_actions;
        let msg_id = &representative_patch.message_id().href;

        if let Some(true) = patchset_actions.get(&PatchsetAction::Bookmark) {
            debug!(msg_id, "bookmarking patchset");
            self.state
                .user_state
                .bookmarked_patchsets
                .bookmark_selected_patch(representative_patch);
        } else {
            debug!(msg_id, "unbookmarking patchset");
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
        debug!(msg_id, "bookmark state persisted");
        Ok(())
    }

    async fn execute_reviewed_reply(&mut self) -> Result<()> {
        let details = self
            .state
            .lore
            .details
            .as_ref()
            .expect("invariant: details must be loaded before executing reviewed reply");
        let representative_patch = details.representative_patch.clone();
        let patchset_actions = &details.patchset_actions;
        let raw_patches = details.raw_patches.clone();
        let patches_to_reply = details.patches_to_reply.clone();

        if let Some(true) = patchset_actions.get(&PatchsetAction::ReplyWithReviewedBy) {
            debug!(
                msg_id = representative_patch.message_id().href,
                "executing reviewed-by reply"
            );
            let successful_indexes = self
                .state
                .user_state
                .reviewed_patchsets
                .remove(&representative_patch.message_id().href)
                .unwrap_or_default();
            let request = ReviewedReplyRequest {
                raw_patches,
                patches_to_reply,
                successful_indexes,
                git_send_email_options: self.state.config.git_send_email_options().to_string(),
            };
            let result = execute_reviewed_reply_action(
                request,
                &self.services.lore_api,
                &*self.services.shell,
            )
            .await?;

            self.state.user_state.reviewed_patchsets.insert(
                representative_patch.message_id().href.clone(),
                result.into_successful_indexes(),
            );

            self.services
                .lore_api
                .save_reviewed(self.state.user_state.reviewed_patchsets.clone())
                .await
                .map_err(|e| eyre!("{e:#?}"))?;

            info!(
                msg_id = representative_patch.message_id().href,
                "reviewed-by reply sent and state persisted"
            );
            self.state
                .lore
                .details
                .as_mut()
                .expect("invariant: details must be loaded before resetting reply action")
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
            .expect("invariant: details must be loaded before executing apply patchset")
            .patchset_actions
            .get(&PatchsetAction::Apply)
        {
            debug!("applying patchset via git-am");
            let details = self
                .state
                .lore
                .details
                .as_ref()
                .expect("invariant: details must be loaded before applying patchset");
            let request = ApplyPatchsetRequest {
                patch_title: details.representative_patch.title().clone(),
                patchset_path: details.patchset_path.clone(),
            };
            let popup = match apply_patchset(
                &request,
                &*self.services.fs,
                &*self.services.shell,
                &self.state.config,
            ) {
                Ok(msg) => popup::AppPopup::info("Patchset Apply Success", msg),
                Err(msg) => popup::AppPopup::info("Patchset Apply Fail", msg),
            };

            self.state.popup = Some(popup);

            self.state
                .lore
                .details
                .as_mut()
                .expect("invariant: details must be loaded before toggling apply action")
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
    pub async fn consolidate_edit_config(&mut self) -> Result<()> {
        if let Some(edit_config) = &self.state.config_state.edit_config {
            debug!("validating and applying config update");
            let draft = edit_config.to_update_draft();
            let snapshot = self
                .services
                .config
                .validate_and_apply(draft)
                .await
                .map_err(|e| eyre!("{e:#?}"))?;
            self.state.config = snapshot;
            info!("configuration updated and persisted");
        }
        Ok(())
    }

    pub fn set_current_screen(&mut self, new_current_screen: CurrentScreen) {
        self.state.navigation.current_screen = new_current_screen;
    }

    /// Projects the current [`AppState`] into an owned [`AppViewModel`].
    ///
    /// This is the primary way for the orchestration layer to hand off
    /// presentation data to the UI actor without exposing raw `AppState`.
    pub fn present(&self) -> AppViewModel {
        view_model::project_state(&self.state)
    }
}
