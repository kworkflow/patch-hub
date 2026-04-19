//! Rich patch/cover preview via external programs (`bat`, `delta`, `diff-so-fancy`).
//!
//! `ShellRenderService` is wired from `App` in a follow-up commit; this module is
//! compiled first so the trait and implementation exist without dead-code noise.

#![allow(dead_code)]

mod r#trait;

pub use r#trait::{RenderError, RenderServiceApi};

use std::sync::Arc;

use crate::{
    app::cover_renderer::render_cover, app::patch_renderer::render_patch_preview,
    infrastructure::shell::ShellTrait, lore::infrastructure::patchset_parser::split_cover,
};

use crate::app::cover_renderer::CoverRenderer;
use crate::app::patch_renderer::PatchRenderer;

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
        raw_patches: &[String],
        patch_renderer: &PatchRenderer,
        cover_renderer: &CoverRenderer,
    ) -> Result<Vec<String>, RenderError> {
        let shell = self.shell.as_ref();
        let mut previews = Vec::with_capacity(raw_patches.len());
        for raw_patch in raw_patches {
            let raw_patch_expanded = raw_patch.replace('\t', "        ");
            let (raw_cover, raw_diff) = split_cover(&raw_patch_expanded);
            let rendered_cover = render_cover(shell, raw_cover, cover_renderer)
                .map_err(|e| RenderError::Failed(e.to_string()))?;
            let rendered_patch = render_patch_preview(shell, raw_diff, patch_renderer)
                .map_err(|e| RenderError::Failed(e.to_string()))?;
            previews.push(format!("{rendered_cover}---\n{rendered_patch}"));
        }
        Ok(previews)
    }
}

#[cfg(test)]
mod tests;
