use crate::{app::App, input::context::InputContext};

impl App {
    /// Projects App state into the context needed by the input mapper.
    pub fn input_context(&self) -> InputContext {
        InputContext {
            current_screen: self.state.navigation.current_screen.clone(),
            popup_open: self.state.popup.is_some(),
            edit_config_editing: self
                .state
                .config_state
                .edit_config
                .as_ref()
                .map(|edit_config| edit_config.is_editing())
                .unwrap_or(false),
            preview_fullscreen: self
                .state
                .lore
                .details
                .as_ref()
                .map(|details| details.preview_fullscreen)
                .unwrap_or(false),
        }
    }
}
