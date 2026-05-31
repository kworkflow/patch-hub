mod bookmarked;
mod details_actions;
mod edit_config;
mod latest;
mod mail_list;

use std::{future::Future, ops::ControlFlow};

use crate::{
    app::{screens::CurrentScreen, App},
    input::{event::InputEvent, mapper::InputMapper},
    terminal::{
        handle::TerminalHandle,
        messages::{TerminalFrame, TerminalResult},
        TerminalError,
    },
};

use bookmarked::handle_bookmarked_patchsets;
use details_actions::handle_patchset_details;
use edit_config::handle_edit_config;
use latest::handle_latest_patchsets;
use mail_list::handle_mailing_list_selection;

pub(crate) trait LoadingIndicator {
    fn start(&mut self, title: String);
    fn stop(&mut self) -> color_eyre::Result<()>;
}

pub(crate) trait TerminalController {
    fn setup_user_io(&mut self) -> color_eyre::Result<()>;
    fn teardown_user_io(&mut self) -> color_eyre::Result<()>;
    fn size(&self) -> color_eyre::Result<(u16, u16)>;
}

struct TerminalLoadingIndicator {
    terminal_handle: TerminalHandle,
}

impl TerminalLoadingIndicator {
    fn new(terminal_handle: TerminalHandle) -> Self {
        Self { terminal_handle }
    }
}

impl LoadingIndicator for TerminalLoadingIndicator {
    fn start(&mut self, _title: String) {
        // Handle-backed loading frames are wired in the next Phase 9 commit.
    }

    fn stop(&mut self) -> color_eyre::Result<()> {
        Ok(())
    }
}

impl TerminalController for TerminalLoadingIndicator {
    fn setup_user_io(&mut self) -> color_eyre::Result<()> {
        terminal_handle_call(self.terminal_handle.setup_user_io())
    }

    fn teardown_user_io(&mut self) -> color_eyre::Result<()> {
        terminal_handle_call(self.terminal_handle.teardown_user_io())
    }

    fn size(&self) -> color_eyre::Result<(u16, u16)> {
        terminal_handle_call(self.terminal_handle.size())
    }
}

fn terminal_handle_call<T>(
    future: impl Future<Output = TerminalResult<T>>,
) -> color_eyre::Result<T> {
    tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(future))
        .map_err(|error| color_eyre::eyre::eyre!("{error}"))
}

fn terminal_error(error: TerminalError) -> color_eyre::Report {
    color_eyre::eyre::eyre!("{error}")
}

async fn input_handling(
    app: &mut App,
    input: InputEvent,
    terminal_handle: &TerminalHandle,
    loading: &mut TerminalLoadingIndicator,
) -> color_eyre::Result<ControlFlow<()>> {
    if let Some(popup) = app.state.popup.as_mut() {
        if input == InputEvent::ClosePopup {
            app.state.popup = None;
        } else {
            popup.handle(input)?;
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
                handle_patchset_details(app, input, loading, terminal_handle).await?;
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

pub async fn run_app(mut app: App, terminal_handle: TerminalHandle) -> color_eyre::Result<()> {
    let mut input_mapper = InputMapper::default();
    let mut loading = TerminalLoadingIndicator::new(terminal_handle.clone());

    loop {
        app.process_system_updates(&mut loading).await?;

        terminal_handle
            .draw(TerminalFrame::Main(Box::new(app.render_snapshot())))
            .await
            .map_err(terminal_error)?;

        if let Some(terminal_event) = terminal_handle.read_event().await.map_err(terminal_error)? {
            let input = input_mapper.map_terminal_event(terminal_event, &app.input_context());
            if let Some(input) = input {
                match input_handling(&mut app, input, &terminal_handle, &mut loading).await? {
                    ControlFlow::Continue(()) => {}
                    ControlFlow::Break(()) => return Ok(()),
                }
            }
        }
    }
}
