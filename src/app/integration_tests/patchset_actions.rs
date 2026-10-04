use std::{
    collections::HashSet,
    fs, io,
    sync::{Arc, Mutex},
    time::Duration,
};

use tokio::time;

use super::helpers::lore::lore_handle_with_persistence;
use crate::{
    infrastructure::{file_system::FileSystemError, process::FakeProcess, shell::MockShellTrait},
    kw::{history::MockKwHistoryStore, status::KwJobStatus},
};

mod helpers {
    use super::super::helpers::{
        app_harness::{dummy_config_handle, dummy_render_handle},
        lore::{
            lore_handle_with_persistence, sample_mailing_list, sample_patch,
            sample_patchset_details,
        },
        render::sample_rendered_preview,
    };
    use crate::{
        app::{
            models::popup::AppPopup,
            screens::{
                details_actions::{PatchsetAction, PatchsetDetailsState},
                CurrentScreen,
            },
            App,
        },
        config::{ConfigSnapshot, ConfigState},
        infrastructure::{
            env::MockEnvTrait,
            file_system::{FileSystemError, MockFileSystemTrait},
            process::FakeProcess,
            shell::{MockShellTrait, ShellCommand, ShellOutput},
        },
        kw::{actor::KwActor, history::MockKwHistoryStore, messages::StartRequest},
        lore::application::{
            handle::LoreApiHandle, messages::LoreApiMessage, models::cache::BootstrapLoreData,
        },
    };
    use std::{
        collections::{HashMap, HashSet, VecDeque},
        env, fs, io,
        path::PathBuf,
        process,
        sync::{Arc, Mutex},
    };

    use tokio::{spawn, sync::mpsc};

    pub type ReviewedState = HashMap<String, HashSet<usize>>;
    pub type SharedReviewedState = Arc<Mutex<Option<ReviewedState>>>;

    pub const KERNEL_TREE_PATH: &str = "/kernel";
    pub const BASE_BRANCH: &str = "main";

    pub fn app_with_apply_details(fs: MockFileSystemTrait, shell: MockShellTrait) -> App {
        app_with_details(
            fs,
            shell,
            lore_handle_with_persistence(),
            apply_details_state(),
            apply_config(),
            history_store_allowing_writes(),
        )
    }

    /// Shuts down the KwActor the app was wired with, instead of relying on
    /// the test runtime aborting it at drop.
    pub async fn shutdown_kw(app: &App) {
        if let Some(kw) = &app.services.kw {
            kw.shutdown().await;
        }
    }

    pub fn app_with_reviewed_reply_details(shell: MockShellTrait, lore_api: LoreApiHandle) -> App {
        app_with_details(
            MockFileSystemTrait::new(),
            shell,
            lore_api,
            reviewed_reply_details_state(),
            apply_config(),
            MockKwHistoryStore::new(),
        )
    }

    pub fn app_with_details(
        fs: MockFileSystemTrait,
        shell: MockShellTrait,
        lore_api: LoreApiHandle,
        details: PatchsetDetailsState,
        config: ConfigSnapshot,
        kw_history: MockKwHistoryStore,
    ) -> App {
        app_with_details_and_kw(
            fs,
            shell,
            lore_api,
            details,
            config,
            kw_history,
            MockShellTrait::new(),
            MockFileSystemTrait::new(),
            MockEnvTrait::new(),
            Arc::new(FakeProcess::new()),
            PathBuf::from("/tmp/patch-hub-test-kw-logs"),
        )
    }

    /// Variant of [`app_with_details`] whose kw actor gets its own mocks,
    /// process double, and log dir, for tests that start jobs through the
    /// app's kw handle.
    #[expect(clippy::too_many_arguments)]
    pub fn app_with_details_and_kw(
        fs: MockFileSystemTrait,
        shell: MockShellTrait,
        lore_api: LoreApiHandle,
        details: PatchsetDetailsState,
        config: ConfigSnapshot,
        kw_history: MockKwHistoryStore,
        kw_shell: MockShellTrait,
        kw_fs: MockFileSystemTrait,
        kw_env: MockEnvTrait,
        kw_process: Arc<FakeProcess>,
        kw_log_dir: PathBuf,
    ) -> App {
        // Apply history is recorded through the real actor wrapping the mock
        // store, mirroring production wiring.
        let kw = KwActor::spawn(
            Arc::new(kw_history),
            kw_process,
            Arc::new(kw_shell),
            Arc::new(kw_fs),
            Arc::new(kw_env),
            kw_log_dir,
        );
        let mut app = App::new(
            config,
            dummy_config_handle(),
            BootstrapLoreData {
                mailing_lists: vec![sample_mailing_list()],
                bookmarks: vec![],
                reviewed: Default::default(),
            },
            Arc::new(fs),
            Box::new(shell),
            lore_api,
            dummy_render_handle(),
            Arc::new(MockKwHistoryStore::new()),
            Some(kw),
        )
        .expect("app should build");

        app.state.navigation.current_screen = CurrentScreen::PatchsetDetails;
        app.state.lore.details = Some(details);
        app
    }

    pub fn apply_details_state() -> PatchsetDetailsState {
        let mut details = PatchsetDetailsState::build_from_rendered_preview(
            sample_patch(),
            sample_patchset_details(),
            sample_rendered_preview(),
            false,
            CurrentScreen::LatestPatchsets,
        );
        details.toggle_apply_action();
        details
    }

    pub fn reviewed_reply_details_state() -> PatchsetDetailsState {
        let mut details = PatchsetDetailsState::build_from_rendered_preview(
            sample_patch(),
            sample_patchset_details(),
            sample_rendered_preview(),
            false,
            CurrentScreen::LatestPatchsets,
        );
        details.toggle_reply_with_reviewed_by_action(false);
        details
    }

    pub fn reviewed_reply_lore_handle(saved_reviewed: SharedReviewedState) -> LoreApiHandle {
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
                        *saved_reviewed.lock().expect("saved reviewed locks") = Some(reviewed);
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
    pub fn history_store_allowing_writes() -> MockKwHistoryStore {
        let mut store = MockKwHistoryStore::new();
        store
            .expect_record_apply()
            .withf(|record| {
                record.message_id == "http://lore.kernel.org/test-list/1234-1-foo@bar.example"
            })
            .times(0..=1)
            .returning(|_| Ok(()));
        store
    }

    pub fn apply_config() -> ConfigSnapshot {
        ConfigSnapshot::from(
            &serde_json::from_value::<ConfigState>(serde_json::json!({
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
            .expect("test config should deserialize"),
        )
    }

    pub fn apply_config_stay_disabled() -> ConfigSnapshot {
        ConfigSnapshot::from(
            &serde_json::from_value::<ConfigState>(serde_json::json!({
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
            .expect("test config should deserialize"),
        )
    }

    pub fn clean_fs() -> MockFileSystemTrait {
        let mut fs = MockFileSystemTrait::new();
        fs.expect_is_dir()
            .withf(|path| {
                path == std::path::Path::new("/kernel")
                    || path == std::path::Path::new("/kernel/.git")
                    || path == std::path::Path::new("/kernel/.git/rebase-apply")
                    || path == std::path::Path::new("/kernel/.git/rebase-merge")
            })
            .times(0..=4)
            .returning(|path| matches!(path.to_str(), Some("/kernel") | Some("/kernel/.git")));
        fs.expect_is_file()
            .withf(|path| {
                path == std::path::Path::new("/kernel/.git/BISECT_LOG")
                    || path == std::path::Path::new("/kernel/.git/MERGE_HEAD")
            })
            .times(0..=2)
            .returning(|_| false);
        fs
    }

    pub fn shell_with_outputs(
        outputs: Vec<ShellOutput>,
    ) -> (MockShellTrait, Arc<Mutex<Vec<Vec<String>>>>) {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let outputs = Arc::new(Mutex::new(VecDeque::from(outputs)));
        let mut shell = MockShellTrait::new();
        let calls_for_execute = Arc::clone(&calls);
        let outputs_for_execute = Arc::clone(&outputs);
        shell
            .expect_execute()
            .withf(|cmd| {
                cmd.program == "git"
                    && (cmd.args == ["-C", "/kernel", "status", "--porcelain"]
                        || cmd.args
                            == [
                                "-C",
                                "/kernel",
                                "show-ref",
                                "--verify",
                                "--quiet",
                                "refs/heads/main",
                            ]
                        || cmd.args == ["-C", "/kernel", "rev-parse", "--abbrev-ref", "HEAD"]
                        || cmd.args == ["-C", "/kernel", "switch", "main"]
                        || (cmd.args.len() == 5
                            && cmd.args[..4] == ["-C", "/kernel", "checkout", "-b"]
                            && cmd.args[4].starts_with("patchset-"))
                        || cmd.args
                            == [
                                "-C",
                                "/kernel",
                                "am",
                                "/tmp/patchset.mbx",
                                "--signoff",
                                "--3way",
                            ]
                        || cmd.args == ["-C", "/kernel", "am", "--abort"]
                        || cmd.args == ["-C", "/kernel", "switch", "feature"]
                        || (cmd.args.len() == 5
                            && cmd.args[..4] == ["-C", "/kernel", "branch", "-D"]
                            && cmd.args[4].starts_with("patchset-")))
            })
            .times(0..=9)
            .returning(move |cmd| {
                calls_for_execute
                    .lock()
                    .expect("calls for execute locks")
                    .push(command_parts(cmd));
                Ok(outputs_for_execute
                    .lock()
                    .expect("outputs for execute locks")
                    .pop_front()
                    .expect("test should provide one output per shell command"))
            });
        (shell, calls)
    }

    pub fn output(
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

    pub fn command_parts(cmd: &ShellCommand) -> Vec<String> {
        let mut parts = vec![cmd.program.clone()];
        parts.extend(cmd.args.clone());
        parts
    }

    /// kw-actor mocks for tests that start jobs through the app's handle:
    /// kw on PATH at the verified version, clean git state, a ready kernel
    /// tree — the happy path through the actor's start probes.
    pub fn kw_actor_shell() -> MockShellTrait {
        let mut shell = MockShellTrait::new();
        shell
            .expect_execute()
            .withf(|cmd| {
                cmd.program == "kw" && cmd.args == ["--version"]
                    || cmd.program == "git"
                        && cmd.args
                            == [
                                "-C",
                                "/kernel",
                                "status",
                                "--porcelain",
                                "--untracked-files=no",
                            ]
                    || cmd.program == "git"
                        && cmd.args == ["-C", "/kernel", "branch", "--show-current"]
                    || cmd.program == "git"
                        && cmd.args
                            == [
                                "-C",
                                "/kernel",
                                "switch",
                                "--",
                                "patchset-2026-08-20-15-00-00",
                            ]
            })
            .times(4)
            .returning(|cmd| {
                let stdout = match cmd.program.as_str() {
                    "kw" => b"kw, version 0.10.0\n".to_vec(),
                    _ if cmd.args.iter().any(|arg| arg == "status") => Vec::new(),
                    _ => b"main\n".to_vec(),
                };
                Ok(ShellOutput {
                    stdout,
                    stderr: Vec::new(),
                    success: true,
                })
            });
        shell
    }

    pub fn kw_actor_fs() -> MockFileSystemTrait {
        let mut fs = MockFileSystemTrait::new();
        fs.expect_is_dir()
            .withf(|path| {
                path == std::path::Path::new("/kernel")
                    || path == std::path::Path::new("/kernel/.kw")
                    || path == std::path::Path::new("/kernel/Documentation")
                    || path == std::path::Path::new("/kernel/arch")
                    || path == std::path::Path::new("/kernel/drivers")
                    || path == std::path::Path::new("/kernel/fs")
                    || path == std::path::Path::new("/kernel/include")
                    || path == std::path::Path::new("/kernel/init")
                    || path == std::path::Path::new("/kernel/ipc")
                    || path == std::path::Path::new("/kernel/kernel")
                    || path == std::path::Path::new("/kernel/lib")
                    || path == std::path::Path::new("/kernel/scripts")
            })
            .times(12)
            .returning(|_| true);
        fs.expect_is_file()
            .withf(|path| {
                path == std::path::Path::new("/kernel/.config")
                    || path == std::path::Path::new("/kernel/.kw/env.current")
                    || path == std::path::Path::new("/kernel/COPYING")
                    || path == std::path::Path::new("/kernel/CREDITS")
                    || path == std::path::Path::new("/kernel/Kbuild")
                    || path == std::path::Path::new("/kernel/Makefile")
                    || path == std::path::Path::new("/kernel/README")
            })
            .times(7)
            .returning(|path| !path.ends_with(".kw/env.current"));
        fs.expect_exists()
            .withf(|path| path == std::path::Path::new("/kernel/MAINTAINERS"))
            .times(1)
            .returning(|_| true);
        fs.expect_read_to_string()
            .withf(|path| {
                path == std::path::Path::new("/kernel/.kw/build.config")
                    || path == std::path::Path::new("/kernel/include/config/kernel.release")
            })
            .times(1..=2)
            .returning(|_| {
                Err(FileSystemError::IoError(io::Error::new(
                    io::ErrorKind::NotFound,
                    "missing",
                )))
            });
        fs.expect_read_dir()
            .withf(|path| path == std::path::Path::new("/kernel/arch"))
            .times(0..=1)
            .returning(|_| {
                Err(FileSystemError::IoError(io::Error::new(
                    io::ErrorKind::NotFound,
                    "missing",
                )))
            });
        fs.expect_create_dir_all()
            .withf(|path| path.starts_with(std::env::temp_dir()))
            .times(1)
            .returning(|_| Ok(()));
        fs
    }

    pub fn kw_actor_env() -> MockEnvTrait {
        let mut env = MockEnvTrait::new();
        env.expect_which()
            .withf(|name| name == "kw")
            .times(1)
            .returning(|_| true);
        env
    }

    /// A real, unique directory: FakeProcess creates the job's log file on
    /// spawn, even though the fs trait is mocked.
    pub fn kw_log_dir(test_name: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!(
            "patch-hub-app-kw-logs-{}-{}",
            test_name,
            process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("dir creates");
        dir
    }

    pub fn kw_start_request() -> StartRequest {
        StartRequest {
            kernel_tree_id: "linux".to_string(),
            tree: serde_json::from_value(serde_json::json!({
                "path": KERNEL_TREE_PATH,
                "branch": BASE_BRANCH
            }))
            .expect("kernel tree should deserialize"),
            branch: "patchset-2026-08-20-15-00-00".to_string(),
            extra_args: Vec::new(),
            deploy: None,
        }
    }

    pub fn command(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|part| part.to_string()).collect()
    }

    pub fn assert_apply_action(app: &App, expected: bool) {
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

    pub fn selected_message_id(app: &App) -> String {
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

    pub fn assert_reply_action_reset(app: &App) {
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

    pub fn assert_info_popup_contains(
        popup: Option<&AppPopup>,
        expected_title: &str,
        expected: &[&str],
    ) {
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
}
pub use helpers::*;

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

    app.consolidate_patchset_actions()
        .await
        .expect("patchset actions consolidate");

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
    shutdown_kw(&app).await;
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

    app.consolidate_patchset_actions()
        .await
        .expect("patchset actions consolidate");

    assert_apply_action(&app, false);
    assert_info_popup_contains(
        app.state.popup.as_ref(),
        "Patchset Apply Success",
        &["Current branch: 'feature'"],
    );
    {
        let calls = calls.lock().expect("calls locks");
        assert_eq!(
            command(&["git", "-C", KERNEL_TREE_PATH, "switch", "feature"]),
            calls[6]
        );
    }
    shutdown_kw(&app).await;
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
        output("", "", true),
    ]);
    let mut app = app_with_apply_details(clean_fs(), shell);

    app.consolidate_patchset_actions()
        .await
        .expect("patchset actions consolidate");

    assert_apply_action(&app, false);
    assert_info_popup_contains(
        app.state.popup.as_ref(),
        "Patchset Apply Fail",
        &["`git am` failed (back on branch 'feature')", "apply failed"],
    );
    {
        let calls = calls.lock().expect("calls locks");
        assert_eq!(
            command(&["git", "-C", KERNEL_TREE_PATH, "am", "--abort"]),
            calls[6]
        );
        assert_eq!(
            &calls[8][..5],
            command(&["git", "-C", KERNEL_TREE_PATH, "branch", "-D"])
        );
    }
    shutdown_kw(&app).await;
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

    app.consolidate_patchset_actions()
        .await
        .expect("patchset actions consolidate");

    assert_info_popup_contains(
        app.state.popup.as_ref(),
        "Patchset Apply Success",
        &["applied successfully"],
    );
    shutdown_kw(&app).await;
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
        .withf(|record| {
            record.message_id == "http://lore.kernel.org/test-list/1234-1-foo@bar.example"
        })
        .times(1)
        .returning(|_| Err(FileSystemError::IoError(io::Error::other("disk full"))));
    let mut app = app_with_details(
        clean_fs(),
        shell,
        lore_handle_with_persistence(),
        apply_details_state(),
        apply_config(),
        kw_history,
    );

    app.consolidate_patchset_actions()
        .await
        .expect("patchset actions consolidate");

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
    shutdown_kw(&app).await;
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
        output("", "", true),
    ]);
    let mut kw_history = MockKwHistoryStore::new();
    kw_history.expect_record_apply().withf(|_| true).times(0);
    let mut app = app_with_details(
        clean_fs(),
        shell,
        lore_handle_with_persistence(),
        apply_details_state(),
        apply_config(),
        kw_history,
    );

    app.consolidate_patchset_actions()
        .await
        .expect("patchset actions consolidate");

    assert_info_popup_contains(app.state.popup.as_ref(), "Patchset Apply Fail", &[]);
    shutdown_kw(&app).await;
}

#[tokio::test]
async fn apply_is_blocked_while_a_kw_job_runs() {
    // No scripted outputs: any apply-time git call fails the test.
    let (shell, calls) = shell_with_outputs(vec![]);
    let log_dir = kw_log_dir("apply-blocked");
    let mut app = app_with_details_and_kw(
        clean_fs(),
        shell,
        lore_handle_with_persistence(),
        apply_details_state(),
        apply_config(),
        history_store_allowing_writes(),
        kw_actor_shell(),
        kw_actor_fs(),
        kw_actor_env(),
        Arc::new(FakeProcess::new()),
        log_dir.clone(),
    );

    let kw = app.services.kw.as_ref().expect("kw handle is available");
    kw.start_build(kw_start_request())
        .await
        .expect("build starts");

    app.consolidate_patchset_actions()
        .await
        .expect("patchset actions consolidate");

    assert_apply_action(&app, false);
    assert_info_popup_contains(
        app.state.popup.as_ref(),
        "Patchset Apply Blocked",
        &["kw job is running", "Wait for the job to finish"],
    );
    assert!(calls.lock().expect("calls locks").is_empty());

    shutdown_kw(&app).await;
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
}

#[tokio::test]
async fn apply_is_allowed_again_after_the_job_finishes() {
    let (shell, calls) = shell_with_outputs(vec![
        output("", "", true),
        output("", "", true),
        output("feature\n", "", true),
        output("", "", true),
        output("", "", true),
        output("", "", true),
    ]);
    let log_dir = kw_log_dir("apply-after-job");
    let process = Arc::new(FakeProcess::new());
    let mut store = MockKwHistoryStore::new();
    store
        .expect_record_apply()
        .withf(|record| {
            record.message_id == "http://lore.kernel.org/test-list/1234-1-foo@bar.example"
        })
        .times(1)
        .returning(|_| Ok(()));
    store
        .expect_apply_record_for_branch()
        .withf(|tree, branch| tree == "linux" && branch == "patchset-2026-08-20-15-00-00")
        .times(1)
        .returning(|_, _| Ok(None));
    store
        .expect_record_build()
        .withf(|record| record.branch == "patchset-2026-08-20-15-00-00")
        .times(1)
        .returning(|_| Ok(()));
    let mut app = app_with_details_and_kw(
        clean_fs(),
        shell,
        lore_handle_with_persistence(),
        apply_details_state(),
        apply_config(),
        store,
        kw_actor_shell(),
        kw_actor_fs(),
        kw_actor_env(),
        process.clone(),
        log_dir.clone(),
    );

    let kw = app
        .services
        .kw
        .as_ref()
        .expect("kw handle is available")
        .clone();
    kw.start_build(kw_start_request())
        .await
        .expect("build starts");

    // While the job runs, the apply is blocked.
    app.consolidate_patchset_actions()
        .await
        .expect("patchset actions consolidate");
    assert_info_popup_contains(app.state.popup.as_ref(), "Patchset Apply Blocked", &[]);
    assert!(calls.lock().expect("calls locks").is_empty());

    // Once the job finishes, the same apply goes through.
    process.last_child().finish(0);
    let mut watch = kw.watch_status().await.expect("status watch opens");
    time::timeout(Duration::from_secs(10), async {
        loop {
            if matches!(watch.borrow().job, KwJobStatus::Succeeded { .. }) {
                break;
            }
            watch.changed().await.expect("watch notifies");
        }
    })
    .await
    .expect("job must reach a terminal state");

    let details = app
        .state
        .lore
        .details
        .as_mut()
        .expect("details should remain loaded");
    details.toggle_apply_action();
    app.consolidate_patchset_actions()
        .await
        .expect("patchset actions consolidate");

    assert_apply_action(&app, false);
    assert_info_popup_contains(
        app.state.popup.as_ref(),
        "Patchset Apply Success",
        &["applied successfully"],
    );
    assert_eq!(6, calls.lock().expect("calls locks").len());

    shutdown_kw(&app).await;
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
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
        .withf(|cmd| cmd.program == "git" && cmd.args == ["send-email", "--annotate"])
        .times(1)
        .returning(|_| Ok(true));
    let mut app = app_with_reviewed_reply_details(shell, lore_api);
    let message_id = selected_message_id(&app);

    app.consolidate_patchset_actions()
        .await
        .expect("patchset actions consolidate");

    assert_reply_action_reset(&app);
    assert_eq!(
        HashSet::from([0]),
        app.state.user_state.reviewed_patchsets[&message_id]
    );
    let saved = saved_reviewed
        .lock()
        .expect("saved reviewed locks")
        .clone()
        .expect("reviewed state should be persisted");
    assert_eq!(HashSet::from([0]), saved[&message_id]);
    shutdown_kw(&app).await;
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
        .withf(|cmd| cmd.program == "git" && cmd.args == ["send-email", "--annotate"])
        .times(1)
        .returning(|_| Ok(false));
    let mut app = app_with_reviewed_reply_details(shell, lore_api);
    let message_id = selected_message_id(&app);

    app.consolidate_patchset_actions()
        .await
        .expect("patchset actions consolidate");

    assert_reply_action_reset(&app);
    assert!(app.state.user_state.reviewed_patchsets[&message_id].is_empty());
    let saved = saved_reviewed
        .lock()
        .expect("saved reviewed locks")
        .clone()
        .expect("reviewed state should be persisted");
    assert!(saved[&message_id].is_empty());
    shutdown_kw(&app).await;
}
