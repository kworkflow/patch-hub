use crate::app::{screens::CurrentScreen, App};
use ratatui::crossterm::event::{KeyCode, KeyEvent};

pub fn handle_edit_config(app: &mut App, key: KeyEvent) -> color_eyre::Result<()> {
    if let Some(edit_config_state) = app.edit_config_state.as_mut() {
        match edit_config_state.is_editing() {
            true => match key.code {
                KeyCode::Esc => {
                    edit_config_state.clear_editing_val();
                    edit_config_state.toggle_editing();
                }
                KeyCode::Backspace => {
                    edit_config_state.remove_char_from_editing_val();
                }
                KeyCode::Char(ch) => {
                    edit_config_state.add_char_to_editing_val(ch);
                }
                KeyCode::Enter => {
                    edit_config_state.push_editing_val_to_buffer();
                    edit_config_state.clear_editing_val();
                    edit_config_state.toggle_editing();
                }
                _ => {}
            },
            false => match key.code {
                KeyCode::Esc => {
                    app.reset_edit_config_state();
                    app.set_current_screen(CurrentScreen::MailingListSelection);
                }
                KeyCode::Char('e') => {
                    edit_config_state.toggle_editing();
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    edit_config_state.highlight_below_entry();
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    edit_config_state.highlight_above_entry();
                }
                KeyCode::Enter => {
                    app.consolidate_edit_config();
                    app.config.save_patch_hub_config()?;
                    app.reset_edit_config_state();
                    app.set_current_screen(CurrentScreen::MailingListSelection);
                }
                _ => {}
            },
        }
    }
    Ok(())
}
