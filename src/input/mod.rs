//! Protocol boundary between terminal input and application intent.
//!
//! The Input actor mediates between the Terminal actor (producer of raw
//! [`event::TerminalEvent`] values) and the App (consumer of semantic
//! [`event::InputEvent`] values). [`mapper::InputMapper`] translates events
//! using the current [`context::InputContext`], which the App updates after
//! every state mutation. [`handle::InputHandle`] is the cloneable public
//! interface to the actor.

pub mod bindings;
pub mod context;
pub mod errors;
pub mod event;
pub mod handle;
pub mod mapper;
pub mod messages;
