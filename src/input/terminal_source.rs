use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};

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

pub fn wait_for_enter_press(event_source: &mut dyn TerminalEventSource) -> color_eyre::Result<()> {
    loop {
        if let Some(TerminalEvent::Key(key)) = event_source.read_event()? {
            if key.code == KeyCode::Enter {
                return Ok(());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use ratatui::crossterm::event::{
        Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers,
    };

    use crate::input::{
        event::{KeyInput, TerminalEvent},
        terminal_source::{
            terminal_event_from_crossterm_event, wait_for_enter_press, TerminalEventSource,
        },
    };

    struct FakeTerminalEventSource {
        events: Vec<Option<TerminalEvent>>,
    }

    impl FakeTerminalEventSource {
        fn new(events: Vec<Option<TerminalEvent>>) -> Self {
            Self { events }
        }
    }

    impl TerminalEventSource for FakeTerminalEventSource {
        fn read_event(&mut self) -> color_eyre::Result<Option<TerminalEvent>> {
            Ok(self.events.remove(0))
        }
    }

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

    #[test]
    fn wait_for_enter_press_ignores_non_enter_events() {
        let mut event_source = FakeTerminalEventSource::new(vec![
            None,
            Some(TerminalEvent::Resize {
                width: 120,
                height: 40,
            }),
            Some(TerminalEvent::Key(KeyInput::press(KeyCode::Char('x')))),
            Some(TerminalEvent::Key(KeyInput::press(KeyCode::Enter))),
        ]);

        let result = wait_for_enter_press(&mut event_source);

        assert!(result.is_ok());
    }
}
