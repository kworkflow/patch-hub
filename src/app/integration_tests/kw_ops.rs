use crate::{
    app::{
        flows::details_actions::handle_patchset_details,
        popup::AppPopup,
        screens::{details_actions::PatchsetDetailsState, CurrentScreen},
    },
    input::event::InputEvent,
};

use super::helpers::{
    app_harness::{dummy_terminal_handle, AppHarness},
    lore::{sample_patch, sample_patchset_details},
    render::sample_rendered_preview,
};

#[tokio::test]
async fn open_kw_ops_without_actor_stays_on_details() {
    let mut harness = AppHarness::new();
    harness.app.state.navigation.current_screen = CurrentScreen::PatchsetDetails;
    harness.app.state.lore.details = Some(details_state());

    handle_patchset_details(
        &mut harness.app,
        InputEvent::OpenKwOps,
        &dummy_terminal_handle(),
    )
    .await
    .unwrap();

    assert_eq!(
        CurrentScreen::PatchsetDetails,
        harness.app.state.navigation.current_screen
    );
    assert_info_popup(
        harness.app.state.popup.as_ref(),
        "Kw operations unavailable",
        "not attached",
    );
}

fn details_state() -> PatchsetDetailsState {
    PatchsetDetailsState::from_rendered_preview(
        sample_patch(),
        sample_patchset_details(),
        sample_rendered_preview(),
        false,
        CurrentScreen::LatestPatchsets,
    )
}

fn assert_info_popup(popup: Option<&AppPopup>, expected_title: &str, fragment: &str) {
    let Some(AppPopup::Info { title, body, .. }) = popup else {
        panic!("expected info popup");
    };
    assert_eq!(expected_title, title);
    assert!(
        body.contains(fragment),
        "expected popup body to contain {fragment:?}, got {body:?}"
    );
}

#[cfg(unix)]
mod unix {
    use std::{
        path::PathBuf,
        sync::{
            atomic::{AtomicU64, Ordering},
            Arc,
        },
    };

    use crate::{
        app::{
            flows::{details_actions::handle_patchset_details, kw_ops::handle_kw_ops},
            screens::CurrentScreen,
            App,
        },
        config::{ConfigSnapshot, ConfigState},
        infrastructure::{
            env::MockEnvTrait,
            file_system::{FileSystemError, MockFileSystemTrait},
            process::FakeProcess,
            shell::{MockShellTrait, ShellOutput},
        },
        input::event::InputEvent,
        kw::{actor::KwActor, history::MockKwHistoryStore, status::KwJobStatus},
        lore::application::cache::BootstrapLoreData,
    };

    use super::{assert_info_popup, details_state};
    use crate::app::integration_tests::helpers::{
        app_harness::{dummy_config_handle, dummy_render_handle, dummy_terminal_handle},
        lore::{lore_handle_with_persistence, sample_mailing_list},
    };

    static LOG_DIR_SEQ: AtomicU64 = AtomicU64::new(0);

    #[tokio::test]
    async fn open_kw_ops_prefills_branch_from_head_not_tree_config() {
        let log_dir = kw_log_dir("open-head");
        let mut app = app_with_details_and_kw(&log_dir, head_branch_shell("feature"));

        handle_patchset_details(&mut app, InputEvent::OpenKwOps, &dummy_terminal_handle())
            .await
            .unwrap();

        assert_eq!(CurrentScreen::KwOps, app.state.navigation.current_screen);
        let ops = app.state.kw.ops.as_ref().expect("KwOps state");
        assert_eq!("feature", ops.branch);
        assert_eq!("main", ops.tree.branch());
        assert!(!ops.head_unreadable);

        shutdown_kw(&app).await;
        std::fs::remove_dir_all(&log_dir).unwrap();
    }

    #[tokio::test]
    async fn start_rejects_empty_branch() {
        let log_dir = kw_log_dir("empty-branch");
        let mut app = app_with_details_and_kw(&log_dir, head_branch_shell(""));
        handle_patchset_details(&mut app, InputEvent::OpenKwOps, &dummy_terminal_handle())
            .await
            .unwrap();
        assert!(app
            .state
            .kw
            .ops
            .as_ref()
            .is_some_and(|ops| ops.branch.is_empty()));

        handle_kw_ops(&mut app, InputEvent::StartKwBuild)
            .await
            .unwrap();
        assert_info_popup(
            app.state.popup.as_ref(),
            "Cannot start build",
            "detached or unverifiable",
        );

        shutdown_kw(&app).await;
        std::fs::remove_dir_all(&log_dir).unwrap();
    }

    #[tokio::test]
    async fn start_build_accepts_and_cancel_sets_requested() {
        let log_dir = kw_log_dir("start-cancel");
        let process = Arc::new(FakeProcess::new());
        let mut app = app_with_details_and_kw_process(
            &log_dir,
            head_branch_shell("feature"),
            process.clone(),
        );
        handle_patchset_details(&mut app, InputEvent::OpenKwOps, &dummy_terminal_handle())
            .await
            .unwrap();

        handle_kw_ops(&mut app, InputEvent::StartKwBuild)
            .await
            .unwrap();
        assert!(app.state.popup.is_none());
        let spawned = process.spawned();
        assert_eq!(1, spawned.len());
        assert_eq!(vec!["build"], spawned[0].args);

        let kw = app.services.kw.as_ref().unwrap().clone();
        let status = kw.get_status().await.unwrap();
        app.state.kw.status = Some(status);
        assert!(matches!(
            app.state.kw.status.as_ref().map(|s| &s.job),
            Some(KwJobStatus::Running { .. })
        ));

        handle_kw_ops(&mut app, InputEvent::CancelKwJob)
            .await
            .unwrap();
        assert!(app
            .state
            .kw
            .ops
            .as_ref()
            .is_some_and(|ops| ops.cancel_requested));

        process.last_child().finish(0);
        shutdown_kw(&app).await;
        std::fs::remove_dir_all(&log_dir).unwrap();
    }

    #[tokio::test]
    async fn back_returns_to_details() {
        let log_dir = kw_log_dir("back");
        let mut app = app_with_details_and_kw(&log_dir, head_branch_shell("feature"));
        handle_patchset_details(&mut app, InputEvent::OpenKwOps, &dummy_terminal_handle())
            .await
            .unwrap();
        handle_kw_ops(&mut app, InputEvent::Back).await.unwrap();
        assert_eq!(
            CurrentScreen::PatchsetDetails,
            app.state.navigation.current_screen
        );
        assert!(app.state.lore.details.is_some());

        shutdown_kw(&app).await;
        std::fs::remove_dir_all(&log_dir).unwrap();
    }

    fn app_with_details_and_kw(log_dir: &std::path::Path, kw_shell: MockShellTrait) -> App {
        app_with_details_and_kw_process(log_dir, kw_shell, Arc::new(FakeProcess::new()))
    }

    fn app_with_details_and_kw_process(
        log_dir: &std::path::Path,
        kw_shell: MockShellTrait,
        process: Arc<FakeProcess>,
    ) -> App {
        let mut history = MockKwHistoryStore::new();
        history
            .expect_apply_record_for_branch()
            .returning(|_, _| Ok(None));
        history.expect_record_build().returning(|_| Ok(()));
        history
            .expect_build_records()
            .returning(|_, _| Ok((None, None)));
        let kw = KwActor::spawn(
            Arc::new(history),
            process,
            Arc::new(kw_shell),
            Arc::new(kw_actor_fs()),
            Arc::new(kw_actor_env()),
            log_dir.to_path_buf(),
        );
        let mut app = App::new(
            apply_config(),
            dummy_config_handle(),
            BootstrapLoreData {
                mailing_lists: vec![sample_mailing_list()],
                bookmarks: vec![],
                reviewed: Default::default(),
            },
            Box::new(MockFileSystemTrait::new()),
            Box::new(MockShellTrait::new()),
            lore_handle_with_persistence(),
            dummy_render_handle(),
            Arc::new(MockKwHistoryStore::new()),
            Some(kw),
        )
        .expect("app should build");
        app.state.navigation.current_screen = CurrentScreen::PatchsetDetails;
        app.state.lore.details = Some(details_state());
        app
    }

    fn head_branch_shell(branch: &str) -> MockShellTrait {
        let branch = branch.to_string();
        let mut shell = MockShellTrait::new();
        shell.expect_execute().returning(move |cmd| {
            let stdout = match cmd.program.as_str() {
                "kw" => b"kw, version 0.10.0\n".to_vec(),
                _ if cmd.args.iter().any(|arg| arg == "status") => Vec::new(),
                _ => format!("{branch}\n").into_bytes(),
            };
            Ok(ShellOutput {
                stdout,
                stderr: Vec::new(),
                success: true,
            })
        });
        shell
    }

    fn kw_actor_fs() -> MockFileSystemTrait {
        let mut fs = MockFileSystemTrait::new();
        fs.expect_is_dir().returning(|_| true);
        fs.expect_is_file()
            .returning(|path| !path.ends_with(".kw/env.current"));
        fs.expect_exists().returning(|_| true);
        fs.expect_read_to_string().returning(|_| {
            Err(FileSystemError::IoError(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "missing",
            )))
        });
        fs.expect_read_dir().returning(|_| {
            Err(FileSystemError::IoError(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "missing",
            )))
        });
        fs.expect_create_dir_all().returning(|_| Ok(()));
        fs
    }

    fn kw_actor_env() -> MockEnvTrait {
        let mut env = MockEnvTrait::new();
        env.expect_which().returning(|_| true);
        env
    }

    fn apply_config() -> ConfigSnapshot {
        serde_json::from_value::<ConfigState>(serde_json::json!({
            "kernel_trees": {
                "linux": {
                    "path": "/kernel",
                    "branch": "main"
                }
            },
            "target_kernel_tree": "linux"
        }))
        .expect("test config should deserialize")
        .to_snapshot()
    }

    async fn shutdown_kw(app: &App) {
        if let Some(kw) = &app.services.kw {
            kw.shutdown().await;
        }
    }

    fn kw_log_dir(test_name: &str) -> PathBuf {
        let n = LOG_DIR_SEQ.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "patch-hub-app-kw-ops-{}-{}-{n}",
            test_name,
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
