use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use crate::{
    app::{
        popup::AppPopup,
        screens::{
            details_actions::{PatchsetAction, PatchsetDetailsState},
            CurrentScreen,
        },
        App,
    },
    config::{ConfigSnapshot, ConfigState},
    infrastructure::{
        file_system::MockFileSystemTrait,
        shell::{MockShellTrait, ShellCommand, ShellOutput},
    },
};

use super::helpers::{
    app_harness::{dummy_config_handle, dummy_render_handle},
    lore::{
        lore_handle_with_persistence, sample_mailing_list, sample_patch, sample_patchset_details,
    },
    render::sample_rendered_preview,
};

const KERNEL_TREE_PATH: &str = "/kernel";
const BASE_BRANCH: &str = "main";

#[tokio::test]
async fn apply_success_sets_success_popup_and_resets_apply_action() {
    let (shell, _calls) = shell_with_outputs(vec![
        output("", "", true),
        output("", "", true),
        output("feature\n", "", true),
        output("", "", true),
        output("", "", true),
        output("", "", true),
        output("", "", true),
    ]);
    let mut app = app_with_apply_details(clean_fs(), shell);

    app.consolidate_patchset_actions().await.unwrap();

    assert_apply_action(&app, false);
    assert_info_popup_contains(
        app.state.popup.as_ref(),
        "Patchset Apply Success",
        &[
            "applied successfully",
            "Kernel Tree: '/kernel'",
            "Applied branch: 'patchset-",
        ],
    );
}

#[tokio::test]
async fn apply_failure_sets_failure_popup_and_resets_apply_action() {
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
    let mut app = app_with_apply_details(clean_fs(), shell);

    app.consolidate_patchset_actions().await.unwrap();

    assert_apply_action(&app, false);
    assert_info_popup_contains(
        app.state.popup.as_ref(),
        "Patchset Apply Fail",
        &["`git am` failed", "feature", "apply failed"],
    );
    let calls = calls.lock().unwrap();
    assert_eq!(
        command(&["git", "-C", KERNEL_TREE_PATH, "am", "--abort"]),
        calls[6]
    );
}

fn app_with_apply_details(fs: MockFileSystemTrait, shell: MockShellTrait) -> App {
    let mut app = App::new(
        apply_config(),
        dummy_config_handle(),
        crate::lore::application::cache::BootstrapLoreData {
            mailing_lists: vec![sample_mailing_list()],
            bookmarks: vec![],
            reviewed: Default::default(),
        },
        Box::new(fs),
        Box::new(shell),
        lore_handle_with_persistence(),
        dummy_render_handle(),
    )
    .expect("app should build");

    app.state.navigation.current_screen = CurrentScreen::PatchsetDetails;
    app.state.lore.details = Some(apply_details_state());
    app
}

fn apply_details_state() -> PatchsetDetailsState {
    let mut details = PatchsetDetailsState::from_rendered_preview(
        sample_patch(),
        sample_patchset_details(),
        sample_rendered_preview(),
        false,
        CurrentScreen::LatestPatchsets,
    );
    details.toggle_apply_action();
    details
}

fn apply_config() -> ConfigSnapshot {
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

fn clean_fs() -> MockFileSystemTrait {
    let mut fs = MockFileSystemTrait::new();
    fs.expect_is_dir()
        .returning(|path| matches!(path.to_str(), Some("/kernel") | Some("/kernel/.git")));
    fs.expect_is_file().returning(|_| false);
    fs
}

fn shell_with_outputs(outputs: Vec<ShellOutput>) -> (MockShellTrait, Arc<Mutex<Vec<Vec<String>>>>) {
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

fn output(stdout: impl Into<Vec<u8>>, stderr: impl Into<Vec<u8>>, success: bool) -> ShellOutput {
    ShellOutput {
        stdout: stdout.into(),
        stderr: stderr.into(),
        success,
    }
}

fn command_parts(cmd: &ShellCommand) -> Vec<String> {
    let mut parts = vec![cmd.program.clone()];
    parts.extend(cmd.args.clone());
    parts
}

fn command(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|part| part.to_string()).collect()
}

fn assert_apply_action(app: &App, expected: bool) {
    let details = app
        .state
        .lore
        .details
        .as_ref()
        .expect("details should remain loaded");
    assert_eq!(
        Some(&expected),
        details.patchset_actions.get(&PatchsetAction::Apply)
    );
}

fn assert_info_popup_contains(popup: Option<&AppPopup>, expected_title: &str, expected: &[&str]) {
    let Some(AppPopup::Info { title, body, .. }) = popup else {
        panic!("expected info popup");
    };

    assert_eq!(expected_title, title);
    for fragment in expected {
        assert!(
            body.contains(fragment),
            "expected popup body to contain {fragment:?}, got {body:?}"
        );
    }
}
