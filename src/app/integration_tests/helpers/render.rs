use crate::render::{RenderedPatchPreview, RenderedPatchsetPreview};

pub(crate) fn sample_rendered_preview() -> RenderedPatchsetPreview {
    RenderedPatchsetPreview::new(vec![RenderedPatchPreview::new(
        "Subject: [PATCH] test\n---\n+added line".to_string(),
    )])
}
