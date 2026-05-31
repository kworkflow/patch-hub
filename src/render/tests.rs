use std::sync::Arc;

use crate::render::{RenderPatchsetRequest, RenderServiceApi, ShellRenderService};
use crate::render_prefs::{CoverRenderer, PatchRenderer};

use crate::infrastructure::shell::OsShell;

#[test]
fn shell_render_service_produces_one_preview_per_patch() {
    let svc: Arc<dyn RenderServiceApi> = Arc::new(ShellRenderService::new(Arc::new(OsShell)));
    let raw = vec!["subject\n\nbody\n---\n+line\n".to_string()];
    let request = RenderPatchsetRequest::new(raw, PatchRenderer::Default, CoverRenderer::Default);
    let out = svc
        .render_patchset_preview(request)
        .expect("default renderers should not spawn");
    assert_eq!(out.entries.len(), 1);
    assert!(out.entries[0].rendered_text.contains("---"));
}
