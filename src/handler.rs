pub mod bookmarked;
pub mod details_actions;
pub mod edit_config;
pub mod latest;
pub mod mail_list;

use std::{
    ops::ControlFlow,
    time::{Duration, Instant},
};

use crate::{
    app::{logging::Logger, screens::CurrentScreen, App},
    loading_screen,
    ui::draw_ui,
};

use bookmarked::handle_bookmarked_patchsets;
use color_eyre::eyre::bail;
use details_actions::handle_patchset_details;
use edit_config::handle_edit_config;
use latest::handle_latest_patchsets;
use mail_list::handle_mailing_list_selection;
use ratatui::{
    crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind},
    prelude::Backend,
    Terminal,
};

fn key_handling<B>(
    mut terminal: Terminal<B>,
    app: &mut App,
    key: KeyEvent,
) -> color_eyre::Result<ControlFlow<(), Terminal<B>>>
where
    B: Backend + Send + 'static,
{
    if let Some(popup) = app.popup.as_mut() {
        if key.code == KeyCode::Esc {
            app.popup = None;
        } else {
            popup.handle(key)?;
        }
    } else {

        
        match app.current_screen {
            CurrentScreen::MailingListSelection => {
                return handle_mailing_list_selection(app, key, terminal);
            }
            CurrentScreen::BookmarkedPatchsets => {
                return handle_bookmarked_patchsets(app, key, terminal);
            }
            CurrentScreen::PatchsetDetails => {
                handle_patchset_details(app, key, &mut terminal)?;
            }
            CurrentScreen::EditConfig => {
                handle_edit_config(app, key)?;
            }
            CurrentScreen::LatestPatchsets => {
                return handle_latest_patchsets(app, key, terminal);
            }
        }
    }
    Ok(ControlFlow::Continue(terminal))
}

fn logic_handling<B>(mut terminal: Terminal<B>, app: &mut App) -> color_eyre::Result<Terminal<B>>
where
    B: Backend + Send + 'static,
{
    match app.current_screen {
        CurrentScreen::MailingListSelection => {
            if app.mailing_list_selection_state.mailing_lists.is_empty() {
                terminal = loading_screen! {
                    terminal, "Fetching mailing lists" => {
                        app.mailing_list_selection_state.refresh_available_mailing_lists()?;
                    }
                };
            }
        }
        CurrentScreen::LatestPatchsets => {
            let patchsets_state = app.latest_patchsets_state.as_mut().unwrap();
            let target_list = patchsets_state.target_list().to_string();
            if patchsets_state.processed_patchsets_count() == 0 {
                terminal = loading_screen! {
                    terminal,
                    format!("Fetching patchsets from {}", target_list) => {
                        patchsets_state.fetch_current_page()?;
                    }
                };

                app.mailing_list_selection_state.clear_target_list();
            }
        }
        CurrentScreen::BookmarkedPatchsets => {
            if app
                .bookmarked_patchsets_state
                .bookmarked_patchsets
                .is_empty()
            {
                app.set_current_screen(CurrentScreen::MailingListSelection);
            }
        }
        _ => {}
    }

    Ok(terminal)
}

pub fn run_app<B>(mut terminal: Terminal<B>, mut app: App) -> color_eyre::Result<()>
where
    B: Backend + Send + 'static,
{
    if !app.check_external_deps() {
        Logger::error("patch-hub cannot be executed because some dependencies are missing");
        bail!("patch-hub cannot be executed because some dependencies are missing, check logs for more information");
    }

    loop {
        terminal.draw(|f| draw_ui(f, &app))?;

        terminal = logic_handling(terminal, &mut app)?;

        // *IMPORTANT*: Uncommenting the if below makes `patch-hub` not block
        // until an event is captured.  We should only do it when (if ever) we
        // need to refresh the UI independently of any event as doing so gravely
        // hinders the performance to below acceptable.
        // if event::poll(Duration::from_millis(16))? {
        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Release {
                continue;
            }
            match key_handling(terminal, &mut app, key)? {
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
        if event::poll(Duration::from_millis(16))? {
            if let Event::Key(key) = event::read()? {
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
