use crate::app::config::{Config, KernelTree};

use super::CurrentScreen;
use ::patch_hub::lore::{lore_api_client::BlockingLoreAPIClient, lore_session, patch::Patch};
use color_eyre::eyre::bail;
use patch_hub::lore::patch::Author;
use ratatui::text::Text;
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    process::Command,
};

pub struct DetailsActions {
    pub representative_patch: Patch,
    /// Raw patches as plain text files
    pub raw_patches: Vec<String>,
    /// Patches in the format to be displayed as preview
    pub patches_preview: Vec<Text<'static>>,
    /// Indicates if patchset has a cover letter
    pub has_cover_letter: bool,
    /// Which patches to reply
    pub patches_to_reply: Vec<bool>,
    /// Path to applicable .mbx of patchset
    #[allow(dead_code)]
    pub patchset_path: String,
    pub preview_index: usize,
    pub preview_scroll_offset: usize,
    /// Horizontal offset
    pub preview_pan: usize,
    /// If true, display the preview in full screen
    pub preview_fullscreen: bool,
    pub patchset_actions: HashMap<PatchsetAction, bool>,
    /// For each patch, a set of `Authors` that appear in `Reviewed-by` trailers
    pub reviewed_by: Vec<HashSet<Author>>,
    /// For each patch, a set of `Authors` that appear in `Tested-by` trailers
    pub tested_by: Vec<HashSet<Author>>,
    /// For each patch, a set of `Authors` that appear in `Acked-by` trailers
    pub acked_by: Vec<HashSet<Author>>,
    pub last_screen: CurrentScreen,
    pub lore_api_client: BlockingLoreAPIClient,
}

const LAST_LINE_PADDING: usize = 10;

#[derive(Hash, Eq, PartialEq)]
pub enum PatchsetAction {
    Bookmark,
    ReplyWithReviewedBy,
    Apply,
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

    pub fn toggle_reply_with_reviewed_by_action(&mut self, all: bool) {
        if all {
            if self.patches_to_reply.contains(&false) {
                // If there is at least one patch not to be replied, set all to be
                self.patches_to_reply = vec![true; self.patches_to_reply.len()];
            } else {
                // If all patches are set to be replied, set none to be
                self.patches_to_reply = vec![false; self.patches_to_reply.len()];
            }
        } else if let Some(entry) = self.patches_to_reply.get_mut(self.preview_index) {
            *entry = !*entry;
        }

        if self.patches_to_reply.contains(&true) {
            self.patchset_actions
                .insert(PatchsetAction::ReplyWithReviewedBy, true);
        } else {
            self.patchset_actions
                .insert(PatchsetAction::ReplyWithReviewedBy, false);
        }
    }

    pub fn toggle_apply_action(&mut self) {
        self.toggle_action(PatchsetAction::Apply);
    }

    pub fn reset_reply_with_reviewed_by_action(&mut self) {
        self.patches_to_reply = vec![false; self.patches_to_reply.len()];
        self.patchset_actions
            .insert(PatchsetAction::ReplyWithReviewedBy, false);
    }

    pub fn toggle_action(&mut self, patchset_action: PatchsetAction) {
        let current_value = *self.patchset_actions.get(&patchset_action).unwrap();
        self.patchset_actions
            .insert(patchset_action, !current_value);
    }

    pub fn actions_require_user_io(&self) -> bool {
        self.patches_to_reply.contains(&true)
    }

    pub fn reply_patchset_with_reviewed_by(
        &self,
        target_list: &str,
        git_send_email_options: &str,
        successful_indexes: &mut HashSet<usize>,
    ) -> color_eyre::Result<()> {
        let (git_user_name, git_user_email) = lore_session::get_git_signature("");

        if git_user_name.is_empty() || git_user_email.is_empty() {
            println!("`git config user.name` or `git config user.email` not set\nAborting...");
            return Ok(());
        }

        let tmp_dir = Command::new("mktemp").arg("--directory").output().unwrap();
        let tmp_dir = Path::new(std::str::from_utf8(&tmp_dir.stdout).unwrap().trim());

        let git_reply_commands = match lore_session::prepare_reply_patchset_with_reviewed_by(
            &self.lore_api_client,
            tmp_dir,
            target_list,
            &self.raw_patches,
            &self.patches_to_reply,
            &format!("{git_user_name} <{git_user_email}>"),
            git_send_email_options,
        ) {
            Ok(commands_vector) => commands_vector,
            Err(failed_patch_html_request) => {
                bail!(format!("{failed_patch_html_request:#?}"));
            }
        };

        let reply_indexes: Vec<usize> = self
            .patches_to_reply
            .iter()
            .enumerate()
            .filter_map(|(i, &val)| if val { Some(i) } else { None })
            .collect();
        for (i, mut command) in git_reply_commands.into_iter().enumerate() {
            let mut child = command.spawn().unwrap();
            let exit_status = child.wait().unwrap();
            if exit_status.success() {
                successful_indexes.insert(reply_indexes[i]);
            }
        }

        Ok(())
    }

    /// Checks if there is a `target_kernel_tree` and if it is in `Config::kernel_trees` and if
    /// that kernel tree is a valid git directory.
    ///
    /// Returns the a valid `KernelTree` or a `String` with the error message on failure.
    fn validate_kernel_tree<'a>(&self, config: &'a Config) -> Result<&'a KernelTree, String> {
        let kernel_tree_id = if let Some(target) = config.target_kernel_tree() {
            target
        } else {
            return Err("target kernel tree unset".to_string());
        };

        let kernel_tree = if let Some(tree) = config.get_kernel_tree(kernel_tree_id) {
            tree
        } else {
            return Err(format!("invalid target kernel tree '{}'", kernel_tree_id));
        };

        let kernel_tree_path = Path::new(kernel_tree.path());
        if !kernel_tree_path.is_dir() {
            return Err(format!("{} isn't a directory", kernel_tree.path()));
        } else if !kernel_tree_path.join(".git").is_dir() {
            return Err(format!("{} isn't a git repository", kernel_tree.path()));
        }

        Ok(kernel_tree)
    }

    // Ensures the kernel directory is not currently in another git operation,
    // that it does not have unstaged or uncommited changes, and that the base branch
    // is valid.
    //
    // Returns `()` on success and a `String` with an error message on failure.
    fn check_git_state(&self, kernel_tree: &KernelTree) -> Result<(), String> {
        let kernel_tree_path = Path::new(kernel_tree.path());

        if kernel_tree_path.join(".git/rebase-merge").is_dir() {
            return Err(
                "rebase in progress. \nrun `git rebase --abort` before continuing".to_string(),
            );
        } else if kernel_tree_path.join(".git/MERGE_HEAD").is_file() {
            return Err(
                "merge in progress. \nrun `git merge --abort` before continuing".to_string(),
            );
        } else if kernel_tree_path.join(".git/BISECT_LOG").is_file() {
            return Err(
                "bisect in progress. \nrun `git bisect reset` before continuing".to_string(),
            );
        } else if kernel_tree_path.join(".git/rebase-apply").is_dir() {
            return Err(
                "`git am` already in progress. \nrun `git am --abort` before continuing"
                    .to_string(),
            );
        }

        let git_status_out = Command::new("git")
            .arg("-C")
            .arg(kernel_tree.path())
            .arg("status")
            .arg("--porcelain")
            .output()
            .map_err(|e| format!("failed to check git status {}", e))?;

        let status_output = String::from_utf8_lossy(&git_status_out.stdout);
        if !status_output.is_empty() {
            return Err(format!(
                "there are staged and/or unstaged changes\n{}",
                status_output
            ));
        }

        let git_show_ref_out = Command::new("git")
            .arg("-C")
            .arg(kernel_tree.path())
            .arg("show-ref")
            .arg("--verify")
            .arg("--quiet")
            .arg(format!("refs/heads/{}", kernel_tree.branch()))
            .output()
            .map_err(|e| format!("failed to verify branch: {}", e))?;

        if !git_show_ref_out.status.success() {
            return Err(format!(
                "invalid branch '{}' for '{}'",
                kernel_tree.branch(),
                kernel_tree.path()
            ));
        }

        Ok(())
    }

    /// Get the current branch of the supplied kernel tree
    ///
    /// Returns the branch name as a `String` or a `String` with the error message on failure
    fn get_current_branch(&self, kernel_tree: &KernelTree) -> Result<String, String> {
        let original_branch = Command::new("git")
            .arg("-C")
            .arg(kernel_tree.path())
            .arg("rev-parse")
            .arg("--abbrev-ref")
            .arg("HEAD")
            .output()
            .map_err(|e| format!("failed to get current branch: {}", e))?;

        let mut branch = String::from_utf8_lossy(&original_branch.stdout).to_string();
        branch.pop();
        Ok(branch)
    }

    /// Switch the supplied kernel tree to the supplied branch, if it exists.
    ///
    /// Returns `()` on sucess and a `String` with the error message on failure.
    fn switch_to_branch(&self, kernel_tree: &KernelTree, branch: &str) -> Result<(), String> {
        let output = Command::new("git")
            .arg("-C")
            .arg(kernel_tree.path())
            .arg("switch")
            .arg(branch)
            .output()
            .map_err(|e| format!("failed to switch branch: {}", e))?;

        if !output.status.success() {
            return Err(format!(
                "failed to switch to branch '{}': {}",
                branch,
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        Ok(())
    }

    /// Create a new branch suffixed with the current timestamp.
    ///
    /// Returns a `String` with the branch name on success or the
    /// error message on failure.
    fn create_target_branch(
        &self,
        kernel_tree: &KernelTree,
        config: &Config,
    ) -> Result<String, String> {
        self.switch_to_branch(kernel_tree, kernel_tree.branch())?;

        let target_branch_name = format!(
            "{}{}",
            config.git_am_branch_prefix(),
            chrono::Utc::now().format("%Y-%m-%d-%H-%M-%S")
        );

        let output = Command::new("git")
            .arg("-C")
            .arg(kernel_tree.path())
            .arg("checkout")
            .arg("-b")
            .arg(&target_branch_name)
            .output()
            .map_err(|e| format!("failed to create target branch: {}", e))?;

        if !output.status.success() {
            return Err(format!(
                "failed to create branch '{}': {}",
                target_branch_name,
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        Ok(target_branch_name)
    }

    /// Apply the selected patchset on the given `kernel_tree` with arguments from `Config`
    ///
    /// Returns `()` on sucess and a `String` containing the error message on failure.
    fn run_git_am(&self, kernel_tree: &KernelTree, config: &Config) -> Result<(), String> {
        let mut git_am_out = Command::new("git");
        git_am_out
            .arg("-C")
            .arg(kernel_tree.path())
            .arg("am")
            .arg(&self.patchset_path);
        config.git_am_options().split_whitespace().for_each(|opt| {
            git_am_out.arg(opt);
        });

        let git_am_out = git_am_out
            .output()
            .map_err(|e| format!("failed to execute git-am: {}", e))?;

        if !git_am_out.status.success() {
            let _ = Command::new("git")
                .arg("-C")
                .arg(kernel_tree.path())
                .arg("am")
                .arg("--abort")
                .output()
                .map_err(|e| format!("failed to abort git-am: {}", e));

            return Err(String::from_utf8_lossy(&git_am_out.stderr).to_string());
        }

        Ok(())
    }

    /// Try to apply the patchset to a target kernel tree and returns a `String`
    /// informing if the apply succeeded or failed and why.
    ///
    /// Returns a `Result<String, String>` containing either the success or the error message.
    /// # TODO:
    /// - Add unit tests
    pub fn apply_patchset(&self, config: &Config) -> Result<String, String> {
        let kernel_tree = self.validate_kernel_tree(config)?;
        self.check_git_state(kernel_tree)?;

        let original_branch = self.get_current_branch(kernel_tree)?;
        let target_branch = self.create_target_branch(kernel_tree, config)?;

        let git_am_result = self.run_git_am(kernel_tree, config);
        self.switch_to_branch(kernel_tree, &original_branch)?;

        match git_am_result {
            Ok(_) => {
                Ok(format!(" Patchset '{}' applied successfully!\n\n - Kernel Tree: '{}'\n\n - Base Branch: '{}'\n\n - Applied branch: '{}'", self.representative_patch.title(), kernel_tree.path(), kernel_tree.branch(), &target_branch))
        },
            Err(e) => Err(format!( "`git am` failed\n{}{}", &original_branch, e))
        }
    }
}
