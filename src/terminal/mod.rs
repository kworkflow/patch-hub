//! Actor boundary for the Ratatui/Crossterm terminal session.
//!
//! Phase 9 makes this module the single owner of terminal session operations.
//! The runtime pull loop in `handler::run_app` draws through
//! [`handle::TerminalHandle::draw`] and reads input through
//! [`handle::TerminalHandle::read_event`]. Phase 10 will introduce an
//! `InputActor` that broadcasts terminal events instead of the current
//! direct pull loop.

pub mod actor;
pub mod errors;
pub mod handle;
pub mod messages;
pub mod session;

pub use errors::TerminalError;
