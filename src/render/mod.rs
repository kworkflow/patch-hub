//! Rich patch/cover preview via external programs (`bat`, `delta`, `diff-so-fancy`).

pub mod dto;
mod r#trait;

pub use dto::{RenderPatchsetRequest, RenderedPatchPreview, RenderedPatchsetPreview};
pub use r#trait::{RenderError, RenderServiceApi};

use std::sync::Arc;

use tracing::{event, Level};

use crate::{
    app::cover_renderer::render_cover, app::patch_renderer::render_patch_preview,
    infrastructure::shell::ShellTrait, lore::infrastructure::patchset_parser::split_cover,
};

/// [`RenderServiceApi`] backed by [`ShellTrait`] and the existing `app` renderer helpers.
pub struct ShellRenderService {
    shell: Arc<dyn ShellTrait>,
}

impl ShellRenderService {
    pub fn new(shell: Arc<dyn ShellTrait>) -> Self {
        Self { shell }
    }
}

impl RenderServiceApi for ShellRenderService {
    fn render_patchset_preview(
        &self,
        request: RenderPatchsetRequest,
    ) -> Result<RenderedPatchsetPreview, RenderError> {
        let shell = self.shell.as_ref();
        let mut previews = Vec::with_capacity(request.raw_patches.len());
        for raw_patch in request.raw_patches {
            let raw_patch_expanded = raw_patch.replace('\t', "        ");
            let (raw_cover, raw_diff) = split_cover(&raw_patch_expanded);
            let rendered_cover = match render_cover(shell, raw_cover, &request.cover_renderer) {
                Ok(render) => render,
                Err(_) => {
                    event!(
                        Level::ERROR,
                        "Failed to render cover preview with external program"
                    );
                    raw_cover.to_string()
                }
            };
            let rendered_patch =
                match render_patch_preview(shell, raw_diff, &request.patch_renderer) {
                    Ok(render) => render,
                    Err(_) => {
                        event!(
                            Level::ERROR,
                            "Failed to render patch preview with external program",
                        );
                        raw_diff.to_string()
                    }
                };
            previews.push(RenderedPatchPreview::new(format!(
                "{rendered_cover}---\n{rendered_patch}"
            )));
        }
        Ok(RenderedPatchsetPreview::new(previews))
    }
}

#[cfg(test)]
mod tests;
