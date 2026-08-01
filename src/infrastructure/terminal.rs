//! Low-level Crossterm/Ratatui helpers for the terminal actor session and
//! emergency restore hooks.
//!
//! Normal runtime startup uses [`init`] from `main`, session operations go
//! through [`crate::terminal::session::CrosstermTerminalSession`], and fatal
//! error hooks call [`restore`] directly.

use ratatui::{
    crossterm::{
        execute,
        terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    },
    layout::Position,
    prelude::{Backend, CrosstermBackend},
    Terminal,
};

use std::io::{self, stdout, Stdout};

/// A type alias for the terminal type used in this application
pub type Tui = Terminal<CrosstermBackend<Stdout>>;

/// Initialize the terminal
pub fn init() -> io::Result<Tui> {
    execute!(stdout(), EnterAlternateScreen)?;
    enable_raw_mode()?;
    Terminal::new(CrosstermBackend::new(stdout()))
}

/// Restore the terminal to its original state
pub fn restore() -> io::Result<()> {
    execute!(stdout(), LeaveAlternateScreen)?;
    disable_raw_mode()?;
    Ok(())
}

pub(crate) fn setup_user_io<B: Backend>(terminal: &mut Terminal<B>) -> color_eyre::Result<()> {
    terminal.clear()?;
    terminal.set_cursor_position(Position::new(0, 0))?;
    terminal.show_cursor()?;
    disable_raw_mode()?;
    Ok(())
}

pub(crate) fn teardown_user_io<B: Backend>(terminal: &mut Terminal<B>) -> color_eyre::Result<()> {
    enable_raw_mode()?;
    terminal.clear()?;
    Ok(())
}
