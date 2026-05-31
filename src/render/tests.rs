use std::sync::Arc;

use crate::render::{RenderServiceApi, ShellRenderService};
use crate::{app::cover_renderer::CoverRenderer, app::patch_renderer::PatchRenderer};

use crate::infrastructure::shell::OsShell;

#[test]
fn shell_render_service_produces_one_preview_per_patch() {
    let svc: Arc<dyn RenderServiceApi> = Arc::new(ShellRenderService::new(Arc::new(OsShell)));
    let raw = vec!["subject\n\nbody\n---\n+line\n".to_string()];
    let out = svc
        .render_patchset_preview(&raw, &PatchRenderer::Default, &CoverRenderer::Default)
        .expect("default renderers should not spawn");
    assert_eq!(out.len(), 1);
    assert!(out[0].contains("---"));
}
