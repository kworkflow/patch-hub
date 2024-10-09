use std::{ops::ControlFlow, sync::{atomic::AtomicBool, Arc}};

use crate::{app::{screens::CurrentScreen, App}, ui::render_loading_screen};
use ratatui::{crossterm::event::{KeyCode, KeyEvent}, prelude::Backend, Terminal};

pub fn handle_mailing_list_selection<B>(
    mut terminal: Terminal<B>,
    app: &mut App,
    key: KeyEvent,
) -> color_eyre::Result<ControlFlow<(), Terminal<B>>>
where B: Backend + Send + 'static
{
    match key.code {
        KeyCode::Enter => {
            if app.mailing_list_selection_state.has_valid_target_list() {
                app.init_latest_patchsets_state();
                let list_name = app.latest_patchsets_state.as_ref().unwrap().target_list().to_string();

                let loading = Arc::new(AtomicBool::new(true));
                let loading_clone = Arc::clone(&loading);

                let handle = std::thread::spawn(move || {
                    while loading_clone.load(std::sync::atomic::Ordering::Relaxed) {
                        terminal = render_loading_screen(terminal, format!("Fetching latest patchsets for {}", list_name));
                        std::thread::sleep(std::time::Duration::from_millis(50));
                    }

                    terminal
                });

                app.latest_patchsets_state
                    .as_mut()
                    .unwrap()
                    .fetch_current_page()?;
                app.mailing_list_selection_state.clear_target_list();
                app.set_current_screen(CurrentScreen::LatestPatchsets);

                loading.store(false, std::sync::atomic::Ordering::Relaxed);
                terminal = handle.join().unwrap();
            }
        }
        KeyCode::F(5) => {
            //terminal.draw(|f| draw_loading_screen(f, "Refreshing lists"))?;
            app.mailing_list_selection_state
                .refresh_available_mailing_lists()?;
        }
        KeyCode::F(2) => {
            app.init_edit_config_state();
            app.set_current_screen(CurrentScreen::EditConfig);
        }
        KeyCode::F(1) => {
            if !app
                .bookmarked_patchsets_state
                .bookmarked_patchsets
                .is_empty()
            {
                app.mailing_list_selection_state.clear_target_list();
                app.set_current_screen(CurrentScreen::BookmarkedPatchsets);
            }
        }
        KeyCode::Backspace => {
            app.mailing_list_selection_state
                .remove_last_target_list_char();
        }
        KeyCode::Esc => {
            return Ok(ControlFlow::Break(()));
        }
        KeyCode::Char(ch) => {
            app.mailing_list_selection_state
                .push_char_to_target_list(ch);
        }
        KeyCode::Down => {
            app.mailing_list_selection_state.highlight_below_list();
        }
        KeyCode::Up => {
            app.mailing_list_selection_state.highlight_above_list();
        }
        _ => {}
    }
    Ok(ControlFlow::Continue(terminal))
}
