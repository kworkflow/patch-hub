use crate::render_prefs::{CoverRenderer, PatchRenderer};

/// Input needed to render all previews for a patchset.
pub struct RenderPatchsetRequest {
    pub raw_patches: Vec<String>,
    pub patch_renderer: PatchRenderer,
    pub cover_renderer: CoverRenderer,
}

impl RenderPatchsetRequest {
    pub fn new(
        raw_patches: Vec<String>,
        patch_renderer: PatchRenderer,
        cover_renderer: CoverRenderer,
    ) -> Self {
        Self {
            raw_patches,
            patch_renderer,
            cover_renderer,
        }
    }
}

/// Rendered preview payload for a full patchset.
pub struct RenderedPatchsetPreview {
    pub entries: Vec<RenderedPatchPreview>,
}

impl RenderedPatchsetPreview {
    pub fn new(entries: Vec<RenderedPatchPreview>) -> Self {
        Self { entries }
    }
}

/// Rendered preview payload for a single patch or cover letter entry.
pub struct RenderedPatchPreview {
    pub rendered_text: String,
}

impl RenderedPatchPreview {
    pub fn new(rendered_text: String) -> Self {
        Self { rendered_text }
    }
}
