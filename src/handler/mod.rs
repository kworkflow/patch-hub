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
    loading_screen,
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
        } else {
            popup.handle(key)?;
        }
    } else {
        match app.state.navigation.current_screen {
            CurrentScreen::MailingListSelection => {
                return handle_mailing_list_selection(app, key, terminal).await;
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
                return handle_latest_patchsets(app, key, terminal).await;
            }
        }
    }
    Ok(ControlFlow::Continue(terminal))
}

async fn logic_handling<B>(
    mut terminal: Terminal<B>,
    app: &mut App,
) -> color_eyre::Result<Terminal<B>>
where
    B: Backend + Send + 'static,
{
    match app.state.navigation.current_screen {
        CurrentScreen::MailingListSelection => {
            if app
                .state
                .lore
                .mailing_list_selection
                .mailing_lists
                .is_empty()
            {
                terminal = loading_screen! {
                    terminal, "Fetching mailing lists" => {
                        app.refresh_mailing_lists().await
                    }
                };
            }
        }
        CurrentScreen::LatestPatchsets => {
            let patchsets_state = app.state.lore.latest_patchsets.as_ref().unwrap();

            if patchsets_state.processed_patchsets_count() == 0 {
                let target_list = patchsets_state.target_list().to_string();
                terminal = loading_screen! {
                    terminal,
                    format!("Fetching patchsets from {}", target_list) => {
                        app.fetch_latest_current_page().await
                    }
                };

                app.state.lore.mailing_list_selection.clear_target_list();
            }
        }
        CurrentScreen::BookmarkedPatchsets => {
            if app
                .state
                .user_state
                .bookmarked_patchsets
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

pub async fn run_app<B>(mut terminal: Terminal<B>, mut app: App) -> color_eyre::Result<()>
where
    B: Backend + Send + 'static,
{
    loop {
        terminal = logic_handling(terminal, &mut app).await?;

        terminal.draw(|f| draw_ui(f, &app.to_view_model()))?;

        // *IMPORTANT*: Uncommenting the if below makes `patch-hub` not block
        // until an event is captured.  We should only do it when (if ever) we
        // need to refresh the UI independently of any event as doing so gravely
        // hinders the performance to below acceptable.
        // if event::poll(Duration::from_millis(16))? {
        if let Event::Key(key) = ratatui::crossterm::event::read()? {
            if key.kind == KeyEventKind::Release {
                continue;
            }
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
