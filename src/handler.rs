pub mod bookmarked;
pub mod details;
pub mod edit_config;
pub mod latest;
pub mod mail_list;

use std::{ops::ControlFlow, sync::{atomic::{AtomicBool, Ordering}, Arc}, thread, time::Duration};

use crate::{
    app::{screens::CurrentScreen, App}, ui::{draw_ui, render_loading_screen}
};

use bookmarked::handle_bookmarked_patchsets;
use details::handle_patchset_details;
use edit_config::handle_edit_config;
use latest::handle_latest_patchsets;
use mail_list::handle_mailing_list_selection;
use ratatui::{
    crossterm::event::{self, Event, KeyEvent, KeyEventKind},
    prelude::Backend,
    Terminal,
};

fn key_handling<B>(
    mut terminal: Terminal<B>,
    app: &mut App,
    key: KeyEvent,
) -> color_eyre::Result<ControlFlow<(), Terminal<B>>> 
where B: Backend + Send + 'static
{
    match app.current_screen {
        CurrentScreen::MailingListSelection => {
            return handle_mailing_list_selection(terminal, app, key);
        }
        CurrentScreen::BookmarkedPatchsets => {
            handle_bookmarked_patchsets(app, key)?;
        }
        CurrentScreen::PatchsetDetails => {
            handle_patchset_details(app, key, &mut terminal)?;
        }
        CurrentScreen::EditConfig => {
            handle_edit_config(app, key)?;
        }
        CurrentScreen::LatestPatchsets => {
            handle_latest_patchsets(app, key)?;
        }
    }
    Ok(ControlFlow::Continue(terminal))
}

fn logic_handling<B>(mut terminal: Terminal<B>, app: &mut App) -> color_eyre::Result<Terminal<B>> 
where B: Backend + Send + 'static {
    match app.current_screen {
        CurrentScreen::MailingListSelection => {
            if app.mailing_list_selection_state.mailing_lists.is_empty() {
                let loading = Arc::new(AtomicBool::new(true));
                let loading_clone = Arc::clone(&loading);
                
                let handle = std::thread::spawn(move || {
                    while loading_clone.load(Ordering::Relaxed) {
                        terminal = render_loading_screen(terminal, "Fetching mailing lists");
                        thread::sleep(Duration::from_millis(50));
                    }

                    terminal
                });

                app.mailing_list_selection_state.refresh_available_mailing_lists()?; // SLOW AF

                loading.store(false, Ordering::Relaxed);
                terminal = handle.join().unwrap();
            }
        }
        CurrentScreen::LatestPatchsets => {
            let patchsets_state = app.latest_patchsets_state.as_mut().unwrap();
            if patchsets_state.processed_patchsets_count() == 0 {
                //terminal.draw(|f| draw_loading_screen(f, format!("Fetching patchsets from {}", patchsets_state.target_list())))?;
                
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

    Ok(terminal)
}

pub fn run_app<B>(mut terminal: Terminal<B>, mut app: App) -> color_eyre::Result<()>
where B: Backend + Send + 'static {
    loop {
        terminal.draw(|f| draw_ui(f, &app))?;

        terminal = logic_handling(terminal, &mut app)?;

        if event::poll(Duration::from_millis(16))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Release {
                    continue;
                }
                match key_handling(terminal, &mut app, key)? {
                    ControlFlow::Continue(t) => terminal = t,
                    ControlFlow::Break(_) => return Ok(()),
                }
            }
        }
    }
}
