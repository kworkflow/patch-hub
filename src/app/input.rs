use crate::{app::popup::AppPopup, app::App, input::context::InputContext};

impl App {
    /// Projects App state into the context needed by the input mapper.
    pub fn input_context(&self) -> InputContext {
        InputContext {
            current_screen: self.state.navigation.current_screen.clone(),
            popup_open: self.state.popup.is_some(),
            confirm_popup_open: matches!(self.state.popup, Some(AppPopup::Confirm { .. })),
            edit_config_editing: self
                .state
                .config_state
                .edit_config
                .as_ref()
                .is_some_and(|edit_config| edit_config.is_editing()),
            kw_ops_editing: self.state.kw.ops.as_ref().is_some_and(|ops| ops.editing),
            preview_fullscreen: self
                .state
                .lore
                .details
                .as_ref()
                .is_some_and(|details| details.preview_fullscreen),
        }
    }
}
