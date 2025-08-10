use ratatui::crossterm::event::{KeyCode, KeyEvent};

use crate::{
    model::Model,
    views::{
        popup::{help::HelpPopUpBuilder, PopUp},
        View,
    },
};

pub fn handle_edit_config(model: &mut Model, key: KeyEvent) -> color_eyre::Result<()> {
    if let Some(edit_config_state) = model.edit_config.as_mut() {
        match edit_config_state.is_editing() {
            true => match key.code {
                KeyCode::Esc => {
                    edit_config_state.clear_edit();
                    edit_config_state.toggle_editing();
                }
                KeyCode::Backspace => {
                    edit_config_state.backspace_edit();
                }
                KeyCode::Char(ch) => {
                    edit_config_state.append_edit(ch);
                }
                KeyCode::Enter => {
                    edit_config_state.stage_edit();
                    edit_config_state.clear_edit();
                    edit_config_state.toggle_editing();
                }
                _ => {}
            },
            false => match key.code {
                KeyCode::Char('?') => {
                    let popup = generate_help_popup();
                    model.popup = Some(popup);
                }
                KeyCode::Esc | KeyCode::Char('q') => {
                    model.consolidate_edit_config();
                    model.config.save_patch_hub_config()?;
                    model.reset_edit_config();
                    model.set_current_screen(View::MailingLists);
                }
                KeyCode::Enter => {
                    edit_config_state.toggle_editing();
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    edit_config_state.highlight_next();
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    edit_config_state.highlight_prev();
                }
                _ => {}
            },
        }
    }
    Ok(())
}

// TODO: Move this to a more appropriate place
pub fn generate_help_popup() -> Box<dyn PopUp> {
    let popup = HelpPopUpBuilder::new()
        .title("Edit Config")
        .description("This screen allows you to edit the configuration options for patch-hub.\nMore configurations may be available in the configuration file.")
        .keybind("ESC", "Exit")
        .keybind("ENTER", "Save changes")
        .keybind("?", "Show this help screen")
        .keybind("j/🡇", "Down")
        .keybind("k/🡅", "Up")
        .keybind("e", "Toggle editing for a configuration option")
        .build();

    Box::new(popup)
}
