use super::CurrentScreen;
use ::patch_hub::{lore_api_client::BlockingLoreAPIClient, lore_session, patch::Patch};
use color_eyre::eyre::bail;
use std::{collections::HashMap, path::Path, process::Command};

pub struct PatchsetDetailsAndActionsState {
    pub representative_patch: Patch,
    pub patches: Vec<String>,
    pub preview_index: usize,
    pub preview_scroll_offset: usize,
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
        if (self.preview_index + 1) < self.patches.len() {
            self.preview_index += 1;
            self.preview_scroll_offset = 0;
        }
    }

    pub fn preview_previous_patch(&mut self) {
        if self.preview_index > 0 {
            self.preview_index -= 1;
            self.preview_scroll_offset = 0;
        }
    }

    pub fn preview_scroll_down(&mut self) {
        let number_of_lines = self.patches[self.preview_index].lines().count();
        if (self.preview_scroll_offset + 1) <= number_of_lines {
            self.preview_scroll_offset += 1;
        }
    }

    pub fn preview_scroll_up(&mut self) {
        if self.preview_scroll_offset > 0 {
            self.preview_scroll_offset -= 1;
        }
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
        let lore_api_client = BlockingLoreAPIClient::new();
        let (git_user_name, git_user_email) = lore_session::get_git_signature("");
        let mut successful_indexes = Vec::new();

        if git_user_name.is_empty() || git_user_email.is_empty() {
            println!("`git config user.name` or `git config user.email` not set\nAborting...");
            return Ok(successful_indexes);
        }

        let tmp_dir = Command::new("mktemp").arg("--directory").output().unwrap();
        let tmp_dir = Path::new(std::str::from_utf8(&tmp_dir.stdout).unwrap().trim());

        let git_reply_commands = match lore_session::prepare_reply_patchset_with_reviewed_by(
            &lore_api_client,
            tmp_dir,
            target_list,
            &self.patches,
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
