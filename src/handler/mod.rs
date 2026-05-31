mod bookmarked;
mod details_actions;
mod edit_config;
mod latest;
mod mail_list;

use ratatui::{prelude::Backend, Terminal};

use std::ops::ControlFlow;

use crate::{
    app::{screens::CurrentScreen, App},
    input::{event::InputEvent, mapper::InputMapper},
    terminal::handle::TerminalHandle,
    ui::draw_ui,
};

use bookmarked::handle_bookmarked_patchsets;
use details_actions::handle_patchset_details;
use edit_config::handle_edit_config;
use latest::handle_latest_patchsets;
use mail_list::handle_mailing_list_selection;

async fn input_handling<B>(
    mut terminal: Terminal<B>,
    app: &mut App,
    input: InputEvent,
) -> color_eyre::Result<ControlFlow<(), Terminal<B>>>
where
    B: Backend + Send + 'static,
{
    if let Some(popup) = app.state.popup.as_mut() {
        if input == InputEvent::ClosePopup {
            app.state.popup = None;
        } else {
            popup.handle(input)?;
        }
    } else {
        match app.state.navigation.current_screen {
            CurrentScreen::MailingListSelection => {
                return handle_mailing_list_selection(app, input, terminal).await;
            }
            CurrentScreen::BookmarkedPatchsets => {
                return handle_bookmarked_patchsets(app, input, terminal).await;
            }
            CurrentScreen::PatchsetDetails => {
                handle_patchset_details(app, input, &mut terminal).await?;
            }
            CurrentScreen::EditConfig => {
                handle_edit_config(app, input)?;
            }
            CurrentScreen::LatestPatchsets => {
                return handle_latest_patchsets(app, input, terminal).await;
            }
        }
    }
    Ok(ControlFlow::Continue(terminal))
}

pub async fn run_app<B>(
    mut terminal: Terminal<B>,
    mut app: App,
    terminal_handle: TerminalHandle,
) -> color_eyre::Result<()>
where
    B: Backend + Send + 'static,
{
    let mut input_mapper = InputMapper::default();

    loop {
        terminal = app.process_system_updates(terminal).await?;

        terminal.draw(|f| draw_ui(f, &app.to_view_model()))?;

        // *IMPORTANT*: Uncommenting the if below makes `patch-hub` not block
        // until an event is captured.  We should only do it when (if ever) we
        // need to refresh the UI independently of any event as doing so gravely
        // hinders the performance to below acceptable.
        // if event::poll(Duration::from_millis(16))? {
        if let Some(terminal_event) = terminal_handle.read_event().await? {
            let input = input_mapper.map_terminal_event(terminal_event, &app.input_context());
            if let Some(input) = input {
                match input_handling(terminal, &mut app, input).await? {
                    ControlFlow::Continue(t) => terminal = t,
                    ControlFlow::Break(_) => return Ok(()),
                }
            }
        }
        // }
    }
}
