use ratatui::text::Text;

use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::{
    app::config::{Config, KernelTree},
    infrastructure::{
        file_system::FileSystemTrait,
        shell::{ShellCommand, ShellTrait},
    },
    lore::domain::patch::{Author, Patch},
};

use super::CurrentScreen;

pub struct PatchsetDetailsState {
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
}

const LAST_LINE_PADDING: usize = 10;

#[derive(Clone, Hash, Eq, PartialEq)]
pub enum PatchsetAction {
    Bookmark,
    ReplyWithReviewedBy,
    Apply,
}

impl PatchsetDetailsState {
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
        let current_value = *self
            .patchset_actions
            .get(&patchset_action)
            .expect("PatchsetDetailsState::patchset_actions must be initialized properly");
        self.patchset_actions
            .insert(patchset_action, !current_value);
    }

    pub fn actions_require_user_io(&self) -> bool {
        self.patches_to_reply.contains(&true)
    }

    /// Checks if there is a `target_kernel_tree` and if it is in `Config::kernel_trees` and if
    /// that kernel tree is a valid git directory.
    ///
    /// Returns the a valid `KernelTree` or a `String` with the error message on failure.
    fn validate_kernel_tree<'a>(
        &self,
        fs: &dyn FileSystemTrait,
        config: &'a Config,
    ) -> Result<&'a KernelTree, String> {
        let kernel_tree_id = if let Some(target) = config.target_kernel_tree() {
            target
        } else {
            return Err("target kernel tree unset".to_string());
        };

        let kernel_tree = if let Some(tree) = config.get_kernel_tree(kernel_tree_id) {
            tree
        } else {
            return Err(format!("invalid target kernel tree '{kernel_tree_id}'"));
        };

        let kernel_tree_path = Path::new(kernel_tree.path());
        if !fs.is_dir(kernel_tree_path) {
            return Err(format!("{} isn't a directory", kernel_tree.path()));
        } else if !fs.is_dir(&kernel_tree_path.join(".git")) {
            return Err(format!("{} isn't a git repository", kernel_tree.path()));
        }

        Ok(kernel_tree)
    }

    // Ensures the kernel directory is not currently in another git operation,
    // that it does not have unstaged or uncommited changes, and that the base branch
    // is valid.
    //
    // Returns `()` on success and a `String` with an error message on failure.
    fn check_git_state(
        &self,
        fs: &dyn FileSystemTrait,
        shell: &dyn ShellTrait,
        kernel_tree: &KernelTree,
    ) -> Result<(), String> {
        let kernel_tree_path = Path::new(kernel_tree.path());

        if fs.is_dir(&kernel_tree_path.join(".git/rebase-merge")) {
            return Err(
                "rebase in progress. \nrun `git rebase --abort` before continuing".to_string(),
            );
        } else if fs.is_file(&kernel_tree_path.join(".git/MERGE_HEAD")) {
            return Err(
                "merge in progress. \nrun `git merge --abort` before continuing".to_string(),
            );
        } else if fs.is_file(&kernel_tree_path.join(".git/BISECT_LOG")) {
            return Err(
                "bisect in progress. \nrun `git bisect reset` before continuing".to_string(),
            );
        } else if fs.is_dir(&kernel_tree_path.join(".git/rebase-apply")) {
            return Err(
                "`git am` already in progress. \nrun `git am --abort` before continuing"
                    .to_string(),
            );
        }

        let status_out = shell
            .execute(
                &ShellCommand::new("git")
                    .arg("-C")
                    .arg(kernel_tree.path())
                    .args(["status", "--porcelain"]),
            )
            .map_err(|e| format!("failed to check git status {e}"))?;

        let status_output = String::from_utf8_lossy(&status_out.stdout);
        if !status_output.is_empty() {
            return Err(format!(
                "there are staged and/or unstaged changes\n{status_output}"
            ));
        }

        let show_ref_out = shell
            .execute(
                &ShellCommand::new("git")
                    .arg("-C")
                    .arg(kernel_tree.path())
                    .args(["show-ref", "--verify", "--quiet"])
                    .arg(format!("refs/heads/{}", kernel_tree.branch())),
            )
            .map_err(|e| format!("failed to verify branch: {e}"))?;

        if !show_ref_out.success {
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
    fn get_current_branch(
        &self,
        shell: &dyn ShellTrait,
        kernel_tree: &KernelTree,
    ) -> Result<String, String> {
        let out = shell
            .execute(
                &ShellCommand::new("git")
                    .arg("-C")
                    .arg(kernel_tree.path())
                    .args(["rev-parse", "--abbrev-ref", "HEAD"]),
            )
            .map_err(|e| format!("failed to get current branch: {e}"))?;

        let mut branch = String::from_utf8_lossy(&out.stdout).to_string();
        branch.pop();
        Ok(branch)
    }

    /// Switch the supplied kernel tree to the supplied branch, if it exists.
    ///
    /// Returns `()` on sucess and a `String` with the error message on failure.
    fn switch_to_branch(
        &self,
        shell: &dyn ShellTrait,
        kernel_tree: &KernelTree,
        branch: &str,
    ) -> Result<(), String> {
        let out = shell
            .execute(
                &ShellCommand::new("git")
                    .arg("-C")
                    .arg(kernel_tree.path())
                    .args(["switch", branch]),
            )
            .map_err(|e| format!("failed to switch branch: {e}"))?;

        if !out.success {
            return Err(format!(
                "failed to switch to branch '{}': {}",
                branch,
                String::from_utf8_lossy(&out.stderr)
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
        shell: &dyn ShellTrait,
        kernel_tree: &KernelTree,
        config: &Config,
    ) -> Result<String, String> {
        self.switch_to_branch(shell, kernel_tree, kernel_tree.branch())?;

        let target_branch_name = format!(
            "{}{}",
            config.git_am_branch_prefix(),
            chrono::Utc::now().format("%Y-%m-%d-%H-%M-%S")
        );

        let out = shell
            .execute(
                &ShellCommand::new("git")
                    .arg("-C")
                    .arg(kernel_tree.path())
                    .args(["checkout", "-b", &target_branch_name]),
            )
            .map_err(|e| format!("failed to create target branch: {e}"))?;

        if !out.success {
            return Err(format!(
                "failed to create branch '{}': {}",
                target_branch_name,
                String::from_utf8_lossy(&out.stderr)
            ));
        }

        Ok(target_branch_name)
    }

    /// Apply the selected patchset on the given `kernel_tree` with arguments from `Config`
    ///
    /// Returns `()` on sucess and a `String` containing the error message on failure.
    fn run_git_am(
        &self,
        shell: &dyn ShellTrait,
        kernel_tree: &KernelTree,
        config: &Config,
    ) -> Result<(), String> {
        let mut git_am_cmd = ShellCommand::new("git")
            .arg("-C")
            .arg(kernel_tree.path())
            .args(["am", &self.patchset_path]);
        for opt in config.git_am_options().split_whitespace() {
            git_am_cmd = git_am_cmd.arg(opt);
        }

        let out = shell
            .execute(&git_am_cmd)
            .map_err(|e| format!("failed to execute git-am: {e}"))?;

        if !out.success {
            let _ = shell.execute(
                &ShellCommand::new("git")
                    .arg("-C")
                    .arg(kernel_tree.path())
                    .args(["am", "--abort"]),
            );

            return Err(String::from_utf8_lossy(&out.stderr).to_string());
        }

        Ok(())
    }

    /// Try to apply the patchset to a target kernel tree and returns a `String`
    /// informing if the apply succeeded or failed and why.
    ///
    /// Returns a `Result<String, String>` containing either the success or the error message.
    /// # TODO:
    /// - Add unit tests
    pub fn apply_patchset(
        &self,
        fs: &dyn FileSystemTrait,
        shell: &dyn ShellTrait,
        config: &Config,
    ) -> Result<String, String> {
        let kernel_tree = self.validate_kernel_tree(fs, config)?;
        self.check_git_state(fs, shell, kernel_tree)?;

        let original_branch = self.get_current_branch(shell, kernel_tree)?;
        let target_branch = self.create_target_branch(shell, kernel_tree, config)?;

        let git_am_result = self.run_git_am(shell, kernel_tree, config);
        self.switch_to_branch(shell, kernel_tree, &original_branch)?;

        match git_am_result {
            Ok(_) => {
                Ok(format!(" Patchset '{}' applied successfully!\n\n - Kernel Tree: '{}'\n\n - Base Branch: '{}'\n\n - Applied branch: '{}'", self.representative_patch.title(), kernel_tree.path(), kernel_tree.branch(), &target_branch))
        },
            Err(e) => Err(format!( "`git am` failed\n{}{}", &original_branch, e))
        }
    }
}
