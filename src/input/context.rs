use crate::app::screens::CurrentScreen;

/// Snapshot of application state needed to map raw terminal input.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct InputContext {
    pub current_screen: CurrentScreen,
    pub popup_open: bool,
    pub confirm_popup_open: bool,
    pub edit_config_editing: bool,
    pub kw_ops_editing: bool,
    pub preview_fullscreen: bool,
}
