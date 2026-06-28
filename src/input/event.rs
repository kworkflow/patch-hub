use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

/// Raw terminal event after conversion from the terminal backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalEvent {
    Key(KeyInput),
    Resize { width: u16, height: u16 },
}

/// Key data kept close to crossterm while avoiding `KeyEvent` outside input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyInput {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
    pub kind: KeyEventKind,
}

impl KeyInput {
    #[cfg(test)]
    pub fn new(code: KeyCode, modifiers: KeyModifiers, kind: KeyEventKind) -> Self {
        Self {
            code,
            modifiers,
            kind,
        }
    }

    #[cfg(test)]
    pub fn press(code: KeyCode) -> Self {
        Self::new(code, KeyModifiers::NONE, KeyEventKind::Press)
    }

    #[cfg(test)]
    pub fn modified_press(code: KeyCode, modifiers: KeyModifiers) -> Self {
        Self::new(code, modifiers, KeyEventKind::Press)
    }
}

impl From<KeyEvent> for KeyInput {
    fn from(key: KeyEvent) -> Self {
        Self {
            code: key.code,
            modifiers: key.modifiers,
            kind: key.kind,
        }
    }
}

/// Semantic input command consumed by application code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputEvent {
    Quit,
    Back,
    ClosePopup,
    OpenHelp,
    TextInput(char),
    Backspace,
    NavigateUp,
    NavigateDown,
    NavigateLeft,
    NavigateRight,
    RefreshMailingLists,
    OpenBookmarkedPatchsets,
    OpenEditConfig,
    OpenLatestPatchsets,
    OpenPatchsetDetails,
    PreviousPage,
    NextPage,
    EditConfigField,
    StageConfigEdit,
    CancelConfigEdit,
    SaveConfig,
    ToggleBookmark,
    ToggleReplyWithReviewedBy,
    ToggleReplyWithReviewedByAll,
    ToggleApply,
    ConsolidatePatchsetActions,
    PreviewNext,
    PreviewPrevious,
    PreviewScrollUp(ScrollAmount),
    PreviewScrollDown(ScrollAmount),
    PreviewPanLeft,
    PreviewPanRight,
    PreviewGoToBeginningOfLine,
    PreviewGoToFirstLine,
    PreviewGoToLastLine,
    TogglePreviewFullscreen,
    ShowReviewTrailers,
    Resize { width: u16, height: u16 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollAmount {
    Line,
    HalfPage,
    Page,
}
