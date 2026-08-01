use mockall::automock;
use thiserror::Error;

use super::dto::{RenderPatchsetRequest, RenderedPatchsetPreview};

/// Failure while running an external preview renderer (bat, delta, etc.).
///
/// Reserved for future strict error propagation; [`super::ShellRenderService`]
/// currently falls back to raw text and returns `Ok`.
#[allow(dead_code)]
#[derive(Debug, Error)]
pub enum RenderError {
    #[error("render failed: {0}")]
    Failed(String),
    #[error("render actor unavailable: {0}")]
    ActorUnavailable(String),
}

/// Abstraction for rich-text patch/cover preview (shell-backed renderers).
///
/// Implemented by [`super::ShellRenderService`]; mockable for tests.
#[automock]
pub trait RenderServiceApi: Send + Sync {
    /// Renders each raw patch string to the same format used by the patchset
    /// details screen: expanded tabs, split cover/diff, external renderers, then
    /// `"{cover}---\\n{patch}"` per entry.
    fn render_patchset_preview(
        &self,
        request: RenderPatchsetRequest,
    ) -> Result<RenderedPatchsetPreview, RenderError>;
}
