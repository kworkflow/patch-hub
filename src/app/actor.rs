use tokio::sync::mpsc;

use crate::{
    app::{handle::AppHandle, App},
    handler,
    input::{event::InputEvent, handle::InputHandle},
    terminal::handle::TerminalHandle,
    ui::handle::UiHandle,
};

/// Owns `App` state and drives the main application loop on a dedicated task.
///
/// Constructed via [`AppActor::spawn`], which moves all owned resources into
/// the actor and returns an [`AppHandle`] to the caller. The run loop itself
/// is identical to the former `handler::run_app` function; the move into an
/// actor boundary is the only change in this commit.
pub struct AppActor {
    app: App,
    terminal_handle: TerminalHandle,
    ui_handle: UiHandle,
    input_handle: InputHandle,
    event_rx: mpsc::Receiver<InputEvent>,
}

impl AppActor {
    /// Moves all resources into a new `AppActor`, spawns it on the Tokio
    /// runtime, and returns an [`AppHandle`] to wait on its completion.
    pub fn spawn(
        app: App,
        terminal_handle: TerminalHandle,
        ui_handle: UiHandle,
        input_handle: InputHandle,
        event_rx: mpsc::Receiver<InputEvent>,
    ) -> AppHandle {
        let actor = Self {
            app,
            terminal_handle,
            ui_handle,
            input_handle,
            event_rx,
        };
        AppHandle::new(tokio::spawn(actor.run()))
    }

    async fn run(self) -> color_eyre::Result<()> {
        handler::run_app(
            self.app,
            self.terminal_handle,
            self.ui_handle,
            self.input_handle,
            self.event_rx,
        )
        .await
    }
}
