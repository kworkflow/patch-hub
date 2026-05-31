mod bookmarked;
mod details_actions;
mod edit_config;
mod latest;
mod mail_list;

use ratatui::{
    crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind},
    prelude::Backend,
    Terminal,
};

use std::{
    ops::ControlFlow,
    time::{Duration, Instant},
};

use crate::{
    app::{screens::CurrentScreen, App},
    input::{
        event::{InputEvent, TerminalEvent},
        terminal_source::{CrosstermEventSource, TerminalEventSource},
    },
    ui::draw_ui,
};

use bookmarked::handle_bookmarked_patchsets;
use details_actions::handle_patchset_details;
use edit_config::handle_edit_config;
use latest::handle_latest_patchsets;
use mail_list::handle_mailing_list_selection;

async fn key_handling<B>(
    mut terminal: Terminal<B>,
    app: &mut App,
    key: KeyEvent,
) -> color_eyre::Result<ControlFlow<(), Terminal<B>>>
where
    B: Backend + Send + 'static,
{
    if let Some(popup) = app.state.popup.as_mut() {
        if matches!(key.code, KeyCode::Esc | KeyCode::Char('q')) {
            app.state.popup = None;
        } else if let Some(input) = popup_input_from_key(key) {
            popup.handle(input)?;
        }
    } else {
        match app.state.navigation.current_screen {
            CurrentScreen::MailingListSelection => {
                return handle_mailing_list_selection(app, key, terminal).await;
            }
            CurrentScreen::BookmarkedPatchsets => {
                return handle_bookmarked_patchsets(app, key, terminal).await;
            }
            CurrentScreen::PatchsetDetails => {
                handle_patchset_details(app, key, &mut terminal).await?;
            }
            CurrentScreen::EditConfig => {
                handle_edit_config(app, key)?;
            }
            CurrentScreen::LatestPatchsets => {
                return handle_latest_patchsets(app, key, terminal).await;
            }
        }
    }
    Ok(ControlFlow::Continue(terminal))
}

fn popup_input_from_key(key: KeyEvent) -> Option<InputEvent> {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => Some(InputEvent::NavigateDown),
        KeyCode::Char('k') | KeyCode::Up => Some(InputEvent::NavigateUp),
        KeyCode::Char('h') | KeyCode::Left => Some(InputEvent::NavigateLeft),
        KeyCode::Char('l') | KeyCode::Right => Some(InputEvent::NavigateRight),
        _ => None,
    }
}

pub async fn run_app<B>(mut terminal: Terminal<B>, mut app: App) -> color_eyre::Result<()>
where
    B: Backend + Send + 'static,
{
    let mut event_source = CrosstermEventSource;

    loop {
        terminal = app.process_system_updates(terminal).await?;

        terminal.draw(|f| draw_ui(f, &app.to_view_model()))?;

        // *IMPORTANT*: Uncommenting the if below makes `patch-hub` not block
        // until an event is captured.  We should only do it when (if ever) we
        // need to refresh the UI independently of any event as doing so gravely
        // hinders the performance to below acceptable.
        // if event::poll(Duration::from_millis(16))? {
        if let Some(TerminalEvent::Key(key)) = event_source.read_event()? {
            let key = key.to_key_event();
            match key_handling(terminal, &mut app, key).await? {
                ControlFlow::Continue(t) => terminal = t,
                ControlFlow::Break(_) => return Ok(()),
            }
        }
        // }
    }
}

fn wait_key_press(ch: char, wait_time: Duration) -> color_eyre::Result<bool> {
    let start = Instant::now();

    while Instant::now() - start < wait_time {
        if ratatui::crossterm::event::poll(Duration::from_millis(16))? {
            if let Event::Key(key) = ratatui::crossterm::event::read()? {
                if key.kind == KeyEventKind::Release {
                    continue;
                }
                if key.code == KeyCode::Char(ch) {
                    return Ok(true);
                }
            }
        }
    }

    Ok(false)
}
