use tokio::sync::{mpsc, oneshot};

use crate::{
    app::view_model::AppViewModel,
    ui::{
        errors::UiError,
        messages::{UiMessage, UiResult},
        scene::UiScene,
    },
};

#[derive(Clone)]
pub struct UiHandle {
    tx: mpsc::Sender<UiMessage>,
}

impl UiHandle {
    pub fn new(tx: mpsc::Sender<UiMessage>) -> Self {
        Self { tx }
    }

    pub async fn build_scene(&self, app_view: AppViewModel) -> UiResult<UiScene> {
        self.request_result(|reply_to| UiMessage::BuildScene {
            app_view: Box::new(app_view),
            reply_to,
        })
        .await
    }

    pub async fn shutdown(&self) {
        self.tx.send(UiMessage::Shutdown).await.ok();
    }

    async fn request_result<T>(
        &self,
        build_message: impl FnOnce(oneshot::Sender<UiResult<T>>) -> UiMessage,
    ) -> UiResult<T>
    where
        T: Send + 'static,
    {
        let (reply_to, rx) = oneshot::channel();
        self.tx
            .send(build_message(reply_to))
            .await
            .map_err(|_| UiError::Build("request channel closed".to_string()))?;
        rx.await
            .map_err(|_| UiError::Build("reply channel closed".to_string()))?
    }
}
