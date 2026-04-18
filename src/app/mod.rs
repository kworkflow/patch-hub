pub mod config;
mod cover_renderer;
mod patch_renderer;
pub mod screens;

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
        shell::{ShellCommand, ShellTrait},
    },
    lore::{
        application::{api::LoreServiceApi, cache::CacheMode, errors::LoreError},
        domain::patch::{Author, Patch},
        infrastructure::patchset_parser::split_cover,
    },
    ui::popup::{info_popup::InfoPopUp, PopUp},
};

use config::Config;
use cover_renderer::render_cover;
use patch_renderer::{render_patch_preview, PatchRenderer};
use screens::{
    bookmarked::BookmarkedPatchsets,
    details_actions::{DetailsActions, PatchsetAction},
    edit_config::EditConfig,
    latest::LatestPatchsets,
    mail_list::MailingListSelection,
    CurrentScreen,
};

/// Result type signalling whether a patchset was successfully loaded.
pub enum B4Result {
    PatchFound,
    PatchNotFound(String),
}

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
    /// Single entry-point to the Lore bounded context
    pub lore_service: Box<dyn LoreServiceApi>,
    pub popup: Option<Box<dyn PopUp>>,
    /// Filesystem abstraction
    pub fs: Box<dyn FileSystemTrait>,
    /// Shell abstraction
    pub shell: Box<dyn ShellTrait>,
    /// Environment abstraction
    pub env: Box<dyn EnvTrait>,
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
    ) -> color_eyre::Result<Self> {
        let bootstrap = lore_service.warm_bootstrap_cache().unwrap_or_default();

        event!(Level::INFO, "patch-hub started");
        collect_garbage(&config);

        Ok(App {
            current_screen: CurrentScreen::MailingListSelection,
            mailing_list_selection: MailingListSelection {
                mailing_lists: bootstrap.mailing_lists.clone(),
                target_list: String::new(),
                possible_mailing_lists: bootstrap.mailing_lists,
                highlighted_list_index: 0,
            },
            latest_patchsets: None,
            details_actions: None,
            edit_config: None,
            bookmarked_patchsets: BookmarkedPatchsets {
                bookmarked_patchsets: bootstrap.bookmarks,
                patchset_index: 0,
            },
            reviewed_patchsets: bootstrap.reviewed,
            config,
            lore_service,
            popup: None,
            fs,
            shell,
            env,
        })
    }

    /// Initializes field [App::latest_patchsets], from currently selected
    /// mailing list in [App::mailing_list_selection].
    pub fn init_latest_patchsets(&mut self) {
        let list_index = self.mailing_list_selection.highlighted_list_index;
        let target_list = self.mailing_list_selection.possible_mailing_lists[list_index]
            .name()
            .to_string();
        self.latest_patchsets = Some(LatestPatchsets::new(target_list, self.config.page_size()));
    }

    /// Sets field [App::latest_patchsets] to `None`.
    pub fn reset_latest_patchsets(&mut self) {
        self.latest_patchsets = None;
    }

    /// Fetches (or re-fetches) the current page of [App::latest_patchsets]
    /// from [App::lore_service].
    ///
    /// Uses field-level splitting so the borrow checker can see that
    /// `lore_service` and `latest_patchsets` are disjoint borrows.
    pub fn fetch_latest_current_page(&mut self) -> color_eyre::Result<()> {
        let App {
            lore_service,
            latest_patchsets,
            ..
        } = self;
        if let Some(patchsets) = latest_patchsets.as_mut() {
            patchsets.fetch_current_page(lore_service.as_mut(), CacheMode::UseCache)
        } else {
            Ok(())
        }
    }

    /// Refreshes available mailing lists via [App::lore_service] and updates
    /// [App::mailing_list_selection].
    ///
    /// Uses field-level splitting so the borrow checker can see that
    /// `lore_service` and `mailing_list_selection` are disjoint borrows.
    pub fn refresh_mailing_lists(&mut self) -> color_eyre::Result<()> {
        let App {
            lore_service,
            mailing_list_selection,
            ..
        } = self;
        mailing_list_selection
            .refresh_available_mailing_lists(lore_service.as_mut(), CacheMode::Refresh)
    }

    /// Initializes field [App::details_actions], from currently selected
    /// patchset in [App::bookmarked_patchsets] or [App::latest_patchsets],
    /// depending on the value of [App::current_screen].
    pub fn init_details_actions(&mut self) -> color_eyre::Result<B4Result> {
        let representative_patch: Patch;
        let mut is_patchset_bookmarked = true;

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

        let details = match self
            .lore_service
            .fetch_patchset_details(&representative_patch)
        {
            Ok(d) => d,
            Err(LoreError::PatchNotFound(err)) => return Ok(B4Result::PatchNotFound(err)),
            Err(e) => bail!("{e:#?}"),
        };

        let mut patches_preview: Vec<Text> = Vec::new();
        let mut reviewed_by: Vec<HashSet<Author>> = Vec::new();
        let mut tested_by: Vec<HashSet<Author>> = Vec::new();
        let mut acked_by: Vec<HashSet<Author>> = Vec::new();

        for (raw_patch, tag_summary) in details.raw_patches.iter().zip(details.tag_summary.iter()) {
            let raw_patch_expanded = raw_patch.replace('\t', "        ");
            let (raw_cover, raw_diff) = split_cover(&raw_patch_expanded);

            reviewed_by.push(tag_summary.reviewed_by.clone());
            tested_by.push(tag_summary.tested_by.clone());
            acked_by.push(tag_summary.acked_by.clone());

            let rendered_cover =
                match render_cover(&*self.shell, raw_cover, self.config.cover_renderer()) {
                    Ok(render) => render,
                    Err(_) => {
                        event!(
                            Level::ERROR,
                            "Failed to render cover preview with external program"
                        );
                        raw_cover.to_string()
                    }
                };

            let rendered_patch =
                match render_patch_preview(&*self.shell, raw_diff, self.config.patch_renderer()) {
                    Ok(render) => render,
                    Err(_) => {
                        event!(
                            Level::ERROR,
                            "Failed to render patch preview with external program",
                        );
                        raw_diff.to_string()
                    }
                };

            patches_preview.push(format!("{rendered_cover}---\n{rendered_patch}").into_text()?);
        }

        let has_cover_letter = representative_patch.number_in_series() == 0;
        let patches_to_reply = vec![false; details.raw_patches.len()];

        self.details_actions = Some(DetailsActions {
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
            last_screen: self.current_screen.clone(),
        });

        Ok(B4Result::PatchFound)
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
    /// This function will panic if `details_actions` is `None`.
    pub fn consolidate_patchset_actions(&mut self) -> color_eyre::Result<()> {
        let details_actions = self.details_actions.as_ref().unwrap();

        let representative_patch = details_actions.representative_patch.clone();
        let patchset_actions = details_actions.patchset_actions.clone();
        let raw_patches = details_actions.raw_patches.clone();
        let patches_to_reply = details_actions.patches_to_reply.clone();

        if let Some(true) = patchset_actions.get(&PatchsetAction::Bookmark) {
            self.bookmarked_patchsets
                .bookmark_selected_patch(&representative_patch);
        } else {
            self.bookmarked_patchsets
                .unbookmark_selected_patch(&representative_patch);
        }

        self.lore_service
            .save_bookmarked_patchsets(&self.bookmarked_patchsets.bookmarked_patchsets)
            .map_err(|e| eyre!("{e:#?}"))?;

        if let Some(true) = patchset_actions.get(&PatchsetAction::ReplyWithReviewedBy) {
            let mut successful_indexes = self
                .reviewed_patchsets
                .remove(&representative_patch.message_id().href)
                .unwrap_or_default();

            let (git_user_name, git_user_email) = self.lore_service.get_git_signature("");

            if git_user_name.is_empty() || git_user_email.is_empty() {
                println!("`git config user.name` or `git config user.email` not set\nAborting...");
            } else {
                let mktemp_cmd = ShellCommand::new("mktemp").arg("--directory");
                let tmp_out = self
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
                    .lore_service
                    .prepare_reply_commands(
                        tmp_dir,
                        "all",
                        &raw_patches,
                        &patches_to_reply,
                        &git_signature,
                        self.config.git_send_email_options(),
                    )
                    .map_err(|e| eyre!("{e:#?}"))?;

                let reply_indexes: Vec<usize> = patches_to_reply
                    .iter()
                    .enumerate()
                    .filter_map(|(i, &val)| if val { Some(i) } else { None })
                    .collect();
                for (i, command) in git_reply_commands.into_iter().enumerate() {
                    let success = self.shell.spawn_interactive(&command).unwrap_or(false);
                    if success {
                        successful_indexes.insert(reply_indexes[i]);
                    }
                }
            }

            self.reviewed_patchsets.insert(
                representative_patch.message_id().href.clone(),
                successful_indexes,
            );

            self.lore_service
                .save_reviewed_patchsets(&self.reviewed_patchsets)
                .map_err(|e| eyre!("{e:#?}"))?;

            self.details_actions
                .as_mut()
                .unwrap()
                .reset_reply_with_reviewed_by_action();
        }

        if let Some(true) = self
            .details_actions
            .as_ref()
            .unwrap()
            .patchset_actions
            .get(&PatchsetAction::Apply)
        {
            let popup = match self.details_actions.as_ref().unwrap().apply_patchset(
                &*self.fs,
                &*self.shell,
                &self.config,
            ) {
                Ok(msg) => InfoPopUp::generate_info_popup("Patchset Apply Success", &msg),
                Err(msg) => InfoPopUp::generate_info_popup("Patchset Apply Fail", &msg),
            };

            self.popup = Some(popup);

            self.details_actions.as_mut().unwrap().toggle_apply_action();
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
            if let Ok(cache_dir) = edit_config.cache_dir(&*self.fs) {
                self.config.set_cache_dir(cache_dir)
            }
            if let Ok(data_dir) = edit_config.data_dir(&*self.fs) {
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

        if !self.env.which("b4") {
            event!(
                Level::ERROR,
                "b4 is not installed, patchsets cannot be downloaded"
            );
            app_can_run = false;
        }

        if !self.env.which("git") {
            event!(Level::WARN, "git is not installed, send-email won't work");
        }

        match self.config.patch_renderer() {
            PatchRenderer::Bat => {
                if !self.env.which("bat") {
                    event!(
                        Level::WARN,
                        "bat is not installed, patch rendering will fallback to default"
                    );
                }
            }
            PatchRenderer::Delta => {
                if !self.env.which("delta") {
                    event!(
                        Level::WARN,
                        "delta is not installed, patch rendering will fallback to default",
                    );
                }
            }
            PatchRenderer::DiffSoFancy => {
                if !self.env.which("diff-so-fancy") {
                    event!(
                        Level::WARN,
                        "diff-so-fancy is not installed, patch rendering will fallback to default",
                    );
                }
            }
            _ => {}
        }

        app_can_run
    }
}
