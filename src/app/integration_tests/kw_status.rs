use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use crate::app::handle::AppHandle;
use crate::app::App;
use crate::kw::handle::KwHandle;
use crate::ui::scene::{PopupBody, PopupScene};
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
    lore::application::models::cache::BootstrapLoreData,
    terminal::{actor::TerminalActor, messages::TerminalFrame, session::MockTerminalSessionApi},
    ui::{actor::UiActor, scene::UiScene},
};

use super::helpers::{
    app_harness::{dummy_config_handle, dummy_render_handle},
    lore::{lore_handle_with_persistence, sample_mailing_list},
};
use std::env;
use std::fs;
use std::io;
use std::path::Path;
use std::process;
use tokio::sync::mpsc;
use tokio::time;

const KERNEL_TREE_PATH: &str = "/kernel";
const BUILD_BRANCH: &str = "patchset-2026-08-20-15-00-00";

static LOG_DIR_SEQ: AtomicU64 = AtomicU64::new(0);

#[tokio::test(flavor = "multi_thread")]
async fn first_frame_shows_a_job_that_started_before_attach() {
    let log_dir = kw_log_dir("attach-running");
    let (app, kw, process) = app_with_kw(&log_dir);
    kw.start_build(start_request()).await.expect("build starts");

    let (scenes, event_tx, handle) = spawn_app_actor(app);
    wait_for_nav(&scenes, |text| text.contains("kw: building")).await;

    process.last_child().finish(0);
    drop(event_tx);
    handle.run_until_done().await.expect("actor finishes");
    kw.shutdown().await;
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
}

#[tokio::test(flavor = "multi_thread")]
async fn status_change_redraws_without_input() {
    let log_dir = kw_log_dir("redraw-no-input");
    let (app, kw, process) = app_with_kw(&log_dir);
    let (scenes, event_tx, handle) = spawn_app_actor(app);

    wait_for_nav(&scenes, |text| !text.contains("kw:")).await;
    let draws_before_start = scene_count(&scenes);

    kw.start_build(start_request()).await.expect("build starts");
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
    handle.run_until_done().await.expect("actor finishes");
    kw.shutdown().await;
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
}

#[tokio::test(flavor = "multi_thread")]
async fn closed_watch_does_not_busy_loop_and_input_still_works() {
    let log_dir = kw_log_dir("watch-closed");
    let (app, kw, _process) = app_with_kw(&log_dir);
    let (scenes, event_tx, handle) = spawn_app_actor(app);

    wait_for_nav(&scenes, |_| true).await;
    kw.shutdown().await;

    // Give the actor a moment to observe the closed watch and redraw once.
    time::sleep(Duration::from_millis(50)).await;
    let draws_after_close = scene_count(&scenes);
    time::sleep(Duration::from_millis(150)).await;
    assert_eq!(
        draws_after_close,
        scene_count(&scenes),
        "a closed kw watch must not spin the render loop"
    );

    event_tx
        .send(InputEvent::NavigateDown)
        .await
        .expect("navigate down sends");
    time::timeout(Duration::from_secs(5), async {
        loop {
            if scene_count(&scenes) > draws_after_close {
                break;
            }
            time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("input must still redraw after the kw watch closes");

    drop(event_tx);
    handle.run_until_done().await.expect("actor finishes");
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
}

#[tokio::test(flavor = "multi_thread")]
async fn quit_with_no_job_exits_without_confirm() {
    let log_dir = kw_log_dir("quit-idle");
    let (app, kw, _process) = app_with_kw(&log_dir);
    let (scenes, event_tx, handle) = spawn_app_actor(app);

    wait_for_nav(&scenes, |_| true).await;
    event_tx.send(InputEvent::Quit).await.expect("quit sends");
    time::timeout(Duration::from_secs(5), handle.run_until_done())
        .await
        .expect("quit with no running job must exit without a confirm popup")
        .expect("actor finishes");

    kw.shutdown().await;
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
}

#[tokio::test(flavor = "multi_thread")]
async fn quit_while_job_running_opens_confirm_and_wait_keeps_app_alive() {
    let log_dir = kw_log_dir("quit-wait");
    let (app, kw, process) = app_with_kw(&log_dir);
    kw.start_build(start_request()).await.expect("build starts");

    let (scenes, event_tx, handle) = spawn_app_actor(app);
    wait_for_nav(&scenes, |text| text.contains("kw: building")).await;

    event_tx.send(InputEvent::Quit).await.expect("quit sends");
    wait_for_latest_popup(&scenes, |popup| {
        popup.is_some_and(|popup| popup.title == "Cancel job and quit?")
    })
    .await;
    let latest = scenes
        .lock()
        .expect("scenes locks")
        .last()
        .cloned()
        .expect("last scene is set");
    let popup = latest.popup.expect("confirm popup");
    match popup.body {
        PopupBody::Confirm {
            options, selected, ..
        } => {
            assert_eq!(
                vec!["Cancel and quit".to_string(), "Wait".to_string()],
                options
            );
            assert_eq!(1, selected);
        }
        other => panic!("expected Confirm scene, got {other:?}"),
    }

    event_tx
        .send(InputEvent::ConfirmPopup)
        .await
        .expect("confirm popup sends");
    wait_for_latest_popup(&scenes, |popup| popup.is_none()).await;
    wait_for_nav(&scenes, |text| text.contains("kw: building")).await;

    event_tx
        .send(InputEvent::NavigateDown)
        .await
        .expect("navigate down sends");
    process.last_child().finish(0);
    drop(event_tx);
    handle.run_until_done().await.expect("actor finishes");
    kw.shutdown().await;
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
}

#[tokio::test(flavor = "multi_thread")]
async fn quit_confirm_esc_is_wait() {
    let log_dir = kw_log_dir("quit-esc-wait");
    let (app, kw, process) = app_with_kw(&log_dir);
    kw.start_build(start_request()).await.expect("build starts");

    let (scenes, event_tx, handle) = spawn_app_actor(app);
    wait_for_nav(&scenes, |text| text.contains("kw: building")).await;

    event_tx.send(InputEvent::Quit).await.expect("quit sends");
    wait_for_latest_popup(&scenes, |popup| popup.is_some()).await;
    event_tx
        .send(InputEvent::ClosePopup)
        .await
        .expect("close popup sends");
    wait_for_latest_popup(&scenes, |popup| popup.is_none()).await;
    wait_for_nav(&scenes, |text| text.contains("kw: building")).await;

    process.last_child().finish(0);
    drop(event_tx);
    handle.run_until_done().await.expect("actor finishes");
    kw.shutdown().await;
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
}

#[tokio::test(flavor = "multi_thread")]
async fn cancel_and_quit_requests_cancellation() {
    let log_dir = kw_log_dir("quit-cancel");
    let (app, kw, process) = app_with_kw(&log_dir);
    kw.start_build(start_request()).await.expect("build starts");

    let (scenes, event_tx, handle) = spawn_app_actor(app);
    wait_for_nav(&scenes, |text| text.contains("kw: building")).await;

    event_tx.send(InputEvent::Quit).await.expect("quit sends");
    wait_for_latest_popup(&scenes, |popup| popup.is_some()).await;
    event_tx
        .send(InputEvent::NavigateLeft)
        .await
        .expect("navigate left sends");
    event_tx
        .send(InputEvent::ConfirmPopup)
        .await
        .expect("confirm popup sends");

    handle.run_until_done().await.expect("actor finishes");
    time::timeout(Duration::from_secs(5), async {
        loop {
            if process.last_child().was_killed() {
                break;
            }
            time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("cancel-and-quit must request kw cancellation");
    drop(event_tx);
    kw.shutdown().await;
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
}

fn spawn_app_actor(
    app: App,
) -> (
    Arc<Mutex<Vec<UiScene>>>,
    mpsc::Sender<InputEvent>,
    AppHandle,
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
                scenes_for_draw
                    .lock()
                    .expect("scenes for draw locks")
                    .push(*scene);
            }
            Ok(())
        });

    let terminal_handle = TerminalActor::spawn(Box::new(session));
    let ui_handle = UiActor::spawn();
    let (event_tx, event_rx) = mpsc::channel::<InputEvent>(8);
    let (input_tx, _input_rx) = mpsc::channel::<InputMessage>(1);
    let input_handle = InputHandle::new(input_tx);
    let handle = AppActor::spawn(app, terminal_handle, ui_handle, input_handle, event_rx);
    (scenes, event_tx, handle)
}

fn app_with_kw(log_dir: &Path) -> (App, KwHandle, Arc<FakeProcess>) {
    let process = Arc::new(FakeProcess::new());
    let mut history = MockKwHistoryStore::new();
    history
        .expect_apply_record_for_branch()
        .withf(|tree, branch| tree == "linux" && branch == "patchset-2026-08-20-15-00-00")
        .times(1)
        .returning(|_, _| Ok(None));
    history
        .expect_record_build()
        .withf(|record| record.branch == "patchset-2026-08-20-15-00-00")
        .times(1)
        .returning(|_| Ok(()));
    let kw = KwActor::spawn(
        Arc::new(history),
        process.clone(),
        Arc::new(kw_actor_shell()),
        Arc::new(kw_actor_fs()),
        Arc::new(kw_actor_env()),
        log_dir.to_path_buf(),
    );
    let app = App::new(
        apply_config(),
        dummy_config_handle(),
        BootstrapLoreData {
            mailing_lists: vec![sample_mailing_list()],
            bookmarks: vec![],
            reviewed: Default::default(),
        },
        Arc::new(MockFileSystemTrait::new()),
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
    time::timeout(Duration::from_secs(5), async {
        loop {
            if scenes
                .lock()
                .expect("scenes locks")
                .iter()
                .any(|scene| predicate(&nav_text(scene)))
            {
                return;
            }
            time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("expected navigation text did not appear");
}

async fn wait_for_latest_popup(
    scenes: &Arc<Mutex<Vec<UiScene>>>,
    predicate: impl Fn(Option<&PopupScene>) -> bool,
) {
    time::timeout(Duration::from_secs(5), async {
        loop {
            let matches = scenes
                .lock()
                .expect("scenes locks")
                .last()
                .is_some_and(|scene| predicate(scene.popup.as_ref()));
            if matches {
                return;
            }
            time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("expected popup state did not appear");
}

fn scene_count(scenes: &Arc<Mutex<Vec<UiScene>>>) -> usize {
    scenes.lock().expect("scenes locks").len()
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
    let dir = env::temp_dir().join(format!(
        "patch-hub-app-kw-status-{}-{}-{n}",
        test_name,
        process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("dir creates");
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
        deploy: None,
    }
}

fn apply_config() -> ConfigSnapshot {
    ConfigSnapshot::from(
        &serde_json::from_value::<ConfigState>(serde_json::json!({
            "kernel_trees": {
                "linux": {
                    "path": KERNEL_TREE_PATH,
                    "branch": "main"
                }
            },
            "target_kernel_tree": "linux"
        }))
        .expect("test config should deserialize"),
    )
}

fn kw_actor_shell() -> MockShellTrait {
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
                || cmd.program == "git" && cmd.args == ["-C", "/kernel", "branch", "--show-current"]
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

fn kw_actor_fs() -> MockFileSystemTrait {
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

fn kw_actor_env() -> MockEnvTrait {
    let mut env = MockEnvTrait::new();
    env.expect_which()
        .withf(|name| name == "kw")
        .times(1)
        .returning(|_| true);
    env
}
