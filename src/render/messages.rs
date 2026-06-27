use tokio::sync::oneshot;

use crate::render::{RenderError, RenderPatchsetRequest, RenderedPatchsetPreview};

pub type RenderResult<T> = Result<T, RenderError>;

pub enum RenderMessage {
    RenderPatchsetPreview {
        request: RenderPatchsetRequest,
        reply: oneshot::Sender<RenderResult<RenderedPatchsetPreview>>,
    },
}

impl RenderMessage {
    pub fn name(&self) -> &'static str {
        match self {
            RenderMessage::RenderPatchsetPreview { .. } => "RenderPatchsetPreview",
        }
    }
}
