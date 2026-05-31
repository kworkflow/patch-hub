#![allow(dead_code)] // Follow-up phases may wire additional render message variants.

use tokio::sync::oneshot;

use crate::{
    render::{RenderError, RenderPatchsetRequest, RenderedPatchPreview, RenderedPatchsetPreview},
    render_prefs::{CoverRenderer, PatchRenderer},
};

pub type RenderResult<T> = Result<T, RenderError>;

pub enum RenderMessage {
    RenderPatchsetPreview {
        request: RenderPatchsetRequest,
        reply: oneshot::Sender<RenderResult<RenderedPatchsetPreview>>,
    },
    RenderSinglePatch {
        raw_patch: String,
        patch_renderer: PatchRenderer,
        cover_renderer: CoverRenderer,
        reply: oneshot::Sender<RenderResult<RenderedPatchPreview>>,
    },
}

impl RenderMessage {
    pub fn name(&self) -> &'static str {
        match self {
            RenderMessage::RenderPatchsetPreview { .. } => "RenderPatchsetPreview",
            RenderMessage::RenderSinglePatch { .. } => "RenderSinglePatch",
        }
    }
}
