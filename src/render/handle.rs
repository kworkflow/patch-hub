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

    /// Signals the actor to stop processing messages and exit its run loop.
    ///
    /// Callers should invoke this after the last request that uses this handle has
    /// completed. Dropping all clones of the handle also stops the actor, but
    /// calling `shutdown` makes the intent explicit and allows ordered teardown.
    pub async fn shutdown(&self) {
        self.tx.send(RenderMessage::Shutdown).await.ok();
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
