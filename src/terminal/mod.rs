//! Actor boundary for the Ratatui/Crossterm terminal session.
//!
//! Phase 9 introduces this module as the only owner of terminal session
//! operations. Later commits wire the existing app loop through
//! [`handle::TerminalHandle`].

pub mod actor;
pub mod errors;
pub mod handle;
pub mod messages;
pub mod session;

pub use errors::TerminalError;
