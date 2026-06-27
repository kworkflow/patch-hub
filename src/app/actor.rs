use std::ops::ControlFlow;

use tokio::sync::mpsc;
use tracing::{event, Level};

use crate::{
    app::{
        errors::AppError,
        flows::{
            bookmarked::handle_bookmarked_patchsets,
            details_actions::handle_patchset_details,
            edit_config::handle_edit_config,
            latest::handle_latest_patchsets,
            mail_list::handle_mailing_list_selection,
        },
        handle::AppHandle,
        loading::{terminal_error, TerminalLoadingIndicator},
        messages::AppMessage,
        screens::CurrentScreen,
        App,
    },
    input::{event::InputEvent, handle::InputHandle},
    render_prefs::PatchRenderer,
    terminal::{handle::TerminalHandle, messages::TerminalFrame},
    ui::handle::UiHandle,
};

pub const DEFAULT_APP_MSG_CHANNEL_SIZE: usize = 32;

/// Owns `App` state and drives the main application loop on a dedicated task.
///
/// Constructed via [`AppActor::spawn`], which moves all owned resources into
/// the actor and returns an [`AppHandle`] to the caller.
pub struct AppActor {
    app: App,
    terminal_handle: TerminalHandle,
    ui_handle: UiHandle,
    input_handle: InputHandle,
    event_rx: mpsc::Receiver<InputEvent>,
    msg_rx: mpsc::Receiver<AppMessage>,
}

impl AppActor {
    /// Moves all resources into a new `AppActor`, spawns it on the Tokio
    /// runtime, and returns an [`AppHandle`] to control and wait on it.
    pub fn spawn(
        app: App,
        terminal_handle: TerminalHandle,
        ui_handle: UiHandle,
        input_handle: InputHandle,
        event_rx: mpsc::Receiver<InputEvent>,
    ) -> AppHandle {
        let (msg_tx, msg_rx) = mpsc::channel(DEFAULT_APP_MSG_CHANNEL_SIZE);
        let actor = Self {
            app,
            terminal_handle,
            ui_handle,
            input_handle,
            event_rx,
            msg_rx,
        };
        AppHandle::new(tokio::spawn(actor.run()), msg_tx)
    }

    /// Verifies required and optional external binaries.
    ///
    /// A missing `b4` is a hard failure; all other missing binaries only emit
    /// warnings. This replicates the former `check_external_deps` free function
    /// that lived in `main.rs`.
    fn initialize(&self) -> Result<(), AppError> {
        let env = &*self.app.services.env;
        let config = &self.app.state.config;

        if !env.which("b4") {
            event!(Level::ERROR, "b4 is not installed, patchsets cannot be downloaded");
            return Err(AppError::Dependencies(
                "b4 is not installed; patchsets cannot be downloaded".to_string(),
            ));
        }

        if !env.which("git") {
            event!(Level::WARN, "git is not installed, send-email won't work");
        }

        match config.patch_renderer() {
            PatchRenderer::Bat => {
                if !env.which("bat") {
                    event!(
                        Level::WARN,
                        "bat is not installed, patch rendering will fallback to default"
                    );
                }
            }
            PatchRenderer::Delta => {
                if !env.which("delta") {
                    event!(
                        Level::WARN,
                        "delta is not installed, patch rendering will fallback to default",
                    );
                }
            }
            PatchRenderer::DiffSoFancy => {
                if !env.which("diff-so-fancy") {
                    event!(
                        Level::WARN,
                        "diff-so-fancy is not installed, patch rendering will fallback to default",
                    );
                }
            }
            _ => {}
        }

        Ok(())
    }

    async fn run(mut self) -> color_eyre::Result<()> {
        self.initialize()?;

        let mut loading = TerminalLoadingIndicator::new(self.terminal_handle.clone());

        loop {
            self.app.process_system_updates(&mut loading).await?;

            let scene = self
                .ui_handle
                .build_scene(self.app.present())
                .await
                .map_err(|e| color_eyre::eyre::eyre!("{e}"))?;
            self.terminal_handle
                .draw(TerminalFrame::Main(Box::new(scene)))
                .await
                .map_err(terminal_error)?;

            tokio::select! {
                input = self.event_rx.recv() => match input {
                    Some(event) => {
                        match on_input(&mut self.app, event, &self.terminal_handle, &mut loading)
                            .await?
                        {
                            ControlFlow::Continue(()) => {}
                            ControlFlow::Break(()) => return Ok(()),
                        }
                        self.input_handle
                            .update_context(self.app.input_context())
                            .await
                            .ok();
                    }
                    None => return Ok(()),
                },
                msg = self.msg_rx.recv() => match msg {
                    Some(AppMessage::Shutdown { reply_to }) => {
                        reply_to.send(()).ok();
                        return Ok(());
                    }
                    Some(AppMessage::Initialize { reply_to }) => {
                        reply_to.send(self.initialize()).ok();
                    }
                    Some(AppMessage::GetViewModel { reply_to }) => {
                        reply_to.send(self.app.present()).ok();
                    }
                    Some(AppMessage::GetInputContext { reply_to }) => {
                        reply_to.send(self.app.input_context()).ok();
                    }
                    Some(AppMessage::Input { .. }) | None => {}
                },
            }
        }
    }
}

async fn on_input(
    app: &mut App,
    input: InputEvent,
    terminal_handle: &TerminalHandle,
    loading: &mut TerminalLoadingIndicator,
) -> color_eyre::Result<ControlFlow<()>> {
    if let Some(popup) = app.state.popup.as_mut() {
        if input == InputEvent::ClosePopup {
            app.state.popup = None;
        } else {
            popup.handle_scroll(input);
        }
    } else {
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
                handle_edit_config(app, input)?;
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
    use std::collections::HashMap;

    use tokio::sync::mpsc;

    use crate::{
        app::{
            errors::AppError,
            screens::{
                bookmarked::BookmarkedPatchsetsState, mail_list::MailingListSelectionState,
                CurrentScreen,
            },
            state::{AppState, ConfigUiState, LoreUiState, NavigationState, UserLoreState},
            AppServices,
        },
        config::ConfigState,
        infrastructure::{
            env::MockEnvTrait, file_system::MockFileSystemTrait, shell::MockShellTrait,
        },
        input::{handle::InputHandle, messages::InputMessage},
        lore::{application::handle::LoreApiHandle, domain::mailing_list::MailingList},
        render::handle::RenderHandle,
        terminal::{actor::TerminalActor, messages::TerminalFrame, session::MockTerminalSessionApi},
        ui::actor::UiActor,
    };

    use super::*;

    struct NullConfigService;

    impl crate::config::ConfigServiceApi for NullConfigService {
        fn snapshot(&self) -> crate::config::ConfigSnapshot {
            ConfigState::default().to_snapshot()
        }

        fn validate_update(
            &self,
            _: crate::config::ConfigUpdateDraft,
        ) -> Result<crate::config::ValidatedConfigUpdate, crate::config::ConfigError> {
            unimplemented!()
        }

        fn apply_update(
            &mut self,
            _: crate::config::ValidatedConfigUpdate,
        ) -> Result<crate::config::ConfigSnapshot, crate::config::ConfigError> {
            unimplemented!()
        }
    }

    fn minimal_app_with_env(env: MockEnvTrait) -> App {
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
                env: Box::new(env),
                config: Box::new(NullConfigService),
            },
        }
    }

    fn minimal_app() -> App {
        let mut env = MockEnvTrait::new();
        env.expect_which().returning(|_| true);
        minimal_app_with_env(env)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn shutdown_message_stops_actor_and_returns_ok() {
        let mut session = MockTerminalSessionApi::new();
        session
            .expect_draw()
            .withf(|frame| matches!(frame, TerminalFrame::Main(_)))
            .times(1..)
            .returning(|_| Ok(()));

        let terminal_handle = TerminalActor::spawn(Box::new(session));
        let ui_handle = UiActor::spawn();

        let (_event_tx, event_rx) = mpsc::channel::<InputEvent>(1);
        let (input_tx, _input_rx) = mpsc::channel::<InputMessage>(1);
        let input_handle = InputHandle::new(input_tx);

        let handle = AppActor::spawn(
            minimal_app(),
            terminal_handle,
            ui_handle,
            input_handle,
            event_rx,
        );

        handle.shutdown().await;
        let result = handle.run_until_done().await;
        assert!(result.is_ok());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn missing_b4_causes_actor_to_return_dependencies_error() {
        let mut env = MockEnvTrait::new();
        env.expect_which()
            .withf(|name| name == "b4")
            .returning(|_| false);

        let mut session = MockTerminalSessionApi::new();
        session.expect_draw().returning(|_| Ok(()));

        let terminal_handle = TerminalActor::spawn(Box::new(session));
        let ui_handle = UiActor::spawn();

        let (_event_tx, event_rx) = mpsc::channel::<InputEvent>(1);
        let (input_tx, _input_rx) = mpsc::channel::<InputMessage>(1);
        let input_handle = InputHandle::new(input_tx);

        let handle = AppActor::spawn(
            minimal_app_with_env(env),
            terminal_handle,
            ui_handle,
            input_handle,
            event_rx,
        );

        let result = handle.run_until_done().await;
        let err = result.unwrap_err();
        assert!(err
            .downcast_ref::<AppError>()
            .is_some_and(|e| matches!(e, AppError::Dependencies(_))));
    }
}
