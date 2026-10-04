//! Protocol boundary between terminal input and application intent.
//!
//! The Input actor turns `TerminalEvent` values into `InputEvent` values
//! via `InputMapper` and the App's `InputContext`. `InputHandle` is the
//! cloneable interface.

pub mod actor;
pub mod bindings;
pub mod context;
pub mod errors;
pub mod event;
pub mod handle;
pub mod mapper;
pub mod messages;
