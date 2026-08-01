use tokio::sync::mpsc;

use crate::input::{context::InputContext, event::InputEvent};

/// Messages understood by the Input actor.
pub enum InputMessage {
    /// Registers the App as the single consumer of [`InputEvent`] values.
    SubscribeApp { tx: mpsc::Sender<InputEvent> },
    /// Updates the mapping context so the actor can translate the next
    /// terminal event with current application state.
    UpdateContext { context: InputContext },
    /// Requests a clean shutdown of the actor's event loop.
    Shutdown,
}
