use std::time::Duration;

use mockall::automock;
use ratatui::crossterm::event::KeyCode;

use crate::{
    input::event::TerminalEvent,
    terminal::messages::{TerminalFrame, TerminalResult},
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
