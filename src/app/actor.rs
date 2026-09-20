//! Central orchestration actor: owns [`App`] and drives the main render/input loop.
//!
//! Each frame: process system updates → project state to [`AppViewModel`] via
//! [`UiHandle`](crate::ui::handle::UiHandle) → draw through
//! [`TerminalHandle`](crate::terminal::handle::TerminalHandle) → await the next
//! [`InputEvent`](crate::input::event::InputEvent) from the channel registered
//! with [`InputHandle`](crate::input::handle::InputHandle).
//!
//! The actor stops when the input event channel closes (user quit) or when I/O
//! returns an unrecoverable error. Startup dependency checks run before this
//! actor is spawned.
use std::ops::ControlFlow;

use color_eyre::{eyre::eyre, Result};
use tokio::{spawn, sync::mpsc};

use crate::{
    app::{
        flows::{
            bookmarked::handle_bookmarked_patchsets, details_actions::handle_patchset_details,
            edit_config::handle_edit_config, latest::handle_latest_patchsets,
            mail_list::handle_mailing_list_selection,
        },
        handle::AppHandle,
        loading::{terminal_error, TerminalLoadingIndicator},
        screens::CurrentScreen,
        App,
    },
    input::{event::InputEvent, handle::InputHandle},
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

        let mut loading = TerminalLoadingIndicator::new(self.terminal_handle.clone());

        loop {
            self.app.process_system_updates(&mut loading).await?;

            let scene = self
                .ui_handle
                .build_scene(self.app.present())
                .await
                .map_err(|e| eyre!("{e}"))?;
            self.terminal_handle
                .draw(TerminalFrame::Main(Box::new(scene)))
                .await
                .map_err(terminal_error)?;

            match self.event_rx.recv().await {
                Some(event) => {
                    match on_input(&mut self.app, event, &self.terminal_handle, &mut loading)
                        .await?
                    {
                        ControlFlow::Continue(()) => {
                            self.input_handle
                                .update_context(self.app.input_context())
                                .await
                                .ok();
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

        tracing::info!("app actor stopped");
        Ok(())
    }
}

async fn on_input(
    app: &mut App,
    input: InputEvent,
    terminal_handle: &TerminalHandle,
    loading: &mut TerminalLoadingIndicator,
) -> Result<ControlFlow<()>> {
    if let Some(popup) = app.state.popup.as_mut() {
        if input == InputEvent::ClosePopup {
            app.state.popup = None;
        } else {
            popup.handle_scroll(input);
        }
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
        }
    }
    Ok(ControlFlow::Continue(()))
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
            },
            services: AppServices {
                lore_api: LoreApiHandle::new(lore_tx),
                render: RenderHandle::new(render_tx),
                shell: Box::new(MockShellTrait::new()),
                fs: Box::new(MockFileSystemTrait::new()),
                config: dummy_config_handle(),
                kw_history: Arc::new(MockKwHistoryStore::new()),
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
            Box::new(MockFileSystemTrait::new()),
            Box::new(MockShellTrait::new()),
            lore_api.clone(),
            render.clone(),
            Arc::new(MockKwHistoryStore::new()),
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
}
