use color_eyre::Result;
use tracing::debug;

use crate::{
    app::{popup::AppPopup, screens::CurrentScreen, App},
    input::event::InputEvent,
};

pub async fn handle_edit_config(app: &mut App, input: InputEvent) -> Result<()> {
    let Some(is_editing) = app
        .state
        .config_state
        .edit_config
        .as_ref()
        .map(|edit_config_state| edit_config_state.is_editing())
    else {
        return Ok(());
    };

    match is_editing {
        true => {
            if let Some(edit_config_state) = app.state.config_state.edit_config.as_mut() {
                match input {
                    InputEvent::CancelConfigEdit => {
                        edit_config_state.clear_edit();
                        edit_config_state.toggle_editing();
                    }
                    InputEvent::Backspace => {
                        edit_config_state.backspace_edit();
                    }
                    InputEvent::TextInput(ch) => {
                        edit_config_state.append_edit(ch);
                    }
                    InputEvent::StageConfigEdit => {
                        edit_config_state.stage_edit();
                        edit_config_state.clear_edit();
                        edit_config_state.toggle_editing();
                    }
                    _ => {}
                }
            }
        }
        false => match input {
            InputEvent::OpenHelp => {
                let popup = generate_help_popup();
                app.state.popup = Some(popup);
            }
            InputEvent::SaveConfig => {
                debug!("saving edited configuration");
                app.consolidate_edit_config().await?;
                app.reset_edit_config();
                app.set_current_screen(CurrentScreen::MailingListSelection);
            }
            InputEvent::EditConfigField => {
                if let Some(edit_config_state) = app.state.config_state.edit_config.as_mut() {
                    edit_config_state.toggle_editing();
                }
            }
            InputEvent::NavigateDown => {
                if let Some(edit_config_state) = app.state.config_state.edit_config.as_mut() {
                    edit_config_state.highlight_next();
                }
            }
            InputEvent::NavigateUp => {
                if let Some(edit_config_state) = app.state.config_state.edit_config.as_mut() {
                    edit_config_state.highlight_prev();
                }
            }
            _ => {}
        },
    }
    Ok(())
}

pub fn generate_help_popup() -> AppPopup {
    AppPopup::help()
        .title("Edit Config")
        .description("This screen allows you to edit the configuration options for patch-hub.\nMore configurations may be available in the configuration file.")
        .keybind("ESC", "Exit")
        .keybind("ENTER", "Save changes")
        .keybind("?", "Show this help screen")
        .keybind("j/🡇", "Down")
        .keybind("k/🡅", "Up")
        .keybind("e", "Toggle editing for a configuration option")
        .build()
}
