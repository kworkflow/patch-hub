use std::time::Instant;

use ratatui::crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};

use crate::{
    app::screens::CurrentScreen,
    input::{
        bindings::KeyBindings,
        context::InputContext,
        event::{InputEvent, KeyInput, ScrollAmount, TerminalEvent},
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PendingInput {
    DetailsGoToFirstLine { started_at: Instant },
}

/// Maps terminal events into semantic input events.
#[derive(Debug, Clone)]
pub struct InputMapper {
    bindings: KeyBindings,
    pending: Option<PendingInput>,
}

impl InputMapper {
    pub fn new(bindings: KeyBindings) -> Self {
        Self {
            bindings,
            pending: None,
        }
    }

    pub fn map_terminal_event(
        &mut self,
        event: TerminalEvent,
        context: &InputContext,
    ) -> Option<InputEvent> {
        self.clear_expired_pending_input();

        match event {
            TerminalEvent::Key(key) => self.map_key_input(key, context),
            TerminalEvent::Resize { width, height } => Some(InputEvent::Resize { width, height }),
        }
    }
}

impl InputMapper {
    fn map_key_input(&mut self, key: KeyInput, context: &InputContext) -> Option<InputEvent> {
        if key.kind == KeyEventKind::Release {
            return None;
        }

        if context.popup_open {
            return self.map_popup_key(&key, context);
        }

        match &context.current_screen {
            CurrentScreen::MailingListSelection => self.map_mailing_list_key(&key),
            CurrentScreen::BookmarkedPatchsets => self.map_bookmarked_key(&key),
            CurrentScreen::LatestPatchsets => self.map_latest_key(&key),
            CurrentScreen::PatchsetDetails => self.map_details_key(&key),
            CurrentScreen::EditConfig => self.map_edit_config_key(&key, context),
            CurrentScreen::KwOps => self.map_kw_ops_key(&key, context),
        }
    }

    fn map_popup_key(&mut self, key: &KeyInput, context: &InputContext) -> Option<InputEvent> {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => Some(InputEvent::ClosePopup),
            KeyCode::Enter if context.confirm_popup_open => Some(InputEvent::ConfirmPopup),
            KeyCode::Char('j') | KeyCode::Down => Some(InputEvent::NavigateDown),
            KeyCode::Char('k') | KeyCode::Up => Some(InputEvent::NavigateUp),
            KeyCode::Char('h') | KeyCode::Left => Some(InputEvent::NavigateLeft),
            KeyCode::Char('l') | KeyCode::Right => Some(InputEvent::NavigateRight),
            _ => None,
        }
    }

    fn map_mailing_list_key(&mut self, key: &KeyInput) -> Option<InputEvent> {
        match key.code {
            KeyCode::Char('?') => Some(InputEvent::OpenHelp),
            KeyCode::Enter => Some(InputEvent::OpenLatestPatchsets),
            KeyCode::F(5) => Some(InputEvent::RefreshMailingLists),
            KeyCode::F(2) => Some(InputEvent::OpenEditConfig),
            KeyCode::F(1) => Some(InputEvent::OpenBookmarkedPatchsets),
            KeyCode::Backspace => Some(InputEvent::Backspace),
            KeyCode::Esc => Some(InputEvent::Quit),
            KeyCode::Char(ch) => Some(InputEvent::TextInput(ch)),
            KeyCode::Down => Some(InputEvent::NavigateDown),
            KeyCode::Up => Some(InputEvent::NavigateUp),
            _ => None,
        }
    }

    fn map_bookmarked_key(&mut self, key: &KeyInput) -> Option<InputEvent> {
        match key.code {
            KeyCode::Char('?') => Some(InputEvent::OpenHelp),
            KeyCode::Esc | KeyCode::Char('q') => Some(InputEvent::Back),
            KeyCode::Char('j') | KeyCode::Down => Some(InputEvent::NavigateDown),
            KeyCode::Char('k') | KeyCode::Up => Some(InputEvent::NavigateUp),
            KeyCode::Enter => Some(InputEvent::OpenPatchsetDetails),
            _ => None,
        }
    }

    fn map_latest_key(&mut self, key: &KeyInput) -> Option<InputEvent> {
        match key.code {
            KeyCode::Char('?') => Some(InputEvent::OpenHelp),
            KeyCode::Esc | KeyCode::Char('q') => Some(InputEvent::Back),
            KeyCode::Char('j') | KeyCode::Down => Some(InputEvent::NavigateDown),
            KeyCode::Char('k') | KeyCode::Up => Some(InputEvent::NavigateUp),
            KeyCode::Char('l') | KeyCode::Right => Some(InputEvent::NextPage),
            KeyCode::Char('h') | KeyCode::Left => Some(InputEvent::PreviousPage),
            KeyCode::Enter => Some(InputEvent::OpenPatchsetDetails),
            _ => None,
        }
    }

    fn map_edit_config_key(
        &mut self,
        key: &KeyInput,
        context: &InputContext,
    ) -> Option<InputEvent> {
        if context.edit_config_editing {
            return match key.code {
                KeyCode::Esc => Some(InputEvent::CancelConfigEdit),
                KeyCode::Backspace => Some(InputEvent::Backspace),
                KeyCode::Left => Some(InputEvent::NavigateLeft),
                KeyCode::Right => Some(InputEvent::NavigateRight),
                KeyCode::Char(ch) => Some(InputEvent::TextInput(ch)),
                KeyCode::Enter => Some(InputEvent::StageConfigEdit),
                _ => None,
            };
        }

        match key.code {
            KeyCode::Char('?') => Some(InputEvent::OpenHelp),
            KeyCode::Esc | KeyCode::Char('q') => Some(InputEvent::SaveConfig),
            KeyCode::Enter => Some(InputEvent::EditConfigField),
            KeyCode::Char('j') | KeyCode::Down => Some(InputEvent::NavigateDown),
            KeyCode::Char('k') | KeyCode::Up => Some(InputEvent::NavigateUp),
            _ => None,
        }
    }

    fn map_details_key(&mut self, key: &KeyInput) -> Option<InputEvent> {
        if key.modifiers.contains(KeyModifiers::SHIFT) {
            return match key.code {
                KeyCode::Char('G') => Some(InputEvent::PreviewGoToLastLine),
                KeyCode::Char('R') => Some(InputEvent::ToggleReplyWithReviewedByAll),
                _ => None,
            };
        }

        if key.modifiers.contains(KeyModifiers::CONTROL) {
            return match key.code {
                KeyCode::Char('b') => Some(InputEvent::PreviewScrollUp(ScrollAmount::Page)),
                KeyCode::Char('f') => Some(InputEvent::PreviewScrollDown(ScrollAmount::Page)),
                KeyCode::Char('u') => Some(InputEvent::PreviewScrollUp(ScrollAmount::HalfPage)),
                KeyCode::Char('d') => Some(InputEvent::PreviewScrollDown(ScrollAmount::HalfPage)),
                KeyCode::Char('t') => Some(InputEvent::ShowReviewTrailers),
                _ => None,
            };
        }

        match key.code {
            KeyCode::Char('?') => Some(InputEvent::OpenHelp),
            KeyCode::Esc | KeyCode::Char('q') => Some(InputEvent::Back),
            KeyCode::Char('a') => Some(InputEvent::ToggleApply),
            KeyCode::Char('j') | KeyCode::Down => {
                Some(InputEvent::PreviewScrollDown(ScrollAmount::Line))
            }
            KeyCode::Char('k') | KeyCode::Up => {
                Some(InputEvent::PreviewScrollUp(ScrollAmount::Line))
            }
            KeyCode::Char('h') | KeyCode::Left => Some(InputEvent::PreviewPanLeft),
            KeyCode::Char('l') | KeyCode::Right => Some(InputEvent::PreviewPanRight),
            KeyCode::Char('0') => Some(InputEvent::PreviewGoToBeginningOfLine),
            KeyCode::Char('g') => self.map_details_go_to_first_line_chord(),
            KeyCode::Char('f') => Some(InputEvent::TogglePreviewFullscreen),
            KeyCode::Char('n') => Some(InputEvent::PreviewNext),
            KeyCode::Char('p') => Some(InputEvent::PreviewPrevious),
            KeyCode::Char('b') => Some(InputEvent::ToggleBookmark),
            KeyCode::Char('r') => Some(InputEvent::ToggleReplyWithReviewedBy),
            KeyCode::Char('w') => Some(InputEvent::OpenKwOps),
            KeyCode::Enter => Some(InputEvent::ConsolidatePatchsetActions),
            _ => None,
        }
    }

    fn map_kw_ops_key(&mut self, key: &KeyInput, context: &InputContext) -> Option<InputEvent> {
        if context.kw_ops_editing {
            return match key.code {
                KeyCode::Esc => Some(InputEvent::CancelKwOpsEdit),
                KeyCode::Backspace => Some(InputEvent::Backspace),
                KeyCode::Enter => Some(InputEvent::StageKwOpsEdit),
                KeyCode::Char(ch) => Some(InputEvent::TextInput(ch)),
                _ => None,
            };
        }

        match key.code {
            KeyCode::Char('?') => Some(InputEvent::OpenHelp),
            KeyCode::Esc | KeyCode::Char('q') => Some(InputEvent::Back),
            KeyCode::Char('j') | KeyCode::Down => Some(InputEvent::NavigateDown),
            KeyCode::Char('k') | KeyCode::Up => Some(InputEvent::NavigateUp),
            KeyCode::Char('e') | KeyCode::Enter => Some(InputEvent::EditKwOpsField),
            KeyCode::Char('b') => Some(InputEvent::StartKwBuild),
            KeyCode::Char('d') => Some(InputEvent::StartKwDeploy),
            KeyCode::Char('D') => Some(InputEvent::StartKwBuildThenDeploy),
            KeyCode::Char('c') => Some(InputEvent::CancelKwJob),
            KeyCode::Char('r') => Some(InputEvent::RestoreKwBranch),
            _ => None,
        }
    }

    fn map_details_go_to_first_line_chord(&mut self) -> Option<InputEvent> {
        if let Some(PendingInput::DetailsGoToFirstLine { started_at }) = self.pending {
            if started_at.elapsed() <= self.bindings.chord_timeout {
                self.pending = None;
                return Some(InputEvent::PreviewGoToFirstLine);
            }
        }
        self.pending = Some(PendingInput::DetailsGoToFirstLine {
            started_at: Instant::now(),
        });
        None
    }

    fn clear_expired_pending_input(&mut self) {
        let Some(PendingInput::DetailsGoToFirstLine { started_at }) = self.pending else {
            return;
        };

        if started_at.elapsed() > self.bindings.chord_timeout {
            self.pending = None;
        }
    }
}

impl Default for InputMapper {
    fn default() -> Self {
        Self::new(KeyBindings::default())
    }
}

#[cfg(test)]
mod tests {

    mod helpers {
        use crate::{
            app::screens::CurrentScreen,
            input::{
                context::InputContext,
                event::{KeyInput, TerminalEvent},
            },
        };
        use ratatui::crossterm::event::{KeyCode, KeyModifiers};

        pub(super) fn context(current_screen: CurrentScreen) -> InputContext {
            InputContext {
                current_screen,
                ..Default::default()
            }
        }

        pub(super) fn key(code: KeyCode) -> TerminalEvent {
            TerminalEvent::Key(KeyInput {
                code,
                ..Default::default()
            })
        }

        pub(super) fn modified_key(code: KeyCode, modifiers: KeyModifiers) -> TerminalEvent {
            TerminalEvent::Key(KeyInput {
                code,
                modifiers,
                ..Default::default()
            })
        }
    }
    use helpers::*;
    use ratatui::crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};

    use crate::{
        app::screens::CurrentScreen,
        input::{
            context::InputContext,
            event::{InputEvent, KeyInput, ScrollAmount, TerminalEvent},
            mapper::InputMapper,
        },
    };

    #[test]
    fn maps_escape_to_close_popup_when_popup_is_open() {
        let mut mapper = InputMapper::default();
        let context = InputContext {
            current_screen: CurrentScreen::PatchsetDetails,
            popup_open: true,
            ..Default::default()
        };

        let event = mapper.map_terminal_event(key(KeyCode::Esc), &context);

        assert_eq!(event, Some(InputEvent::ClosePopup));
    }

    #[test]
    fn maps_enter_to_confirm_popup_only_for_confirm_popups() {
        let mut mapper = InputMapper::default();
        let confirm = InputContext {
            current_screen: CurrentScreen::MailingListSelection,
            popup_open: true,
            confirm_popup_open: true,
            ..Default::default()
        };
        assert_eq!(
            mapper.map_terminal_event(key(KeyCode::Enter), &confirm),
            Some(InputEvent::ConfirmPopup)
        );

        let info = InputContext {
            current_screen: CurrentScreen::MailingListSelection,
            popup_open: true,
            ..Default::default()
        };
        assert_eq!(mapper.map_terminal_event(key(KeyCode::Enter), &info), None);
    }

    #[test]
    fn maps_escape_to_back_on_details_without_popup() {
        let mut mapper = InputMapper::default();
        let context = context(CurrentScreen::PatchsetDetails);

        let event = mapper.map_terminal_event(key(KeyCode::Esc), &context);

        assert_eq!(event, Some(InputEvent::Back));
    }

    #[test]
    fn maps_mailing_list_char_to_text_input() {
        let mut mapper = InputMapper::default();
        let context = context(CurrentScreen::MailingListSelection);

        let event = mapper.map_terminal_event(key(KeyCode::Char('n')), &context);

        assert_eq!(event, Some(InputEvent::TextInput('n')));
    }

    #[test]
    fn maps_mailing_list_function_keys_to_semantic_events() {
        let mut mapper = InputMapper::default();
        let context = context(CurrentScreen::MailingListSelection);

        assert_eq!(
            mapper.map_terminal_event(key(KeyCode::F(1)), &context),
            Some(InputEvent::OpenBookmarkedPatchsets)
        );
        assert_eq!(
            mapper.map_terminal_event(key(KeyCode::F(2)), &context),
            Some(InputEvent::OpenEditConfig)
        );
        assert_eq!(
            mapper.map_terminal_event(key(KeyCode::F(5)), &context),
            Some(InputEvent::RefreshMailingLists)
        );
    }

    #[test]
    fn maps_enter_in_edit_mode_to_stage_config_edit() {
        let mut mapper = InputMapper::default();
        let context = InputContext {
            current_screen: CurrentScreen::EditConfig,
            edit_config_editing: true,
            ..Default::default()
        };

        let event = mapper.map_terminal_event(key(KeyCode::Enter), &context);

        assert_eq!(event, Some(InputEvent::StageConfigEdit));
    }

    #[test]
    fn maps_enter_outside_edit_mode_to_edit_config_field() {
        let mut mapper = InputMapper::default();
        let context = context(CurrentScreen::EditConfig);

        let event = mapper.map_terminal_event(key(KeyCode::Enter), &context);

        assert_eq!(event, Some(InputEvent::EditConfigField));
    }

    #[test]
    fn maps_arrows_to_navigate_while_editing_config_and_keeps_hl_as_text() {
        let mut mapper = InputMapper::default();
        let editing = InputContext {
            current_screen: CurrentScreen::EditConfig,
            edit_config_editing: true,
            ..Default::default()
        };

        assert_eq!(
            mapper.map_terminal_event(key(KeyCode::Left), &editing),
            Some(InputEvent::NavigateLeft)
        );
        assert_eq!(
            mapper.map_terminal_event(key(KeyCode::Right), &editing),
            Some(InputEvent::NavigateRight)
        );
        assert_eq!(
            mapper.map_terminal_event(key(KeyCode::Char('h')), &editing),
            Some(InputEvent::TextInput('h'))
        );
        assert_eq!(
            mapper.map_terminal_event(key(KeyCode::Char('l')), &editing),
            Some(InputEvent::TextInput('l'))
        );

        let browsing = context(CurrentScreen::EditConfig);
        assert_eq!(
            mapper.map_terminal_event(key(KeyCode::Left), &browsing),
            None
        );
        assert_eq!(
            mapper.map_terminal_event(key(KeyCode::Right), &browsing),
            None
        );
    }

    #[test]
    fn ignores_key_release_events() {
        let mut mapper = InputMapper::default();
        let context = context(CurrentScreen::MailingListSelection);
        let event = TerminalEvent::Key(KeyInput {
            code: KeyCode::Esc,
            kind: KeyEventKind::Release,
            ..Default::default()
        });

        let mapped = mapper.map_terminal_event(event, &context);

        assert_eq!(mapped, None);
    }

    #[test]
    fn maps_details_modifier_keys() {
        let mut mapper = InputMapper::default();
        let context = context(CurrentScreen::PatchsetDetails);

        assert_eq!(
            mapper.map_terminal_event(
                modified_key(KeyCode::Char('G'), KeyModifiers::SHIFT),
                &context
            ),
            Some(InputEvent::PreviewGoToLastLine)
        );
        assert_eq!(
            mapper.map_terminal_event(
                modified_key(KeyCode::Char('t'), KeyModifiers::CONTROL),
                &context
            ),
            Some(InputEvent::ShowReviewTrailers)
        );
        assert_eq!(
            mapper.map_terminal_event(
                modified_key(KeyCode::Char('d'), KeyModifiers::CONTROL),
                &context
            ),
            Some(InputEvent::PreviewScrollDown(ScrollAmount::HalfPage))
        );
    }

    #[test]
    fn maps_details_gg_sequence_to_go_to_first_line() {
        let mut mapper = InputMapper::default();
        let context = context(CurrentScreen::PatchsetDetails);

        assert_eq!(
            mapper.map_terminal_event(key(KeyCode::Char('g')), &context),
            None
        );
        assert_eq!(
            mapper.map_terminal_event(key(KeyCode::Char('g')), &context),
            Some(InputEvent::PreviewGoToFirstLine)
        );
    }

    #[test]
    fn maps_w_on_details_to_open_kw_ops() {
        let mut mapper = InputMapper::default();
        let context = context(CurrentScreen::PatchsetDetails);

        assert_eq!(
            mapper.map_terminal_event(key(KeyCode::Char('w')), &context),
            Some(InputEvent::OpenKwOps)
        );
    }

    #[test]
    fn maps_kw_ops_keys_and_treats_letters_as_text_while_editing() {
        let mut mapper = InputMapper::default();
        let context = context(CurrentScreen::KwOps);

        assert_eq!(
            mapper.map_terminal_event(key(KeyCode::Char('b')), &context),
            Some(InputEvent::StartKwBuild)
        );
        assert_eq!(
            mapper.map_terminal_event(key(KeyCode::Char('d')), &context),
            Some(InputEvent::StartKwDeploy)
        );
        assert_eq!(
            mapper.map_terminal_event(key(KeyCode::Char('D')), &context),
            Some(InputEvent::StartKwBuildThenDeploy)
        );
        assert_eq!(
            mapper.map_terminal_event(key(KeyCode::Char('c')), &context),
            Some(InputEvent::CancelKwJob)
        );
        assert_eq!(
            mapper.map_terminal_event(key(KeyCode::Char('r')), &context),
            Some(InputEvent::RestoreKwBranch)
        );
        assert_eq!(
            mapper.map_terminal_event(key(KeyCode::Char('e')), &context),
            Some(InputEvent::EditKwOpsField)
        );

        let editing = InputContext {
            kw_ops_editing: true,
            ..context
        };
        assert_eq!(
            mapper.map_terminal_event(key(KeyCode::Char('b')), &editing),
            Some(InputEvent::TextInput('b'))
        );
        assert_eq!(
            mapper.map_terminal_event(key(KeyCode::Esc), &editing),
            Some(InputEvent::CancelKwOpsEdit)
        );
    }

    #[test]
    fn maps_resize_and_tick_events() {
        let mut mapper = InputMapper::default();
        let context = context(CurrentScreen::MailingListSelection);

        assert_eq!(
            mapper.map_terminal_event(
                TerminalEvent::Resize {
                    width: 120,
                    height: 40,
                },
                &context
            ),
            Some(InputEvent::Resize {
                width: 120,
                height: 40,
            })
        );
    }
}
