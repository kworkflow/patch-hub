use tokio::sync::{mpsc, oneshot};

use crate::render::{
    messages::{RenderMessage, RenderResult},
    RenderError, RenderPatchsetRequest, RenderedPatchsetPreview,
};

#[derive(Clone)]
pub struct RenderHandle {
    tx: mpsc::Sender<RenderMessage>,
}

impl RenderHandle {
    pub fn new(tx: mpsc::Sender<RenderMessage>) -> Self {
        Self { tx }
    }

    pub async fn render_patchset_preview(
        &self,
        request: RenderPatchsetRequest,
    ) -> RenderResult<RenderedPatchsetPreview> {
        self.request_result(|reply| RenderMessage::RenderPatchsetPreview { request, reply })
            .await
    }

    async fn request_result<T>(
        &self,
        build_message: impl FnOnce(oneshot::Sender<RenderResult<T>>) -> RenderMessage,
    ) -> RenderResult<T>
    where
        T: Send + 'static,
    {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(build_message(reply))
            .await
            .map_err(|_| RenderError::ActorUnavailable("request channel closed".to_string()))?;
        rx.await
            .map_err(|_| RenderError::ActorUnavailable("reply channel closed".to_string()))?
    }
}
