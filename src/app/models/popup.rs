/// Action taken when the user confirms a choice popup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfirmAction {
    CancelKwAndQuit,
    Wait,
    /// User accepted kw's one-shot boot into the new kernel.
    ProceedWithBootOnce,
    /// User declined the boot-once gate; the pending deploy is dropped.
    BackOut,
}

/// Concrete, cloneable popup state stored in `AppState`.
#[derive(Clone, Debug)]
pub enum AppPopup {
    Info {
        title: String,
        body: String,
        scroll: (u16, u16),
        max_scroll: (u16, u16),
        dimensions: (u16, u16),
    },
    Help {
        title: Option<String>,
        description: Option<String>,
        formatted_keybinds: String,
        scroll: (u16, u16),
        max_scroll: (u16, u16),
        dimensions: (u16, u16),
    },
    ReviewTrailers {
        reviewed_by: String,
        tested_by: String,
        acked_by: String,
        scroll: (u16, u16),
        max_scroll: (u16, u16),
        dimensions: (u16, u16),
    },
    Confirm {
        title: String,
        body: String,
        options: Vec<(String, ConfirmAction)>,
        selected: usize,
        dimensions: (u16, u16),
    },
}

/// Fluent builder for `AppPopup::Help`.
///
/// Mirrors the API of the old `HelpPopUpBuilder` so handler call sites change
/// minimally.
#[derive(Default)]
pub struct AppHelpBuilder {
    pub(crate) title: Option<String>,
    pub(crate) description: Option<String>,
    pub(crate) keybinds: Vec<(String, String)>,
}
