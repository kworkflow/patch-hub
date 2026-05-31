//! Protocol boundary between terminal input and application intent.
//!
//! Terminal backends produce [`event::TerminalEvent`] values through the
//! terminal actor, [`mapper::InputMapper`] translates them using
//! [`context::InputContext`], and handlers consume semantic
//! [`event::InputEvent`] values. The Input actor and broadcast handle are
//! introduced in the next phase.

pub mod bindings;
pub mod context;
pub mod event;
pub mod mapper;
