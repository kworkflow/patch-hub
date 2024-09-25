use std::ops::ControlFlow;

use crate::app::{screens::CurrentScreen, App};
use ratatui::crossterm::event::{KeyCode, KeyEvent};

pub fn handle_mailing_list_selection(
    app: &mut App,
    key: KeyEvent,
) -> color_eyre::Result<ControlFlow<(), ()>> {
    match key.code {
        KeyCode::Enter => {
            if app.mailing_list_selection_state.has_valid_target_list() {
                app.init_latest_patchsets_state();
                app.latest_patchsets_state
                    .as_mut()
                    .unwrap()
                    .fetch_current_page()?;
                app.mailing_list_selection_state.clear_target_list();
                app.set_current_screen(CurrentScreen::LatestPatchsets);
            }
        }
        KeyCode::F(5) => {
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
    Ok(ControlFlow::Continue(()))
}
