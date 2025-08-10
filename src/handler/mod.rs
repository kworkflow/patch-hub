mod bookmarked;
mod details_actions;
mod edit_config;
mod latest;
mod mail_list;

use ratatui::{
    crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind},
    prelude::Backend,
    Terminal,
};

use std::{
    ops::ControlFlow,
    time::{Duration, Instant},
};

use crate::{
    loading_screen,
    model::Model,
    views::{draw_ui, View},
};

use bookmarked::handle_bookmarked_patchsets;
use details_actions::handle_patchset_details;
use edit_config::handle_edit_config;
use latest::handle_latest_patchsets;
use mail_list::handle_mailing_list_selection;

fn key_handling<B>(
    mut terminal: Terminal<B>,
    model: &mut Model,
    key: KeyEvent,
) -> color_eyre::Result<ControlFlow<(), Terminal<B>>>
where
    B: Backend + Send + 'static,
{
    if let Some(popup) = model.popup.as_mut() {
        if matches!(key.code, KeyCode::Esc | KeyCode::Char('q')) {
            model.popup = None;
        } else {
            popup.handle(key)?;
        }
    } else {
        match model.current_screen {
            View::MailingLists => {
                return handle_mailing_list_selection(model, key, terminal);
            }
            View::BookmarkedPatchsets => {
                return handle_bookmarked_patchsets(model, key, terminal);
            }
            View::PatchsetDetails => {
                handle_patchset_details(model, key, &mut terminal)?;
            }
            View::EditConfig => {
                handle_edit_config(model, key)?;
            }
            View::LatestPatchsets => {
                return handle_latest_patchsets(model, key, terminal);
            }
        }
    }
    Ok(ControlFlow::Continue(terminal))
}

fn logic_handling<B>(
    mut terminal: Terminal<B>,
    model: &mut Model,
) -> color_eyre::Result<Terminal<B>>
where
    B: Backend + Send + 'static,
{
    match model.current_screen {
        View::MailingLists => {
            if model.mailing_list_selection.mailing_lists.is_empty() {
                terminal = loading_screen! {
                    terminal, "Fetching mailing lists" => {
                        model.mailing_list_selection.refresh_available_mailing_lists()
                    }
                };
            }
        }
        View::LatestPatchsets => {
            let patchsets_state = model.latest_patchsets.as_mut().unwrap();
            let target_list = patchsets_state.target_list().to_string();
            if patchsets_state.processed_patchsets_count() == 0 {
                terminal = loading_screen! {
                    terminal,
                    format!("Fetching patchsets from {}", target_list) => {
                        patchsets_state.fetch_current_page()
                    }
                };

                model.mailing_list_selection.clear_target_list();
            }
        }
        View::BookmarkedPatchsets => {
            if model.bookmarked_patchsets.bookmarked_patchsets.is_empty() {
                model.set_current_screen(View::MailingLists);
            }
        }
        _ => {}
    }

    Ok(terminal)
}

pub fn run_app<B>(mut terminal: Terminal<B>, mut model: Model) -> color_eyre::Result<()>
where
    B: Backend + Send + 'static,
{
    loop {
        terminal = logic_handling(terminal, &mut model)?;

        terminal.draw(|f| draw_ui(f, &model))?;

        // *IMPORTANT*: Uncommenting the if below makes `patch-hub` not block
        // until an event is captured.  We should only do it when (if ever) we
        // need to refresh the UI independently of any event as doing so gravely
        // hinders the performance to below acceptable.
        // if event::poll(Duration::from_millis(16))? {
        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Release {
                continue;
            }
            match key_handling(terminal, &mut model, key)? {
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
