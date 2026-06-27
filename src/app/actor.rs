use std::ops::ControlFlow;

use tokio::sync::mpsc;

use crate::{
    app::{
        flows::{
            bookmarked::handle_bookmarked_patchsets,
            details_actions::handle_patchset_details,
            edit_config::handle_edit_config,
            latest::handle_latest_patchsets,
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

    async fn run(mut self) -> color_eyre::Result<()> {
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

            match self.event_rx.recv().await {
                Some(input) => {
                    match on_input(&mut self.app, input, &self.terminal_handle, &mut loading)
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
