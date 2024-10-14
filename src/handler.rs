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
    app::{screens::CurrentScreen, App},
    ui::draw_ui,
};

use bookmarked::handle_bookmarked_patchsets;
use details_actions::handle_patchset_details;
use edit_config::handle_edit_config;
use latest::handle_latest_patchsets;
use mail_list::handle_mailing_list_selection;
use ratatui::{
    crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind},
    prelude::Backend,
    Terminal,
};

fn key_handling<B: Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    key: KeyEvent,
) -> color_eyre::Result<ControlFlow<(), ()>> {
    match app.current_screen {
        CurrentScreen::MailingListSelection => {
            return handle_mailing_list_selection(app, key);
        }
        CurrentScreen::BookmarkedPatchsets => {
            handle_bookmarked_patchsets(app, key)?;
        }
        CurrentScreen::PatchsetDetails => {
            handle_patchset_details(app, key, terminal)?;
        }
        CurrentScreen::EditConfig => {
            handle_edit_config(app, key)?;
        }
        CurrentScreen::LatestPatchsets => {
            handle_latest_patchsets(app, key)?;
        }
    }
    Ok(ControlFlow::Continue(()))
}

fn logic_handling(app: &mut App) -> color_eyre::Result<()> {
    match app.current_screen {
        CurrentScreen::MailingListSelection => {
            if app.mailing_list_selection_state.mailing_lists.is_empty() {
                app.mailing_list_selection_state
                    .refresh_available_mailing_lists()?;
            }
        }
        CurrentScreen::LatestPatchsets => {
            let patchsets_state = app.latest_patchsets_state.as_mut().unwrap();
            if patchsets_state.processed_patchsets_count() == 0 {
                patchsets_state.fetch_current_page()?;
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

    Ok(())
}

pub fn run_app<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> color_eyre::Result<()> {
    loop {
        terminal.draw(|f| draw_ui(f, app))?;

        logic_handling(app)?;

        if event::poll(Duration::from_millis(16))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Release {
                    continue;
                }
                if key_handling(terminal, app, key)? == ControlFlow::Break(()) {
                    return Ok(());
                }
            }
        }
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
