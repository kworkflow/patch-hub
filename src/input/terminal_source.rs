use ratatui::crossterm::event::{self, Event, KeyEventKind};

use crate::input::event::{KeyInput, TerminalEvent};

pub trait TerminalEventSource {
    fn read_event(&mut self) -> color_eyre::Result<Option<TerminalEvent>>;
}

pub struct CrosstermEventSource;

impl TerminalEventSource for CrosstermEventSource {
    fn read_event(&mut self) -> color_eyre::Result<Option<TerminalEvent>> {
        Ok(terminal_event_from_crossterm_event(event::read()?))
    }
}

pub fn terminal_event_from_crossterm_event(event: Event) -> Option<TerminalEvent> {
    match event {
        Event::Key(key) if key.kind == KeyEventKind::Release => None,
        Event::Key(key) => Some(TerminalEvent::Key(KeyInput::from(key))),
        Event::Resize(width, height) => Some(TerminalEvent::Resize { width, height }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use ratatui::crossterm::event::{
        Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers,
    };

    use crate::input::{
        event::{KeyInput, TerminalEvent},
        terminal_source::terminal_event_from_crossterm_event,
    };

    #[test]
    fn converts_key_press_to_terminal_key_event() {
        let key = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE);

        let event = terminal_event_from_crossterm_event(Event::Key(key));

        assert_eq!(event, Some(TerminalEvent::Key(KeyInput::from(key))));
    }

    #[test]
    fn ignores_key_release_events() {
        let key = KeyEvent {
            code: KeyCode::Char('j'),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Release,
            state: KeyEventState::NONE,
        };

        let event = terminal_event_from_crossterm_event(Event::Key(key));

        assert_eq!(event, None);
    }

    #[test]
    fn converts_resize_events() {
        let event = terminal_event_from_crossterm_event(Event::Resize(120, 40));

        assert_eq!(
            event,
            Some(TerminalEvent::Resize {
                width: 120,
                height: 40
            })
        );
    }

    #[test]
    fn ignores_non_key_non_resize_events() {
        let event = terminal_event_from_crossterm_event(Event::FocusGained);

        assert_eq!(event, None);
    }
}
