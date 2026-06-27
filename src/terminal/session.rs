use std::time::Duration;

use mockall::automock;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};

use crate::{
    infrastructure::terminal::{
        restore, setup_user_io as setup_terminal_user_io,
        teardown_user_io as teardown_terminal_user_io, Tui,
    },
    input::event::{KeyInput, TerminalEvent},
    terminal::{
        messages::{TerminalFrame, TerminalResult},
        TerminalError,
    },
    ui::{loading_screen::draw_loading_screen, painter},
};

/// Stateful terminal session owned by the terminal actor.
#[automock]
pub trait TerminalSessionApi: Send {
    fn draw(&mut self, frame: TerminalFrame) -> TerminalResult<()>;
    fn read_event(&mut self) -> TerminalResult<Option<TerminalEvent>>;
    fn poll_event(&mut self, timeout: Duration) -> TerminalResult<Option<TerminalEvent>>;
    fn setup_user_io(&mut self) -> TerminalResult<()>;
    fn teardown_user_io(&mut self) -> TerminalResult<()>;
    fn wait_for_key_press(&mut self, key: KeyCode, timeout: Duration) -> TerminalResult<bool>;
    fn size(&self) -> TerminalResult<(u16, u16)>;
    fn shutdown(&mut self) -> TerminalResult<()>;
}

/// Ratatui/Crossterm-backed terminal session.
pub struct CrosstermTerminalSession {
    terminal: Tui,
    shutdown: bool,
}

impl CrosstermTerminalSession {
    pub fn new(terminal: Tui) -> Self {
        Self {
            terminal,
            shutdown: false,
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
}

impl TerminalSessionApi for CrosstermTerminalSession {
    fn draw(&mut self, frame: TerminalFrame) -> TerminalResult<()> {
        match frame {
            TerminalFrame::Main(scene) => {
                self.terminal.draw(|f| painter::paint(f, &scene))?;
            }
            TerminalFrame::Loading(title) => {
                self.terminal.draw(|f| draw_loading_screen(f, &title))?;
            }
            TerminalFrame::Empty => {
                self.terminal.draw(|_| {})?;
            }
        }
        Ok(())
    }

    fn read_event(&mut self) -> TerminalResult<Option<TerminalEvent>> {
        Ok(Self::terminal_event_from_crossterm_event(event::read()?))
    }

    fn poll_event(&mut self, timeout: Duration) -> TerminalResult<Option<TerminalEvent>> {
        if !event::poll(timeout)? {
            return Ok(None);
        }

        self.read_event()
    }

    fn setup_user_io(&mut self) -> TerminalResult<()> {
        setup_terminal_user_io(&mut self.terminal)
            .map_err(|err| TerminalError::Session(err.to_string()))
    }

    fn teardown_user_io(&mut self) -> TerminalResult<()> {
        teardown_terminal_user_io(&mut self.terminal)
            .map_err(|err| TerminalError::Session(err.to_string()))
    }

    fn wait_for_key_press(&mut self, key: KeyCode, timeout: Duration) -> TerminalResult<bool> {
        wait_for_key_press_from_session(self, key, timeout)
    }

    fn size(&self) -> TerminalResult<(u16, u16)> {
        let size = self.terminal.size()?;
        Ok((size.width, size.height))
    }

    fn shutdown(&mut self) -> TerminalResult<()> {
        if self.shutdown {
            return Ok(());
        }

        restore()?;
        self.shutdown = true;
        Ok(())
    }
}

fn wait_for_key_press_from_session(
    session: &mut dyn TerminalSessionApi,
    key: KeyCode,
    timeout: Duration,
) -> TerminalResult<bool> {
    let started_at = std::time::Instant::now();

    while started_at.elapsed() < timeout {
        let elapsed = started_at.elapsed();
        let remaining = timeout.saturating_sub(elapsed);
        let poll_timeout = remaining.min(Duration::from_millis(16));

        if let Some(TerminalEvent::Key(input)) = session.poll_event(poll_timeout)? {
            if input.code == key {
                return Ok(true);
            }
        }
    }

    Ok(false)
}

#[cfg(test)]
mod tests {
    use ratatui::crossterm::event::{
        Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers,
    };

    use crate::input::event::{KeyInput, TerminalEvent};

    use super::CrosstermTerminalSession;

    #[test]
    fn converts_key_press_to_terminal_key_event() {
        let key = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE);

        let event = CrosstermTerminalSession::terminal_event_from_crossterm_event(Event::Key(key));

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

        let event = CrosstermTerminalSession::terminal_event_from_crossterm_event(Event::Key(key));

        assert_eq!(event, None);
    }

    #[test]
    fn converts_resize_events() {
        let event =
            CrosstermTerminalSession::terminal_event_from_crossterm_event(Event::Resize(120, 40));

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
        let event =
            CrosstermTerminalSession::terminal_event_from_crossterm_event(Event::FocusGained);

        assert_eq!(event, None);
    }
}
