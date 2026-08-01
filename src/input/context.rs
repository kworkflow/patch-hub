use crate::app::screens::CurrentScreen;

/// Snapshot of application state needed to map raw terminal input.
#[derive(Debug, Clone, PartialEq)]
pub struct InputContext {
    pub current_screen: CurrentScreen,
    pub popup_open: bool,
    pub edit_config_editing: bool,
    pub preview_fullscreen: bool,
}

impl InputContext {
    #[cfg(test)]
    pub fn new(current_screen: CurrentScreen) -> Self {
        Self {
            current_screen,
            popup_open: false,
            edit_config_editing: false,
            preview_fullscreen: false,
        }
    }

    #[cfg(test)]
    pub fn with_popup_open(mut self, popup_open: bool) -> Self {
        self.popup_open = popup_open;
        self
    }

    #[cfg(test)]
    pub fn with_edit_config_editing(mut self, edit_config_editing: bool) -> Self {
        self.edit_config_editing = edit_config_editing;
        self
    }
}
