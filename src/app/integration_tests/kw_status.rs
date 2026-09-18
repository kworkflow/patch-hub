use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use crate::{
    app::actor::AppActor,
    config::{ConfigSnapshot, ConfigState},
    infrastructure::{
        env::MockEnvTrait,
        file_system::{FileSystemError, MockFileSystemTrait},
        process::FakeProcess,
        shell::{MockShellTrait, ShellOutput},
    },
    input::{event::InputEvent, handle::InputHandle, messages::InputMessage},
    kw::{actor::KwActor, history::MockKwHistoryStore, messages::StartRequest},
    lore::application::cache::BootstrapLoreData,
    terminal::{actor::TerminalActor, messages::TerminalFrame, session::MockTerminalSessionApi},
    ui::{actor::UiActor, scene::UiScene},
};

use super::helpers::{
    app_harness::{dummy_config_handle, dummy_render_handle},
    lore::{lore_handle_with_persistence, sample_mailing_list},
};

const KERNEL_TREE_PATH: &str = "/kernel";
const BUILD_BRANCH: &str = "patchset-2026-08-20-15-00-00";

static LOG_DIR_SEQ: AtomicU64 = AtomicU64::new(0);

#[tokio::test(flavor = "multi_thread")]
async fn first_frame_shows_a_job_that_started_before_attach() {
    let log_dir = kw_log_dir("attach-running");
    let (app, kw, process) = app_with_kw(&log_dir);
    kw.start_build(start_request()).await.unwrap();

    let (scenes, event_tx, handle) = spawn_app_actor(app);
    wait_for_nav(&scenes, |text| text.contains("kw: building")).await;

    process.last_child().finish(0);
    drop(event_tx);
    handle.run_until_done().await.unwrap();
    kw.shutdown().await;
    std::fs::remove_dir_all(&log_dir).unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn status_change_redraws_without_input() {
    let log_dir = kw_log_dir("redraw-no-input");
    let (app, kw, process) = app_with_kw(&log_dir);
    let (scenes, event_tx, handle) = spawn_app_actor(app);

    wait_for_nav(&scenes, |text| !text.contains("kw:")).await;
    let draws_before_start = scene_count(&scenes);

    kw.start_build(start_request()).await.unwrap();
    wait_for_nav(&scenes, |text| {
        text.contains(&format!("kw: building {BUILD_BRANCH}"))
    })
    .await;
    assert!(
        scene_count(&scenes) > draws_before_start,
        "a running job must trigger a redraw without an input event"
    );

    process.last_child().finish(0);
    wait_for_nav(&scenes, |text| !text.contains("kw:")).await;

    drop(event_tx);
    handle.run_until_done().await.unwrap();
    kw.shutdown().await;
    std::fs::remove_dir_all(&log_dir).unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn closed_watch_does_not_busy_loop_and_input_still_works() {
    let log_dir = kw_log_dir("watch-closed");
    let (app, kw, _process) = app_with_kw(&log_dir);
    let (scenes, event_tx, handle) = spawn_app_actor(app);

    wait_for_nav(&scenes, |_| true).await;
    kw.shutdown().await;

    // Give the actor a moment to observe the closed watch and redraw once.
    tokio::time::sleep(Duration::from_millis(50)).await;
    let draws_after_close = scene_count(&scenes);
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(
        draws_after_close,
        scene_count(&scenes),
        "a closed kw watch must not spin the render loop"
    );

    event_tx.send(InputEvent::NavigateDown).await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if scene_count(&scenes) > draws_after_close {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("input must still redraw after the kw watch closes");

    drop(event_tx);
    handle.run_until_done().await.unwrap();
    std::fs::remove_dir_all(&log_dir).unwrap();
}

fn spawn_app_actor(
    app: crate::app::App,
) -> (
    Arc<Mutex<Vec<UiScene>>>,
    tokio::sync::mpsc::Sender<InputEvent>,
    crate::app::handle::AppHandle,
) {
    let scenes = Arc::new(Mutex::new(Vec::new()));
    let scenes_for_draw = Arc::clone(&scenes);
    let mut session = MockTerminalSessionApi::new();
    session
        .expect_draw()
        .withf(|frame| matches!(frame, TerminalFrame::Main(_)))
        .times(1..)
        .returning(move |frame| {
            if let TerminalFrame::Main(scene) = frame {
                scenes_for_draw.lock().unwrap().push(*scene);
            }
            Ok(())
        });

    let terminal_handle = TerminalActor::spawn(Box::new(session));
    let ui_handle = UiActor::spawn();
    let (event_tx, event_rx) = tokio::sync::mpsc::channel::<InputEvent>(8);
    let (input_tx, _input_rx) = tokio::sync::mpsc::channel::<InputMessage>(1);
    let input_handle = InputHandle::new(input_tx);
    let handle = AppActor::spawn(app, terminal_handle, ui_handle, input_handle, event_rx);
    (scenes, event_tx, handle)
}

fn app_with_kw(
    log_dir: &std::path::Path,
) -> (
    crate::app::App,
    crate::kw::handle::KwHandle,
    Arc<FakeProcess>,
) {
    let process = Arc::new(FakeProcess::new());
    let mut history = MockKwHistoryStore::new();
    history
        .expect_apply_record_for_branch()
        .returning(|_, _| Ok(None));
    history.expect_record_build().returning(|_| Ok(()));
    let kw = KwActor::spawn(
        Arc::new(history),
        process.clone(),
        Arc::new(kw_actor_shell()),
        Arc::new(kw_actor_fs()),
        Arc::new(kw_actor_env()),
        log_dir.to_path_buf(),
    );
    let app = crate::app::App::new(
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
        Some(kw.clone()),
    )
    .expect("app should build");
    (app, kw, process)
}

async fn wait_for_nav(scenes: &Arc<Mutex<Vec<UiScene>>>, predicate: impl Fn(&str) -> bool) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if scenes
                .lock()
                .unwrap()
                .iter()
                .any(|scene| predicate(&nav_text(scene)))
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("expected navigation text did not appear");
}

fn scene_count(scenes: &Arc<Mutex<Vec<UiScene>>>) -> usize {
    scenes.lock().unwrap().len()
}

fn nav_text(scene: &UiScene) -> String {
    scene
        .navigation
        .mode_spans
        .iter()
        .map(|span| span.content.to_string())
        .collect()
}

fn kw_log_dir(test_name: &str) -> PathBuf {
    let n = LOG_DIR_SEQ.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "patch-hub-app-kw-status-{}-{}-{n}",
        test_name,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn start_request() -> StartRequest {
    StartRequest {
        kernel_tree_id: "linux".to_string(),
        tree: serde_json::from_value(serde_json::json!({
            "path": KERNEL_TREE_PATH,
            "branch": "main"
        }))
        .expect("kernel tree should deserialize"),
        branch: BUILD_BRANCH.to_string(),
        extra_args: Vec::new(),
    }
}

fn apply_config() -> ConfigSnapshot {
    serde_json::from_value::<ConfigState>(serde_json::json!({
        "kernel_trees": {
            "linux": {
                "path": KERNEL_TREE_PATH,
                "branch": "main"
            }
        },
        "target_kernel_tree": "linux"
    }))
    .expect("test config should deserialize")
    .to_snapshot()
}

fn kw_actor_shell() -> MockShellTrait {
    let mut shell = MockShellTrait::new();
    shell.expect_execute().returning(|cmd| {
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
