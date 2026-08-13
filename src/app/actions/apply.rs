use std::path::Path;

use chrono::Utc;

use crate::{
    config::{ConfigSnapshot, KernelTree},
    infrastructure::{
        file_system::FileSystemTrait,
        shell::{ShellCommand, ShellTrait},
    },
};

pub(crate) struct ApplyPatchsetRequest {
    pub patch_title: String,
    pub patchset_path: String,
}

#[derive(Debug)]
pub(crate) struct AppliedPatchset {
    pub message: String,
    // Consumed by the apply-history record write (kw integration); no
    // production reader exists until that wiring lands.
    #[allow(dead_code)]
    pub applied_branch: String,
}

pub(crate) fn apply_patchset(
    request: &ApplyPatchsetRequest,
    fs: &dyn FileSystemTrait,
    shell: &dyn ShellTrait,
    config: &ConfigSnapshot,
) -> Result<AppliedPatchset, String> {
    let kernel_tree = validate_kernel_tree(fs, config)?;
    check_git_state(fs, shell, kernel_tree)?;

    let original_branch = get_current_branch(shell, kernel_tree)?;
    let target_branch = create_target_branch(shell, kernel_tree, config)?;

    let git_am_result = run_git_am(request, shell, kernel_tree, config);

    match git_am_result {
        Ok(_) => {
            let current_branch = if config.stay_on_applied_branch() {
                target_branch.clone()
            } else {
                switch_to_branch(shell, kernel_tree, &original_branch)?;
                original_branch
            };

            Ok(AppliedPatchset {
                message: format!(
                    " Patchset '{}' applied successfully!\n\n - Kernel Tree: '{}'\n\n - Base Branch: '{}'\n\n - Applied branch: '{}'\n\n - Current branch: '{}'",
                    request.patch_title,
                    kernel_tree.path(),
                    kernel_tree.branch(),
                    &target_branch,
                    current_branch
                ),
                applied_branch: target_branch,
            })
        }
        Err(e) => {
            switch_to_branch(shell, kernel_tree, &original_branch)?;
            Err(format!(" `git am` failed\n{}{}", &original_branch, e))
        }
    }
}

fn validate_kernel_tree<'a>(
    fs: &dyn FileSystemTrait,
    config: &'a ConfigSnapshot,
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

fn check_git_state(
    fs: &dyn FileSystemTrait,
    shell: &dyn ShellTrait,
    kernel_tree: &KernelTree,
) -> Result<(), String> {
    let kernel_tree_path = Path::new(kernel_tree.path());

    if fs.is_dir(&kernel_tree_path.join(".git/rebase-merge")) {
        return Err("rebase in progress. \nrun `git rebase --abort` before continuing".to_string());
    } else if fs.is_file(&kernel_tree_path.join(".git/MERGE_HEAD")) {
        return Err("merge in progress. \nrun `git merge --abort` before continuing".to_string());
    } else if fs.is_file(&kernel_tree_path.join(".git/BISECT_LOG")) {
        return Err("bisect in progress. \nrun `git bisect reset` before continuing".to_string());
    } else if fs.is_dir(&kernel_tree_path.join(".git/rebase-apply")) {
        return Err(
            "`git am` already in progress. \nrun `git am --abort` before continuing".to_string(),
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

fn get_current_branch(shell: &dyn ShellTrait, kernel_tree: &KernelTree) -> Result<String, String> {
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

fn switch_to_branch(
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

fn create_target_branch(
    shell: &dyn ShellTrait,
    kernel_tree: &KernelTree,
    config: &ConfigSnapshot,
) -> Result<String, String> {
    switch_to_branch(shell, kernel_tree, kernel_tree.branch())?;

    let target_branch_name = format!(
        "{}{}",
        config.git_am_branch_prefix(),
        Utc::now().format("%Y-%m-%d-%H-%M-%S")
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

fn run_git_am(
    request: &ApplyPatchsetRequest,
    shell: &dyn ShellTrait,
    kernel_tree: &KernelTree,
    config: &ConfigSnapshot,
) -> Result<(), String> {
    let mut git_am_cmd = ShellCommand::new("git")
        .arg("-C")
        .arg(kernel_tree.path())
        .args(["am", &request.patchset_path]);
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

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        sync::{Arc, Mutex},
    };

    use crate::{
        config::{ConfigSnapshot, ConfigState},
        infrastructure::{
            file_system::MockFileSystemTrait,
            shell::{MockShellTrait, ShellCommand, ShellOutput},
        },
    };

    use super::*;

    const KERNEL_TREE_PATH: &str = "/kernel";
    const BASE_BRANCH: &str = "main";
    const PATCHSET_PATH: &str = "/tmp/patchset.mbx";

    // `stay_on_applied_branch` is deliberately absent so the tests below
    // exercise the serde default (true) that existing config files inherit.
    fn config() -> ConfigSnapshot {
        serde_json::from_value::<ConfigState>(serde_json::json!({
            "kernel_trees": {
                "linux": {
                    "path": KERNEL_TREE_PATH,
                    "branch": BASE_BRANCH
                }
            },
            "target_kernel_tree": "linux",
            "git_am_options": "--signoff --3way",
            "git_am_branch_prefix": "patchset-"
        }))
        .expect("test config should deserialize")
        .to_snapshot()
    }

    fn config_stay_disabled() -> ConfigSnapshot {
        serde_json::from_value::<ConfigState>(serde_json::json!({
            "kernel_trees": {
                "linux": {
                    "path": KERNEL_TREE_PATH,
                    "branch": BASE_BRANCH
                }
            },
            "target_kernel_tree": "linux",
            "git_am_options": "--signoff --3way",
            "git_am_branch_prefix": "patchset-",
            "stay_on_applied_branch": false
        }))
        .expect("test config should deserialize")
        .to_snapshot()
    }

    fn config_without_target() -> ConfigSnapshot {
        ConfigState::default().to_snapshot()
    }

    fn request() -> ApplyPatchsetRequest {
        ApplyPatchsetRequest {
            patch_title: "[PATCH] test".to_string(),
            patchset_path: PATCHSET_PATH.to_string(),
        }
    }

    fn output(
        stdout: impl Into<Vec<u8>>,
        stderr: impl Into<Vec<u8>>,
        success: bool,
    ) -> ShellOutput {
        ShellOutput {
            stdout: stdout.into(),
            stderr: stderr.into(),
            success,
        }
    }

    fn clean_fs() -> MockFileSystemTrait {
        let mut fs = MockFileSystemTrait::new();
        fs.expect_is_dir()
            .returning(|path| matches!(path.to_str(), Some("/kernel") | Some("/kernel/.git")));
        fs.expect_is_file().returning(|_| false);
        fs
    }

    fn shell_with_outputs(
        outputs: Vec<ShellOutput>,
    ) -> (MockShellTrait, Arc<Mutex<Vec<Vec<String>>>>) {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let outputs = Arc::new(Mutex::new(VecDeque::from(outputs)));
        let mut shell = MockShellTrait::new();
        let calls_for_execute = Arc::clone(&calls);
        let outputs_for_execute = Arc::clone(&outputs);
        shell.expect_execute().returning(move |cmd| {
            calls_for_execute.lock().unwrap().push(command_parts(cmd));
            Ok(outputs_for_execute
                .lock()
                .unwrap()
                .pop_front()
                .expect("test should provide one output per shell command"))
        });
        (shell, calls)
    }

    fn command_parts(cmd: &ShellCommand) -> Vec<String> {
        let mut parts = vec![cmd.program.clone()];
        parts.extend(cmd.args.clone());
        parts
    }

    fn command(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|part| part.to_string()).collect()
    }

    #[test]
    fn apply_success_stays_on_applied_branch_by_default() {
        let fs = clean_fs();
        let (shell, calls) = shell_with_outputs(vec![
            output("", "", true),
            output("", "", true),
            output("feature\n", "", true),
            output("", "", true),
            output("", "", true),
            output("", "", true),
        ]);

        let applied = apply_patchset(&request(), &fs, &shell, &config()).unwrap();

        assert!(applied.applied_branch.starts_with("patchset-"));
        assert!(applied
            .message
            .contains("Patchset '[PATCH] test' applied successfully"));
        assert!(applied.message.contains("Applied branch: 'patchset-"));
        assert!(applied
            .message
            .contains(&format!("Current branch: '{}'", applied.applied_branch)));
        let calls = calls.lock().unwrap();
        assert_eq!(6, calls.len());
        assert_eq!(
            &calls[0],
            &command(&["git", "-C", KERNEL_TREE_PATH, "status", "--porcelain"])
        );
        assert_eq!(
            &calls[1],
            &command(&[
                "git",
                "-C",
                KERNEL_TREE_PATH,
                "show-ref",
                "--verify",
                "--quiet",
                "refs/heads/main"
            ])
        );
        assert_eq!(
            &calls[2],
            &command(&[
                "git",
                "-C",
                KERNEL_TREE_PATH,
                "rev-parse",
                "--abbrev-ref",
                "HEAD"
            ])
        );
        assert_eq!(
            &calls[3],
            &command(&["git", "-C", KERNEL_TREE_PATH, "switch", BASE_BRANCH])
        );
        assert_eq!(
            &calls[4][0..5],
            command(&["git", "-C", KERNEL_TREE_PATH, "checkout", "-b"]).as_slice()
        );
        assert!(calls[4][5].starts_with("patchset-"));
        assert_eq!(
            &calls[5],
            &command(&[
                "git",
                "-C",
                KERNEL_TREE_PATH,
                "am",
                PATCHSET_PATH,
                "--signoff",
                "--3way"
            ])
        );
    }

    #[test]
    fn apply_success_switches_back_when_stay_disabled() {
        let fs = clean_fs();
        let (shell, calls) = shell_with_outputs(vec![
            output("", "", true),
            output("", "", true),
            output("feature\n", "", true),
            output("", "", true),
            output("", "", true),
            output("", "", true),
            output("", "", true),
        ]);

        let applied = apply_patchset(&request(), &fs, &shell, &config_stay_disabled()).unwrap();

        assert!(applied.message.contains("Current branch: 'feature'"));
        let calls = calls.lock().unwrap();
        assert_eq!(7, calls.len());
        assert_eq!(
            &calls[6],
            &command(&["git", "-C", KERNEL_TREE_PATH, "switch", "feature"])
        );
    }

    #[test]
    fn dirty_worktree_rejects_before_branch_creation() {
        let fs = clean_fs();
        let (shell, calls) = shell_with_outputs(vec![output(" M file.rs\n", "", true)]);

        let result = apply_patchset(&request(), &fs, &shell, &config()).unwrap_err();

        assert!(result.contains("there are staged and/or unstaged changes"));
        assert_eq!(1, calls.lock().unwrap().len());
    }

    #[test]
    fn missing_target_kernel_tree_rejects_before_shell_commands() {
        let mut fs = MockFileSystemTrait::new();
        fs.expect_is_dir().times(0);
        fs.expect_is_file().times(0);
        let mut shell = MockShellTrait::new();
        shell.expect_execute().times(0);

        let result = apply_patchset(&request(), &fs, &shell, &config_without_target()).unwrap_err();

        assert_eq!("target kernel tree unset", result);
    }

    #[test]
    fn invalid_configured_branch_rejects_before_branch_creation() {
        let fs = clean_fs();
        let (shell, calls) = shell_with_outputs(vec![
            output("", "", true),
            output("", "missing branch", false),
        ]);

        let result = apply_patchset(&request(), &fs, &shell, &config()).unwrap_err();

        assert!(result.contains("invalid branch 'main'"));
        assert_eq!(2, calls.lock().unwrap().len());
    }

    #[test]
    fn failed_git_am_aborts_and_switches_back() {
        for config in [config(), config_stay_disabled()] {
            let fs = clean_fs();
            let (shell, calls) = shell_with_outputs(vec![
                output("", "", true),
                output("", "", true),
                output("feature\n", "", true),
                output("", "", true),
                output("", "", true),
                output("", "apply failed", false),
                output("", "", true),
                output("", "", true),
            ]);

            let result = apply_patchset(&request(), &fs, &shell, &config).unwrap_err();

            assert!(result.contains("`git am` failed"));
            assert!(result.contains("feature"));
            assert!(result.contains("apply failed"));
            let calls = calls.lock().unwrap();
            assert_eq!(
                &calls[6],
                &command(&["git", "-C", KERNEL_TREE_PATH, "am", "--abort"])
            );
            assert_eq!(
                &calls[7],
                &command(&["git", "-C", KERNEL_TREE_PATH, "switch", "feature"])
            );
        }
    }
}
