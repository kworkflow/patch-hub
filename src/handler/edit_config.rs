use crate::{
    app::{screens::CurrentScreen, App},
    input::event::InputEvent,
    ui::popup::{help::HelpPopUpBuilder, PopUp},
};

pub fn handle_edit_config(app: &mut App, input: InputEvent) -> color_eyre::Result<()> {
    if let Some(edit_config_state) = app.state.config_state.edit_config.as_mut() {
        match edit_config_state.is_editing() {
            true => match input {
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
            },
            false => match input {
                InputEvent::OpenHelp => {
                    let popup = generate_help_popup();
                    app.state.popup = Some(popup);
                }
                InputEvent::SaveConfig => {
                    app.consolidate_edit_config()?;
                    app.reset_edit_config();
                    app.set_current_screen(CurrentScreen::MailingListSelection);
                }
                InputEvent::EditConfigField => {
                    edit_config_state.toggle_editing();
                }
                InputEvent::NavigateDown => {
                    edit_config_state.highlight_next();
                }
                InputEvent::NavigateUp => {
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
