//! Protocol boundary between terminal input and application intent.
//!
//! Phase 8 keeps this module actor-free: terminal backends produce
//! [`event::TerminalEvent`] values, [`mapper::InputMapper`] translates them
//! using [`context::InputContext`], and handlers consume semantic
//! [`event::InputEvent`] values. The Input actor and broadcast handle are
//! introduced later once Terminal is also actorized.

pub mod bindings;
pub mod context;
pub mod event;
pub mod mapper;
pub mod terminal_source;
