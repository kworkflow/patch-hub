//! Actor boundary for the Ratatui/Crossterm terminal session.
//!
//! This module owns session operations. Frames are drawn through
//! `TerminalHandle::draw`. Raw events go to the input actor via
//! `poll_event`, which turns them into `InputEvent` values.

pub mod actor;
pub mod errors;
pub mod handle;
pub mod messages;
pub mod session;

pub use errors::TerminalError;
