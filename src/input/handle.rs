use tokio::sync::mpsc;

use crate::input::{
    context::InputContext, errors::InputError, event::InputEvent, messages::InputMessage,
};

/// Cloneable handle to the Input actor.
///
/// All methods are fire-and-forget sends; the actor processes them
/// asynchronously in its own task.
#[derive(Clone)]
pub struct InputHandle {
    tx: mpsc::Sender<InputMessage>,
}

impl InputHandle {
    pub fn new(tx: mpsc::Sender<InputMessage>) -> Self {
        Self { tx }
    }

    /// Registers `tx` as the destination for translated [`InputEvent`] values.
    ///
    /// The actor delivers one `InputEvent` per raw terminal event that maps to
    /// a semantic action under the current [`InputContext`].
    pub async fn subscribe_app(&self, tx: mpsc::Sender<InputEvent>) -> Result<(), InputError> {
        self.send(InputMessage::SubscribeApp { tx }).await
    }

    /// Replaces the current mapping context with `context`.
    ///
    /// Should be called by the App after every state mutation that changes
    /// screen, popup visibility, or edit mode.
    pub async fn update_context(&self, context: InputContext) -> Result<(), InputError> {
        self.send(InputMessage::UpdateContext { context }).await
    }

    /// Requests the actor to stop its event loop.
    ///
    /// Dropping all clones of the handle achieves the same effect because the
    /// actor's receive channel closes when its last sender is gone.
    #[allow(dead_code)]
    pub async fn shutdown(&self) -> Result<(), InputError> {
        self.send(InputMessage::Shutdown).await
    }

    async fn send(&self, message: InputMessage) -> Result<(), InputError> {
        self.tx.send(message).await.map_err(|_| InputError::Closed)
    }
}
