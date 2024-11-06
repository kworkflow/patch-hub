use super::CurrentScreen;
use ::patch_hub::lore::{lore_api_client::BlockingLoreAPIClient, lore_session, patch::Patch};
use color_eyre::eyre::bail;
use ratatui::text::Text;
use std::{collections::HashMap, path::Path, process::Command};

pub struct DetailsActions {
    pub representative_patch: Patch,
    /// Raw patches as plain text files
    pub raw_patches: Vec<String>,
    /// Patches in the format to be displayed as preview
    pub patches_preview: Vec<Text<'static>>,
    pub preview_index: usize,
    pub preview_scroll_offset: usize,
    /// Horizontal offset
    pub preview_pan: usize,
    /// If true, display the preview in full screen
    pub preview_fullscreen: bool,
    pub patchset_actions: HashMap<PatchsetAction, bool>,
    pub last_screen: CurrentScreen,
    pub lore_api_client: BlockingLoreAPIClient,
}

const LAST_LINE_PADDING: usize = 10;

#[derive(Hash, Eq, PartialEq)]
pub enum PatchsetAction {
    Bookmark,
    ReplyWithReviewedBy,
}

impl DetailsActions {
    pub fn preview_next_patch(&mut self) {
        if (self.preview_index + 1) < self.patches_preview.len() {
            self.preview_index += 1;
            self.preview_scroll_offset = 0;
            self.preview_pan = 0;
        }
    }

    pub fn preview_previous_patch(&mut self) {
        if self.preview_index > 0 {
            self.preview_index -= 1;
            self.preview_scroll_offset = 0;
            self.preview_pan = 0;
        }
    }

    /// Scroll `n` lines down
    pub fn preview_scroll_down(&mut self, n: usize) {
        // TODO: Support for renderers (only considers base preview string)
        let number_of_lines = self.patches_preview[self.preview_index].height();
        if (self.preview_scroll_offset + n) <= number_of_lines {
            self.preview_scroll_offset += n;
        }
    }

    /// Scroll `n` lines up
    pub fn preview_scroll_up(&mut self, n: usize) {
        self.preview_scroll_offset = self.preview_scroll_offset.saturating_sub(n);
    }

    /// Scroll to the last line
    pub fn go_to_last_line(&mut self) {
        // TODO: Support for renderers (only considers base preview string)
        let number_of_lines = self.patches_preview[self.preview_index].height();
        self.preview_scroll_offset = number_of_lines - LAST_LINE_PADDING;
    }

    /// Scroll to first line
    pub fn go_to_first_line(&mut self) {
        self.preview_scroll_offset = 0;
    }

    /// Move preview horizontally one column to the right
    pub fn preview_pan_right(&mut self) {
        if self.preview_pan <= 200 {
            self.preview_pan += 1;
        }
    }

    /// Move preview horizontally one column to the left
    pub fn preview_pan_left(&mut self) {
        if self.preview_pan > 0 {
            self.preview_pan -= 1;
        }
    }

    /// Move preview horizontally to start of line
    pub fn go_to_beg_of_line(&mut self) {
        self.preview_pan = 0;
    }

    /// Toggle the preview fullscreen
    pub fn toggle_preview_fullscreen(&mut self) {
        self.preview_fullscreen = !self.preview_fullscreen;
    }

    pub fn toggle_bookmark_action(&mut self) {
        self.toggle_action(PatchsetAction::Bookmark);
    }

    pub fn toggle_reply_with_reviewed_by_action(&mut self) {
        self.toggle_action(PatchsetAction::ReplyWithReviewedBy);
    }

    pub fn toggle_action(&mut self, patchset_action: PatchsetAction) {
        let current_value = *self.patchset_actions.get(&patchset_action).unwrap();
        self.patchset_actions
            .insert(patchset_action, !current_value);
    }

    pub fn actions_require_user_io(&self) -> bool {
        *self
            .patchset_actions
            .get(&PatchsetAction::ReplyWithReviewedBy)
            .unwrap()
    }

    pub fn reply_patchset_with_reviewed_by(
        &self,
        target_list: &str,
        git_send_email_options: &str,
    ) -> color_eyre::Result<Vec<usize>> {
        let (git_user_name, git_user_email) = lore_session::get_git_signature("");
        let mut successful_indexes = Vec::new();

        if git_user_name.is_empty() || git_user_email.is_empty() {
            println!("`git config user.name` or `git config user.email` not set\nAborting...");
            return Ok(successful_indexes);
        }

        let tmp_dir = Command::new("mktemp").arg("--directory").output().unwrap();
        let tmp_dir = Path::new(std::str::from_utf8(&tmp_dir.stdout).unwrap().trim());

        let git_reply_commands = match lore_session::prepare_reply_patchset_with_reviewed_by(
            &self.lore_api_client,
            tmp_dir,
            target_list,
            &self.raw_patches,
            &format!("{git_user_name} <{git_user_email}>"),
            git_send_email_options,
        ) {
            Ok(commands_vector) => commands_vector,
            Err(failed_patch_html_request) => {
                bail!(format!("{failed_patch_html_request:#?}"));
            }
        };

        for (index, mut command) in git_reply_commands.into_iter().enumerate() {
            let mut child = command.spawn().unwrap();
            let exit_status = child.wait().unwrap();
            if exit_status.success() {
                successful_indexes.push(index);
            }
        }

        Ok(successful_indexes)
    }
}
