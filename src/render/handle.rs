#![allow(dead_code)] // Some protocol methods are reserved for future App flows.

use tokio::sync::{mpsc, oneshot};

use crate::{
    render::{
        messages::{RenderMessage, RenderResult},
        RenderError, RenderPatchsetRequest, RenderedPatchPreview, RenderedPatchsetPreview,
    },
    render_prefs::{CoverRenderer, PatchRenderer},
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

    pub async fn render_single_patch(
        &self,
        raw_patch: String,
        patch_renderer: PatchRenderer,
        cover_renderer: CoverRenderer,
    ) -> RenderResult<RenderedPatchPreview> {
        self.request_result(|reply| RenderMessage::RenderSinglePatch {
            raw_patch,
            patch_renderer,
            cover_renderer,
            reply,
        })
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
