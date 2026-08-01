use tokio::{
    sync::{mpsc, oneshot},
    task::JoinHandle,
};

use crate::{
    app::{errors::AppError, messages::AppMessage, view_model::AppViewModel},
    input::context::InputContext,
};

#[allow(dead_code)]
/// Handle to the `AppActor` task.
///
/// Exposes typed async methods for controlling the actor and querying its
/// state. `run_until_done` consumes the handle and awaits the task's exit.
pub struct AppHandle {
    join: JoinHandle<color_eyre::Result<()>>,
    tx: mpsc::Sender<AppMessage>,
}

#[allow(dead_code)]
impl AppHandle {
    pub fn new(join: JoinHandle<color_eyre::Result<()>>, tx: mpsc::Sender<AppMessage>) -> Self {
        Self { join, tx }
    }

    /// Requests the actor to run startup validation and returns the result.
    pub async fn initialize(&self) -> Result<(), AppError> {
        self.request(|reply_to| AppMessage::Initialize { reply_to })
            .await
            .unwrap_or(Err(AppError::Input("actor channel closed".to_string())))
    }

    /// Requests the actor to stop its run loop and waits for acknowledgement.
    pub async fn shutdown(&self) {
        self.request(|reply_to| AppMessage::Shutdown { reply_to })
            .await
            .ok();
    }

    /// Returns a snapshot of the current presentation model.
    pub async fn get_view_model(&self) -> Option<AppViewModel> {
        self.request(|reply_to| AppMessage::GetViewModel { reply_to })
            .await
            .ok()
    }

    /// Returns the current input context for the input mapper.
    pub async fn get_input_context(&self) -> Option<InputContext> {
        self.request(|reply_to| AppMessage::GetInputContext { reply_to })
            .await
            .ok()
    }

    /// Blocks the caller until the actor task completes, propagating any error
    /// returned by the actor's run loop.
    pub async fn run_until_done(self) -> color_eyre::Result<()> {
        self.join.await?
    }

    async fn request<T>(
        &self,
        build_message: impl FnOnce(oneshot::Sender<T>) -> AppMessage,
    ) -> Result<T, oneshot::error::RecvError>
    where
        T: Send + 'static,
    {
        let (reply_to, rx) = oneshot::channel();
        self.tx.send(build_message(reply_to)).await.ok();
        rx.await
    }
}
