//! Central orchestration actor: owns [`App`] and drives the main render/input loop.
//!
//! Each frame: process system updates → project state to [`AppViewModel`] via
//! [`UiHandle`](crate::ui::handle::UiHandle) → draw through
//! [`TerminalHandle`](crate::terminal::handle::TerminalHandle) → await the next
//! [`InputEvent`](crate::input::event::InputEvent), a kw-status change, or a
//! KwOps log-tail tick while a job is running on that screen.
//!
//! The actor stops when the input event channel closes (user quit) or when I/O
//! returns an unrecoverable error. Startup dependency checks run before this
//! actor is spawned.
use std::{ops::ControlFlow, time::Duration};

use color_eyre::{eyre::eyre, Result};
use tokio::{spawn, sync::mpsc, sync::watch, time::MissedTickBehavior};

use crate::{
    app::{
        flows::{
            bookmarked::handle_bookmarked_patchsets,
            details_actions::handle_patchset_details,
            edit_config::handle_edit_config,
            kw_ops::{
                apply_kw_snapshot, apply_kw_snapshot_refreshing_readiness, clear_pending_deploy,
                fallback_kw_status, handle_kw_ops, poll_kw_status, refresh_kw_ops_log_tail,
                resume_pending_deploy,
            },
            latest::handle_latest_patchsets,
            mail_list::handle_mailing_list_selection,
        },
        handle::AppHandle,
        loading::{terminal_error, TerminalLoadingIndicator},
        popup::{AppPopup, ConfirmAction},
        screens::CurrentScreen,
        App,
    },
    input::{event::InputEvent, handle::InputHandle},
    kw::{
        errors::KwError,
        status::{KwJobStatus, KwStatusSnapshot},
    },
    terminal::{handle::TerminalHandle, messages::TerminalFrame},
    ui::handle::UiHandle,
};

/// Owns `App` state and drives the main application loop on a dedicated task.
///
/// Constructed via [`AppActor::spawn`], which moves all owned resources into
/// the actor and returns an [`AppHandle`] to the caller.
///
/// The actor runs until the input event channel closes (the user requested
/// exit via the normal key binding) or an unrecoverable error occurs.
pub struct AppActor {
    app: App,
    terminal_handle: TerminalHandle,
    ui_handle: UiHandle,
    input_handle: InputHandle,
    event_rx: mpsc::Receiver<InputEvent>,
}

enum KwWatchEvent {
    Updated(KwStatusSnapshot),
    Closed,
}

const KW_OPS_LOG_TICK: Duration = Duration::from_millis(350);

impl AppActor {
    /// Moves all resources into a new `AppActor`, spawns it on the Tokio
    /// runtime, and returns an [`AppHandle`] to wait on it.
    pub fn spawn(
        app: App,
        terminal_handle: TerminalHandle,
        ui_handle: UiHandle,
        input_handle: InputHandle,
        event_rx: mpsc::Receiver<InputEvent>,
    ) -> AppHandle {
        tracing::debug!("spawning app actor");
        let actor = Self {
            app,
            terminal_handle,
            ui_handle,
            input_handle,
            event_rx,
        };
        AppHandle::new(spawn(actor.run()))
    }

    async fn run(mut self) -> Result<()> {
        tracing::info!("app actor started");
        tracing::info!("app actor initialized");

        let mut kw_status_rx = self.subscribe_kw_status().await;
        let mut loading = TerminalLoadingIndicator::new(self.terminal_handle.clone());
        let mut log_interval = tokio::time::interval_at(
            tokio::time::Instant::now() + KW_OPS_LOG_TICK,
            KW_OPS_LOG_TICK,
        );
        log_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        let mut redraw = true;
        loop {
            let system_changed = self.app.process_system_updates(&mut loading).await?;
            if system_changed {
                redraw = true;
            }

            if redraw {
                let scene = self
                    .ui_handle
                    .build_scene(self.app.present())
                    .await
                    .map_err(|e| eyre!("{e}"))?;
                self.terminal_handle
                    .draw(TerminalFrame::Main(Box::new(scene)))
                    .await
                    .map_err(terminal_error)?;
            }

            let tail_while_running = kw_ops_should_tail(&self.app);
            let poll_status = kw_status_rx.is_none() && should_poll_kw_status(&self.app);
            tokio::select! {
                event = self.event_rx.recv() => {
                    match event {
                        Some(event) => {
                            match on_input(&mut self.app, event, &self.terminal_handle, &mut loading)
                                .await?
                            {
                                ControlFlow::Continue(()) => {
                                    self.input_handle
                                        .update_context(self.app.input_context())
                                        .await
                                        .ok();
                                    redraw = true;
                                }
                                ControlFlow::Break(()) => break,
                            }
                        }
                        None => {
                            tracing::info!("input channel closed; app actor stopping");
                            break;
                        }
                    }
                }
                watch_event = kw_status_changed(&mut kw_status_rx) => {
                    match watch_event {
                        KwWatchEvent::Updated(snapshot) => {
                            apply_kw_snapshot_refreshing_readiness(&mut self.app, snapshot).await;
                            if self.app.state.navigation.current_screen == CurrentScreen::KwOps
                            {
                                refresh_kw_ops_log_tail(&mut self.app).await;
                            }
                        }
                        KwWatchEvent::Closed => {
                            tracing::warn!(
                                "kw status watch closed; falling back to keyboard-only redraws"
                            );
                            kw_status_rx = None;
                            fallback_kw_status(&mut self.app).await;
                        }
                    }
                    redraw = true;
                }
                _ = log_interval.tick(), if tail_while_running || poll_status => {
                    let mut changed = false;
                    if poll_status {
                        changed |= poll_kw_status(&mut self.app).await;
                    }
                    if tail_while_running {
                        changed |= refresh_kw_ops_log_tail(&mut self.app).await;
                    }
                    redraw = changed;
                }
            }
        }

        tracing::info!("app actor stopped");
        Ok(())
    }

    /// Subscribes once at startup. A missing handle or a failed
    /// subscription leaves the app on keyboard-only redraws rather than
    /// failing to start. A failed subscribe still seeds the projection
    /// from GetStatus so a job already running is visible and the poll
    /// arm can engage.
    async fn subscribe_kw_status(&mut self) -> Option<watch::Receiver<KwStatusSnapshot>> {
        let kw = self.app.services.kw.clone()?;
        match kw.watch_status().await {
            Ok(mut rx) => {
                apply_kw_snapshot(&mut self.app, rx.borrow_and_update().clone());
                Some(rx)
            }
            Err(error) => {
                tracing::warn!(
                    %error,
                    "failed to subscribe to kw status; keyboard-only redraws"
                );
                fallback_kw_status(&mut self.app).await;
                None
            }
        }
    }
}

/// Waits for the next kw-status change. With no receiver this future
/// never completes, so `select!` stays on the input arm instead of
/// spinning.
async fn kw_status_changed(rx: &mut Option<watch::Receiver<KwStatusSnapshot>>) -> KwWatchEvent {
    match rx.as_mut() {
        Some(rx) => match rx.changed().await {
            Ok(()) => KwWatchEvent::Updated(rx.borrow_and_update().clone()),
            Err(_) => KwWatchEvent::Closed,
        },
        None => std::future::pending().await,
    }
}

fn kw_ops_should_tail(app: &App) -> bool {
    app.state.navigation.current_screen == CurrentScreen::KwOps
        && matches!(
            app.state.kw.status.as_ref().map(|status| &status.job),
            Some(KwJobStatus::Running { .. })
        )
}

/// When the status watch is missing, poll GetStatus while a start is
/// in flight or a job is running so `start_requested` cannot latch and
/// the nav indicator can leave "building".
fn should_poll_kw_status(app: &App) -> bool {
    app.services.kw.is_some()
        && (kw_job_is_running(app)
            || app
                .state
                .kw
                .ops
                .as_ref()
                .is_some_and(|ops| ops.start_requested))
}

async fn on_input(
    app: &mut App,
    input: InputEvent,
    terminal_handle: &TerminalHandle,
    loading: &mut TerminalLoadingIndicator,
) -> Result<ControlFlow<()>> {
    if app.state.popup.is_some() {
        match input {
            InputEvent::ClosePopup => {
                dismiss_open_popup(app);
            }
            InputEvent::ConfirmPopup => {
                if let Some(action) = app
                    .state
                    .popup
                    .as_ref()
                    .and_then(AppPopup::selected_confirm_action)
                {
                    return apply_confirm_action(app, action).await;
                }
            }
            _ => {
                if let Some(popup) = app.state.popup.as_mut() {
                    popup.handle_input(input);
                }
            }
        }
    } else if input == InputEvent::Quit && kw_job_is_running(app) {
        app.state.popup = Some(AppPopup::quit_while_job_running());
    } else {
        tracing::debug!(screen = ?app.state.navigation.current_screen, "dispatching input to screen handler");
        match app.state.navigation.current_screen {
            CurrentScreen::MailingListSelection => {
                match handle_mailing_list_selection(app, input, loading).await? {
                    ControlFlow::Continue(()) => {}
                    ControlFlow::Break(()) => return Ok(ControlFlow::Break(())),
                }
            }
            CurrentScreen::BookmarkedPatchsets => {
                handle_bookmarked_patchsets(app, input, loading).await?;
            }
            CurrentScreen::PatchsetDetails => {
                handle_patchset_details(app, input, terminal_handle).await?;
            }
            CurrentScreen::EditConfig => {
                handle_edit_config(app, input).await?;
            }
            CurrentScreen::LatestPatchsets => {
                handle_latest_patchsets(app, input, loading).await?;
            }
            CurrentScreen::KwOps => {
                handle_kw_ops(app, input).await?;
            }
        }
    }
    Ok(ControlFlow::Continue(()))
}

fn is_boot_once_confirm(popup: &AppPopup) -> bool {
    matches!(
        popup.selected_confirm_action(),
        Some(ConfirmAction::ProceedWithBootOnce | ConfirmAction::BackOut)
    )
}

fn dismiss_open_popup(app: &mut App) {
    if app.state.popup.as_ref().is_some_and(is_boot_once_confirm) {
        clear_pending_deploy(app);
    }
    app.state.popup = None;
}

async fn apply_confirm_action(app: &mut App, action: ConfirmAction) -> Result<ControlFlow<()>> {
    app.state.popup = None;
    match action {
        ConfirmAction::CancelKwAndQuit => Ok(cancel_kw_and_quit(app).await),
        ConfirmAction::Wait => Ok(ControlFlow::Continue(())),
        ConfirmAction::ProceedWithBootOnce => {
            resume_pending_deploy(app).await?;
            Ok(ControlFlow::Continue(()))
        }
        ConfirmAction::BackOut => {
            clear_pending_deploy(app);
            Ok(ControlFlow::Continue(()))
        }
    }
}

fn kw_job_is_running(app: &App) -> bool {
    matches!(
        app.state.kw.status.as_ref().map(|status| &status.job),
        Some(KwJobStatus::Running { .. })
    )
}

/// Cancel then leave. A job that finished while the confirm popup was
/// open is `NoJobRunning`; that race is harmless and still quits.
async fn cancel_kw_and_quit(app: &App) -> ControlFlow<()> {
    if let Some(kw) = app.services.kw.as_ref() {
        match kw.cancel().await {
            Ok(()) => {}
            Err(KwError::NoJobRunning) => {
                tracing::debug!("kw job already finished before cancel-and-quit");
            }
            Err(error) => {
                tracing::warn!(%error, "kw cancel failed while quitting");
            }
        }
    }
    ControlFlow::Break(())
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, sync::Arc};

    use tokio::sync::mpsc;

    use crate::{
        app::{
            screens::{
                bookmarked::BookmarkedPatchsetsState, mail_list::MailingListSelectionState,
                CurrentScreen,
            },
            state::{AppState, ConfigUiState, LoreUiState, NavigationState, UserLoreState},
            AppServices,
        },
        config::{ConfigHandle, ConfigState},
        infrastructure::{file_system::MockFileSystemTrait, shell::MockShellTrait},
        input::{event::InputEvent, handle::InputHandle, messages::InputMessage},
        kw::history::MockKwHistoryStore,
        lore::{
            application::{
                actor::LoreApiActor, cache::CacheTtl, handle::LoreApiHandle, service::LoreService,
            },
            domain::mailing_list::MailingList,
            infrastructure::{
                http_lore_client::{MockFeedGateway, MockListsGateway, MockPatchHtmlGateway},
                patchset_fetcher::MockPatchsetFetcher,
                patchset_parser::MockPatchsetParser,
                persistence::{MockMailingListsCacheStore, MockUserLoreStateStore},
            },
        },
        render::{actor::RenderActor, handle::RenderHandle, ShellRenderService},
        terminal::{
            actor::TerminalActor, messages::TerminalFrame, session::MockTerminalSessionApi,
        },
        ui::actor::UiActor,
    };

    use super::*;

    fn dummy_config_handle() -> ConfigHandle {
        let (config_tx, _config_rx) = mpsc::channel(1);
        ConfigHandle::new(config_tx)
    }

    fn minimal_app() -> App {
        let (lore_tx, _lore_rx) = mpsc::channel(1);
        let (render_tx, _render_rx) = mpsc::channel(1);

        let dummy_list = MailingList::new("test-list", "Test list");

        App {
            state: AppState {
                navigation: NavigationState {
                    current_screen: CurrentScreen::MailingListSelection,
                },
                lore: LoreUiState {
                    mailing_list_selection: MailingListSelectionState {
                        mailing_lists: vec![dummy_list.clone()],
                        target_list: String::new(),
                        possible_mailing_lists: vec![dummy_list],
                        highlighted_list_index: 0,
                    },
                    latest_patchsets: None,
                    details: None,
                },
                user_state: UserLoreState {
                    bookmarked_patchsets: BookmarkedPatchsetsState {
                        bookmarked_patchsets: vec![],
                        patchset_index: 0,
                    },
                    reviewed_patchsets: HashMap::new(),
                },
                config_state: ConfigUiState { edit_config: None },
                config: ConfigState::default().to_snapshot(),
                popup: None,
                kw: Default::default(),
            },
            services: AppServices {
                lore_api: LoreApiHandle::new(lore_tx),
                render: RenderHandle::new(render_tx),
                shell: Box::new(MockShellTrait::new()),
                fs: Arc::new(MockFileSystemTrait::new()),
                config: dummy_config_handle(),
                kw_history: Arc::new(MockKwHistoryStore::new()),
                kw: None,
            },
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn input_channel_close_stops_actor_and_returns_ok() {
        let mut session = MockTerminalSessionApi::new();
        session
            .expect_draw()
            .withf(|frame| matches!(frame, TerminalFrame::Main(_)))
            .times(1..)
            .returning(|_| Ok(()));

        let terminal_handle = TerminalActor::spawn(Box::new(session));
        let ui_handle = UiActor::spawn();

        let (event_tx, event_rx) = mpsc::channel::<InputEvent>(1);
        let (input_tx, _input_rx) = mpsc::channel::<InputMessage>(1);
        let input_handle = InputHandle::new(input_tx);

        let handle = AppActor::spawn(
            minimal_app(),
            terminal_handle,
            ui_handle,
            input_handle,
            event_rx,
        );

        // Dropping the sender closes the channel; the actor stops after one
        // render frame when event_rx.recv() returns None.
        drop(event_tx);
        let result = handle.run_until_done().await;
        assert!(result.is_ok());
    }

    fn spawn_real_lore_api(list: MailingList) -> LoreApiHandle {
        let mut lists_store = MockMailingListsCacheStore::new();
        lists_store
            .expect_load_available_lists()
            .returning(move || Ok(vec![list.clone()]));
        let mut user_state = MockUserLoreStateStore::new();
        user_state
            .expect_load_bookmarked_patchsets()
            .returning(|| Ok(vec![]));
        user_state
            .expect_load_reviewed_patchsets()
            .returning(|| Ok(HashMap::new()));

        let service = LoreService::new(
            Arc::new(MockListsGateway::new()),
            Arc::new(MockFeedGateway::new()),
            Arc::new(MockPatchHtmlGateway::new()),
            Arc::new(lists_store),
            Arc::new(user_state),
            Arc::new(MockPatchsetFetcher::new()),
            Arc::new(MockPatchsetParser::new()),
            Arc::new(MockFileSystemTrait::new()),
            Arc::new(MockShellTrait::new()),
            CacheTtl::default(),
        );
        LoreApiActor::spawn(service)
    }

    fn spawn_real_render() -> RenderHandle {
        RenderActor::spawn(Box::new(ShellRenderService::new(Arc::new(
            MockShellTrait::new(),
        ))))
    }

    /// Verifies that AppActor, LoreApiActor, and RenderActor can be wired
    /// together, go through a full bootstrap cycle, and all shut down cleanly
    /// in the documented order when the input channel closes.
    #[tokio::test(flavor = "multi_thread")]
    async fn three_actor_lifecycle_stops_cleanly() {
        let dummy_list = MailingList::new("test-list", "Test list");
        let lore_api = spawn_real_lore_api(dummy_list.clone());
        let render = spawn_real_render();

        let bootstrap = lore_api
            .get_bootstrap_data()
            .await
            .expect("bootstrap must succeed with mock infrastructure");
        assert_eq!(1, bootstrap.mailing_lists.len());

        let mut session = MockTerminalSessionApi::new();
        session
            .expect_draw()
            .withf(|frame| matches!(frame, TerminalFrame::Main(_)))
            .times(1..)
            .returning(|_| Ok(()));
        let terminal_handle = TerminalActor::spawn(Box::new(session));
        let ui_handle = UiActor::spawn();

        let app = App::new(
            ConfigState::default().to_snapshot(),
            dummy_config_handle(),
            bootstrap,
            Arc::new(MockFileSystemTrait::new()),
            Box::new(MockShellTrait::new()),
            lore_api.clone(),
            render.clone(),
            Arc::new(MockKwHistoryStore::new()),
            None,
        )
        .expect("App::new must succeed");

        let (event_tx, event_rx) = mpsc::channel::<InputEvent>(1);
        let (input_tx, _input_rx) = mpsc::channel::<InputMessage>(1);
        let input_handle = InputHandle::new(input_tx);

        let handle = AppActor::spawn(app, terminal_handle, ui_handle, input_handle, event_rx);

        // Closing the input channel stops AppActor.
        drop(event_tx);
        assert!(handle.run_until_done().await.is_ok());

        // Explicit shutdown in documented order: LoreAPI then Render.
        lore_api.shutdown().await;
        render.shutdown().await;
    }

    fn sample_kw_ops() -> crate::app::screens::kw_ops::KwOpsState {
        crate::app::screens::kw_ops::KwOpsState::new(
            "title".to_string(),
            "mid".to_string(),
            "linux".to_string(),
            serde_json::from_value(serde_json::json!({
                "path": "/kernel",
                "branch": "main"
            }))
            .unwrap(),
            crate::kw::readiness::KwReadiness {
                kw_binary: crate::kw::readiness::KwBinaryProbe {
                    available: true,
                    version_line: Some("kw, version 0.10.0".to_string()),
                    check: crate::kw::readiness::KwVersionCheck::Meets,
                },
                tree: crate::kw::readiness::TreeReadiness::Ready {
                    arch: Some("x86_64".to_string()),
                },
                output_dir: None,
                kernel_image: None,
                build_record: None,
                latest_build: None,
                deploy_alone: Err(crate::kw::readiness::DeployAloneRefusal::NoBuildRecord),
                current_branch: Some("main".to_string()),
                deploy_remote: Err(crate::kw::remote::RemoteRefusal::NoRemotesConfigured),
                boot_once: crate::kw::readiness::BootOnceState::Unknown,
            },
        )
    }

    #[tokio::test]
    async fn proceed_with_boot_once_acks_and_clears_pending() {
        let mut app = minimal_app();
        let mut ops = sample_kw_ops();
        ops.pending_deploy = Some(crate::app::screens::kw_ops::DeployStartKind::Deploy);
        app.state.kw.ops = Some(ops);
        app.state.popup = Some(AppPopup::boot_once_warning());

        let flow = apply_confirm_action(&mut app, ConfirmAction::ProceedWithBootOnce)
            .await
            .unwrap();
        assert_eq!(ControlFlow::Continue(()), flow);
        let ops = app.state.kw.ops.as_ref().unwrap();
        assert!(ops.boot_once_acknowledged);
        assert_eq!(None, ops.pending_deploy);
        let Some(AppPopup::Info { title, body, .. }) = &app.state.popup else {
            panic!("resume without a kw actor should explain that deploy cannot start");
        };
        assert_eq!("Cannot start deploy", title);
        assert!(
            body.contains("no build recorded"),
            "resume with a stale deploy-alone snapshot should refuse before the missing-actor path, got {body:?}"
        );
    }

    #[tokio::test]
    async fn back_out_clears_pending_without_acknowledging() {
        let mut app = minimal_app();
        let mut ops = sample_kw_ops();
        ops.pending_deploy = Some(crate::app::screens::kw_ops::DeployStartKind::BuildThenDeploy);
        app.state.kw.ops = Some(ops);
        app.state.popup = Some(AppPopup::boot_once_warning());

        let flow = apply_confirm_action(&mut app, ConfirmAction::BackOut)
            .await
            .unwrap();
        assert_eq!(ControlFlow::Continue(()), flow);
        assert!(app.state.popup.is_none());
        let ops = app.state.kw.ops.as_ref().unwrap();
        assert!(!ops.boot_once_acknowledged);
        assert_eq!(None, ops.pending_deploy);
    }

    #[test]
    fn closing_the_boot_once_popup_clears_pending() {
        let mut app = minimal_app();
        let mut ops = sample_kw_ops();
        ops.pending_deploy = Some(crate::app::screens::kw_ops::DeployStartKind::Deploy);
        app.state.kw.ops = Some(ops);
        app.state.popup = Some(AppPopup::boot_once_warning());

        dismiss_open_popup(&mut app);
        assert!(app.state.popup.is_none());
        assert_eq!(None, app.state.kw.ops.as_ref().unwrap().pending_deploy);
        assert!(!app.state.kw.ops.as_ref().unwrap().boot_once_acknowledged);
    }

    #[test]
    fn closing_the_quit_popup_does_not_touch_pending_deploy() {
        let mut app = minimal_app();
        let mut ops = sample_kw_ops();
        ops.pending_deploy = Some(crate::app::screens::kw_ops::DeployStartKind::Deploy);
        app.state.kw.ops = Some(ops);
        app.state.popup = Some(AppPopup::quit_while_job_running());

        dismiss_open_popup(&mut app);
        assert!(app.state.popup.is_none());
        assert_eq!(
            Some(crate::app::screens::kw_ops::DeployStartKind::Deploy),
            app.state.kw.ops.as_ref().unwrap().pending_deploy
        );
    }
}
