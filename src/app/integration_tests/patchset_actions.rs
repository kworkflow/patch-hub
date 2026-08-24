use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{Arc, Mutex},
};

use tokio::{spawn, sync::mpsc};

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
        file_system::{FileSystemError, MockFileSystemTrait},
        shell::{MockShellTrait, ShellCommand, ShellOutput},
    },
    kw::history::MockKwHistoryStore,
    lore::application::{
        cache::BootstrapLoreData, handle::LoreApiHandle, messages::LoreApiMessage,
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

type ReviewedState = HashMap<String, HashSet<usize>>;
type SharedReviewedState = Arc<Mutex<Option<ReviewedState>>>;

#[tokio::test]
async fn apply_success_sets_success_popup_and_resets_apply_action() {
    let (shell, _calls) = shell_with_outputs(vec![
        output("", "", true),
        output("", "", true),
        output("feature\n", "", true),
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
            "Current branch: 'patchset-",
        ],
    );
}

#[tokio::test]
async fn apply_success_switches_back_when_stay_disabled() {
    let (shell, calls) = shell_with_outputs(vec![
        output("", "", true),
        output("", "", true),
        output("feature\n", "", true),
        output("", "", true),
        output("", "", true),
        output("", "", true),
        output("", "", true),
    ]);
    let mut app = app_with_details(
        clean_fs(),
        shell,
        lore_handle_with_persistence(),
        apply_details_state(),
        apply_config_stay_disabled(),
        history_store_allowing_writes(),
    );

    app.consolidate_patchset_actions().await.unwrap();

    assert_apply_action(&app, false);
    assert_info_popup_contains(
        app.state.popup.as_ref(),
        "Patchset Apply Success",
        &["Current branch: 'feature'"],
    );
    let calls = calls.lock().unwrap();
    assert_eq!(
        command(&["git", "-C", KERNEL_TREE_PATH, "switch", "feature"]),
        calls[6]
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

#[tokio::test]
async fn apply_success_records_apply_history() {
    let (shell, _calls) = shell_with_outputs(vec![
        output("", "", true),
        output("", "", true),
        output("feature\n", "", true),
        output("", "", true),
        output("", "", true),
        output("", "", true),
    ]);
    let mut kw_history = MockKwHistoryStore::new();
    kw_history
        .expect_record_apply()
        .times(1)
        .withf(|record| {
            record.message_id == "http://lore.kernel.org/test-list/1234-1-foo@bar.example"
                && record.kernel_tree_id == "linux"
                && record.tree_path == KERNEL_TREE_PATH
                && record.applied_branch.starts_with("patchset-")
                && record.base_branch == BASE_BRANCH
                && !record.applied_at.is_empty()
        })
        .returning(|_| Ok(()));
    let mut app = app_with_details(
        clean_fs(),
        shell,
        lore_handle_with_persistence(),
        apply_details_state(),
        apply_config(),
        kw_history,
    );

    app.consolidate_patchset_actions().await.unwrap();

    assert_info_popup_contains(
        app.state.popup.as_ref(),
        "Patchset Apply Success",
        &["applied successfully"],
    );
}

#[tokio::test]
async fn apply_success_with_history_write_failure_keeps_success_popup() {
    let (shell, _calls) = shell_with_outputs(vec![
        output("", "", true),
        output("", "", true),
        output("feature\n", "", true),
        output("", "", true),
        output("", "", true),
        output("", "", true),
    ]);
    let mut kw_history = MockKwHistoryStore::new();
    kw_history
        .expect_record_apply()
        .times(1)
        .returning(|_| Err(FileSystemError::IoError(std::io::Error::other("disk full"))));
    let mut app = app_with_details(
        clean_fs(),
        shell,
        lore_handle_with_persistence(),
        apply_details_state(),
        apply_config(),
        kw_history,
    );

    app.consolidate_patchset_actions().await.unwrap();

    assert_apply_action(&app, false);
    assert_info_popup_contains(
        app.state.popup.as_ref(),
        "Patchset Apply Success",
        &[
            "applied successfully",
            "was not recorded in the kw history",
            "disk full",
            "inspect or delete that file",
        ],
    );
}

#[tokio::test]
async fn apply_failure_does_not_record_history() {
    let (shell, _calls) = shell_with_outputs(vec![
        output("", "", true),
        output("", "", true),
        output("feature\n", "", true),
        output("", "", true),
        output("", "", true),
        output("", "apply failed", false),
        output("", "", true),
        output("", "", true),
    ]);
    let mut kw_history = MockKwHistoryStore::new();
    kw_history.expect_record_apply().times(0);
    let mut app = app_with_details(
        clean_fs(),
        shell,
        lore_handle_with_persistence(),
        apply_details_state(),
        apply_config(),
        kw_history,
    );

    app.consolidate_patchset_actions().await.unwrap();

    assert_info_popup_contains(app.state.popup.as_ref(), "Patchset Apply Fail", &[]);
}

#[tokio::test]
async fn reviewed_reply_success_records_persists_and_resets_reply_action() {
    let saved_reviewed = Arc::new(Mutex::new(None));
    let lore_api = reviewed_reply_lore_handle(Arc::clone(&saved_reviewed));
    let mut shell = MockShellTrait::new();
    shell
        .expect_execute()
        .withf(|cmd| cmd.program == "mktemp" && cmd.args == ["--directory"])
        .times(1)
        .returning(|_| Ok(output("/tmp/reviewed-reply\n", "", true)));
    shell
        .expect_spawn_interactive()
        .times(1)
        .returning(|_| Ok(true));
    let mut app = app_with_reviewed_reply_details(shell, lore_api);
    let message_id = selected_message_id(&app);

    app.consolidate_patchset_actions().await.unwrap();

    assert_reply_action_reset(&app);
    assert_eq!(
        HashSet::from([0]),
        app.state.user_state.reviewed_patchsets[&message_id]
    );
    let saved = saved_reviewed
        .lock()
        .unwrap()
        .clone()
        .expect("reviewed state should be persisted");
    assert_eq!(HashSet::from([0]), saved[&message_id]);
}

#[tokio::test]
async fn reviewed_reply_failure_does_not_record_failed_index() {
    let saved_reviewed = Arc::new(Mutex::new(None));
    let lore_api = reviewed_reply_lore_handle(Arc::clone(&saved_reviewed));
    let mut shell = MockShellTrait::new();
    shell
        .expect_execute()
        .withf(|cmd| cmd.program == "mktemp" && cmd.args == ["--directory"])
        .times(1)
        .returning(|_| Ok(output("/tmp/reviewed-reply\n", "", true)));
    shell
        .expect_spawn_interactive()
        .times(1)
        .returning(|_| Ok(false));
    let mut app = app_with_reviewed_reply_details(shell, lore_api);
    let message_id = selected_message_id(&app);

    app.consolidate_patchset_actions().await.unwrap();

    assert_reply_action_reset(&app);
    assert!(app.state.user_state.reviewed_patchsets[&message_id].is_empty());
    let saved = saved_reviewed
        .lock()
        .unwrap()
        .clone()
        .expect("reviewed state should be persisted");
    assert!(saved[&message_id].is_empty());
}

fn app_with_apply_details(fs: MockFileSystemTrait, shell: MockShellTrait) -> App {
    app_with_details(
        fs,
        shell,
        lore_handle_with_persistence(),
        apply_details_state(),
        apply_config(),
        history_store_allowing_writes(),
    )
}

fn app_with_reviewed_reply_details(shell: MockShellTrait, lore_api: LoreApiHandle) -> App {
    app_with_details(
        MockFileSystemTrait::new(),
        shell,
        lore_api,
        reviewed_reply_details_state(),
        apply_config(),
        MockKwHistoryStore::new(),
    )
}

fn app_with_details(
    fs: MockFileSystemTrait,
    shell: MockShellTrait,
    lore_api: LoreApiHandle,
    details: PatchsetDetailsState,
    config: ConfigSnapshot,
    kw_history: MockKwHistoryStore,
) -> App {
    let mut app = App::new(
        config,
        dummy_config_handle(),
        BootstrapLoreData {
            mailing_lists: vec![sample_mailing_list()],
            bookmarks: vec![],
            reviewed: Default::default(),
        },
        Box::new(fs),
        Box::new(shell),
        lore_api,
        dummy_render_handle(),
        Arc::new(kw_history),
    )
    .expect("app should build");

    app.state.navigation.current_screen = CurrentScreen::PatchsetDetails;
    app.state.lore.details = Some(details);
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

fn reviewed_reply_details_state() -> PatchsetDetailsState {
    let mut details = PatchsetDetailsState::from_rendered_preview(
        sample_patch(),
        sample_patchset_details(),
        sample_rendered_preview(),
        false,
        CurrentScreen::LatestPatchsets,
    );
    details.toggle_reply_with_reviewed_by_action(false);
    details
}

fn reviewed_reply_lore_handle(saved_reviewed: SharedReviewedState) -> LoreApiHandle {
    let (tx, mut rx) = mpsc::channel(8);
    spawn(async move {
        while let Some(message) = rx.recv().await {
            match message {
                LoreApiMessage::SaveBookmarks { reply, .. } => {
                    reply.send(Ok(())).ok();
                }
                LoreApiMessage::GetGitSignature { reply, .. } => {
                    reply
                        .send(Ok((
                            "Reviewer".to_string(),
                            "reviewer@example.com".to_string(),
                        )))
                        .ok();
                }
                LoreApiMessage::PrepareReplyCommands { reply, .. } => {
                    reply
                        .send(Ok(vec![
                            ShellCommand::new("git").args(["send-email", "--annotate"])
                        ]))
                        .ok();
                }
                LoreApiMessage::SaveReviewed { reviewed, reply } => {
                    *saved_reviewed.lock().unwrap() = Some(reviewed);
                    reply.send(Ok(())).ok();
                }
                LoreApiMessage::Shutdown => break,
                other => panic!("unexpected lore message: {}", other.name()),
            }
        }
    });
    LoreApiHandle::new(tx)
}

// `stay_on_applied_branch` is deliberately absent so the tests exercise the
// serde default (true) that existing config files inherit.
fn history_store_allowing_writes() -> MockKwHistoryStore {
    let mut store = MockKwHistoryStore::new();
    store.expect_record_apply().returning(|_| Ok(()));
    store
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

fn apply_config_stay_disabled() -> ConfigSnapshot {
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

fn selected_message_id(app: &App) -> String {
    app.state
        .lore
        .details
        .as_ref()
        .expect("details should remain loaded")
        .representative_patch
        .message_id()
        .href
        .clone()
}

fn assert_reply_action_reset(app: &App) {
    let details = app
        .state
        .lore
        .details
        .as_ref()
        .expect("details should remain loaded");
    assert_eq!(
        Some(&false),
        details
            .patchset_actions
            .get(&PatchsetAction::ReplyWithReviewedBy)
    );
    assert_eq!(vec![false], details.patches_to_reply);
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
