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

#[cfg(test)]
mod integration_tests;

use color_eyre::{
    eyre::{bail, eyre},
    Result,
};
use tracing::{debug, event, info, warn, Level};

use std::sync::Arc;

use chrono::{SecondsFormat, Utc};

use crate::{
    app::actions::{
        apply::{AppliedPatchset, ApplyPatchsetRequest},
        reviewed_reply::ReviewedReplyRequest,
        PatchsetActionService,
    },
    config::{ConfigHandle, ConfigSnapshot},
    infrastructure::{
        file_system::FileSystemTrait, monitoring::logging::garbage_collector::collect_garbage,
        shell::ShellTrait,
    },
    kw::{
        handle::KwHandle,
        history::{KwApplyRecord, KwHistoryStore},
        status::KwJobStatus,
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
    /// Direct access to the store, for the non-unix fallback below.
    pub kw_history: Arc<dyn KwHistoryStore>,
    /// `None` on non-unix builds, where ProcessTrait (and thus KwActor)
    /// does not exist; apply-history writes then go to `kw_history`
    /// directly, as they did before the actor landed.
    pub kw: Option<KwHandle>,
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
        kw_history: Arc<dyn KwHistoryStore>,
        kw: Option<KwHandle>,
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
                kw_history,
                kw,
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
        self.execute_apply_patchset().await;
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
        if patchset_action_selected(details, &PatchsetAction::ReplyWithReviewedBy) {
            let message_id = details.representative_patch.message_id().href.clone();
            debug!(msg_id = message_id, "executing reviewed-by reply");
            let successful_indexes = self
                .state
                .user_state
                .reviewed_patchsets
                .remove(&message_id)
                .unwrap_or_default();
            let request = reviewed_reply_request(
                details,
                successful_indexes,
                self.state.config.git_send_email_options().to_string(),
            );
            let action_service = PatchsetActionService::new(
                &*self.services.fs,
                &*self.services.shell,
                &self.services.lore_api,
            );
            let result = action_service.execute_reviewed_reply(request).await?;

            self.state
                .user_state
                .reviewed_patchsets
                .insert(message_id.clone(), result.into_successful_indexes());

            self.services
                .lore_api
                .save_reviewed(self.state.user_state.reviewed_patchsets.clone())
                .await
                .map_err(|e| eyre!("{e:#?}"))?;

            info!(
                msg_id = message_id,
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

    async fn execute_apply_patchset(&mut self) {
        let details = self
            .state
            .lore
            .details
            .as_ref()
            .expect("invariant: details must be loaded before executing apply patchset");

        if patchset_action_selected(details, &PatchsetAction::Apply) {
            debug!("applying patchset via git-am");
            // A running kw job owns the tree: applying would rewrite the
            // branch the job is building under it (integration plan
            // §2.1i). No Start can race this check: both paths are
            // serialized by the AppActor loop.
            let popup = match self.kw_job_running_popup().await {
                Some(popup) => popup,
                None => self.apply_patchset_popup(details).await,
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

    /// The popup blocking an apply while a kw job runs, if a job is in
    /// fact running. An unreachable actor cannot be running a job, so a
    /// status-query failure lets the apply proceed.
    async fn kw_job_running_popup(&self) -> Option<popup::AppPopup> {
        let kw = self.services.kw.as_ref()?;
        match kw.get_status().await {
            Ok(snapshot) if matches!(snapshot.job, KwJobStatus::Running { .. }) => {
                Some(popup::AppPopup::info(
                    "Patchset Apply Blocked",
                    " A kw job is running on the kernel tree.\n\nApplying a patchset now would rewrite the branch the job is building under it.\n\nWait for the job to finish, then apply again.",
                ))
            }
            _ => None,
        }
    }

    /// Runs the git-am apply and maps the outcome to the result popup,
    /// recording the apply in the kw history on success.
    async fn apply_patchset_popup(&self, details: &PatchsetDetailsState) -> popup::AppPopup {
        let request = apply_patchset_request(details);
        let action_service = PatchsetActionService::new(
            &*self.services.fs,
            &*self.services.shell,
            &self.services.lore_api,
        );
        match action_service.apply_patchset(&request, &self.state.config) {
            Ok(applied) => {
                let popup_body = match kw_apply_record(details, &self.state.config, &applied) {
                    // Defensive: the apply itself resolved this tree from
                    // the same snapshot, so this is unreachable unless the
                    // config changed mid-apply.
                    None => {
                        warn!(
                            "kw apply history skipped: target kernel tree is no longer configured"
                        );
                        format!(
                            "{}\n\nWarning: the apply was not recorded in the kw history: target kernel tree is no longer configured",
                            applied.message
                        )
                    }
                    Some(record) => {
                        // History writes go through KwActor so apply
                        // recording serializes with job state; without an
                        // actor (non-unix), write the store directly.
                        let recorded = match &self.services.kw {
                            Some(kw) => kw.record_apply(record).await.map_err(|e| e.to_string()),
                            None => self
                                .services
                                .kw_history
                                .record_apply(record)
                                .map_err(|e| e.to_string()),
                        };
                        // The git apply itself succeeded; a history-write
                        // failure must not turn it into a reported failure.
                        match recorded {
                            Ok(()) => applied.message,
                            Err(e) => {
                                warn!(error = %e, "failed to record kw apply history");
                                format!(
                                    "{}\n\nWarning: the apply was not recorded in the kw history: {e}\nIf this warning keeps appearing, inspect or delete that file.",
                                    applied.message
                                )
                            }
                        }
                    }
                };
                popup::AppPopup::info("Patchset Apply Success", popup_body)
            }
            Err(msg) => popup::AppPopup::info("Patchset Apply Fail", msg),
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

fn patchset_action_selected(details: &PatchsetDetailsState, action: &PatchsetAction) -> bool {
    matches!(details.patchset_actions.get(action), Some(true))
}

fn reviewed_reply_request(
    details: &PatchsetDetailsState,
    successful_indexes: std::collections::HashSet<usize>,
    git_send_email_options: String,
) -> ReviewedReplyRequest {
    ReviewedReplyRequest {
        raw_patches: details.raw_patches.clone(),
        patches_to_reply: details.patches_to_reply.clone(),
        successful_indexes,
        git_send_email_options,
    }
}

fn apply_patchset_request(details: &PatchsetDetailsState) -> ApplyPatchsetRequest {
    ApplyPatchsetRequest {
        patch_title: details.representative_patch.title().clone(),
        patchset_path: details.patchset_path.clone(),
    }
}

fn kw_apply_record(
    details: &PatchsetDetailsState,
    config: &ConfigSnapshot,
    applied: &AppliedPatchset,
) -> Option<KwApplyRecord> {
    let kernel_tree_id = config.target_kernel_tree().as_ref()?;
    let kernel_tree = config.get_kernel_tree(kernel_tree_id)?;

    Some(KwApplyRecord {
        message_id: details.representative_patch.message_id().href.clone(),
        kernel_tree_id: kernel_tree_id.clone(),
        tree_path: kernel_tree.path().clone(),
        applied_branch: applied.applied_branch.clone(),
        base_branch: kernel_tree.branch().clone(),
        applied_at: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
    })
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use serde_xml_rs::from_str;

    use super::*;

    fn test_patch() -> Patch {
        from_str(
            r#"
            <entry xmlns:thr="http://purl.org/syndication/thread/1.0">
                <author>
                    <name>Foo Bar</name>
                    <email>foo@bar.foo.bar</email>
                </author>
                <title>[PATCH 1/1] test patch</title>
                <updated>2024-07-06T19:15:48Z</updated>
                <link href="http://lore.kernel.org/some-list/1234-1-foo@bar.foo.bar" />
                <id>urn:uuid:123-abcd-1f2a3b</id>
                <content></content>
            </entry>
        "#,
        )
        .expect("test patch XML should deserialize")
    }

    fn details_state() -> PatchsetDetailsState {
        PatchsetDetailsState {
            representative_patch: test_patch(),
            raw_patches: vec!["raw patch 0".to_string(), "raw patch 1".to_string()],
            patches_preview: vec!["preview 0".to_string(), "preview 1".to_string()],
            has_cover_letter: false,
            patches_to_reply: vec![false, true],
            patchset_path: "/tmp/patchset.mbx".to_string(),
            preview_index: 0,
            preview_scroll_offset: 0,
            preview_pan: 0,
            preview_fullscreen: false,
            patchset_actions: HashMap::from([
                (PatchsetAction::Bookmark, false),
                (PatchsetAction::ReplyWithReviewedBy, true),
                (PatchsetAction::Apply, true),
            ]),
            reviewed_by: vec![HashSet::new(), HashSet::new()],
            tested_by: vec![HashSet::new(), HashSet::new()],
            acked_by: vec![HashSet::new(), HashSet::new()],
            last_screen: CurrentScreen::LatestPatchsets,
        }
    }

    #[test]
    fn patchset_action_selected_reads_action_map() {
        let mut details = details_state();

        assert!(patchset_action_selected(&details, &PatchsetAction::Apply));
        assert!(patchset_action_selected(
            &details,
            &PatchsetAction::ReplyWithReviewedBy
        ));

        details
            .patchset_actions
            .insert(PatchsetAction::Apply, false);

        assert!(!patchset_action_selected(&details, &PatchsetAction::Apply));
    }

    #[test]
    fn reviewed_reply_request_copies_reply_inputs() {
        let details = details_state();
        let request = reviewed_reply_request(
            &details,
            HashSet::from([4usize]),
            "--dry-run --suppress-cc=all".to_string(),
        );

        assert_eq!(details.raw_patches, request.raw_patches);
        assert_eq!(details.patches_to_reply, request.patches_to_reply);
        assert_eq!(HashSet::from([4]), request.successful_indexes);
        assert_eq!(
            "--dry-run --suppress-cc=all",
            request.git_send_email_options
        );
    }

    #[test]
    fn apply_patchset_request_copies_apply_inputs() {
        let details = details_state();
        let request = apply_patchset_request(&details);

        assert_eq!("[PATCH 1/1] test patch", request.patch_title);
        assert_eq!("/tmp/patchset.mbx", request.patchset_path);
    }
}
