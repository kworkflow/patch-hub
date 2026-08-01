//! Protocol boundary between terminal input and application intent.
//!
//! Phase 9 uses a pull loop: `handler::run_app` reads raw
//! [`event::TerminalEvent`] values through the terminal actor, then
//! [`mapper::InputMapper`] translates them using [`context::InputContext`]
//! before handlers consume semantic [`event::InputEvent`] values. Phase 10
//! will introduce an `InputActor` that broadcasts terminal events instead of
//! this direct pull loop.

pub mod bindings;
pub mod context;
pub mod event;
pub mod mapper;
