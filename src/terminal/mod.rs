//! Actor boundary for the Ratatui/Crossterm terminal session.
//!
//! This module is the single owner of terminal session operations. The runtime
//! draws frames through [`handle::TerminalHandle::draw`]. Raw terminal events
//! are delivered to the [`crate::input`] actor via
//! [`handle::TerminalHandle::poll_event`]; the Input actor translates them into
//! semantic [`crate::input::event::InputEvent`] values before forwarding them
//! to the App.

pub mod actor;
pub mod errors;
pub mod handle;
pub mod messages;
pub mod session;

pub use errors::TerminalError;
