//! Render actor: serializes patch and cover preview rendering.
//!
//! `RenderHandle::render_patchset_preview` sends a `RenderMessage`. The
//! actor runs a `RenderServiceApi` (typically `ShellRenderService`) on a
//! blocking pool, off the async runtime and the UI thread.
use std::ops::ControlFlow;

use tokio::{spawn, sync::mpsc, task};

use crate::infrastructure::actor_reply::ActorReplyService;
use crate::render::{
    handle::RenderHandle,
    messages::{RenderMessage, RenderResult},
    RenderError, RenderServiceApi,
};

pub const DEFAULT_RENDER_CHANNEL_SIZE: usize = 32;

const REQUEST_FAILED_LOG: &str = "render request failed";
const REPLY_DROPPED_LOG: &str = "render reply receiver dropped before response";

pub struct RenderActor {
    core: Option<Box<dyn RenderServiceApi>>,
    rx: mpsc::Receiver<RenderMessage>,
}

impl RenderActor {
    pub fn new(core: Box<dyn RenderServiceApi>, rx: mpsc::Receiver<RenderMessage>) -> Self {
        Self {
            core: Some(core),
            rx,
        }
    }

    pub fn spawn(core: Box<dyn RenderServiceApi>) -> RenderHandle {
        let (tx, rx) = mpsc::channel(DEFAULT_RENDER_CHANNEL_SIZE);
        tracing::debug!(
            channel_size = DEFAULT_RENDER_CHANNEL_SIZE,
            "spawning render actor"
        );
        spawn(Self::new(core, rx).run());
        RenderHandle::new(tx)
    }

    pub async fn run(mut self) {
        tracing::info!("render actor started");
        while let Some(message) = self.rx.recv().await {
            if let ControlFlow::Break(()) = self.handle_message(message).await {
                break;
            }
        }
        tracing::info!("render actor stopped");
    }
}

impl RenderActor {
    async fn handle_message(&mut self, message: RenderMessage) -> ControlFlow<()> {
        let message_name = message.name();
        tracing::debug!(message = message_name, "render request received");

        match message {
            RenderMessage::RenderPatchsetPreview { request, reply } => {
                tracing::debug!(
                    patch_count = request.raw_patches.len(),
                    patch_renderer = %request.patch_renderer,
                    cover_renderer = %request.cover_renderer,
                    "rendering patchset preview"
                );
                let result = self
                    .with_core(move |core| core.render_patchset_preview(request))
                    .await
                    .and_then(|result| result);
                ActorReplyService::send_actor_reply(
                    message_name,
                    REQUEST_FAILED_LOG,
                    REPLY_DROPPED_LOG,
                    reply,
                    result,
                );
                ControlFlow::Continue(())
            }
            RenderMessage::Shutdown => {
                tracing::debug!("render actor shutting down");
                ControlFlow::Break(())
            }
        }
    }

    async fn with_core<T, F>(&mut self, operation: F) -> RenderResult<T>
    where
        T: Send + 'static,
        F: FnOnce(&dyn RenderServiceApi) -> T + Send + 'static,
    {
        let core = self
            .core
            .take()
            .ok_or_else(|| RenderError::ActorUnavailable("core unavailable".to_string()))?;
        let (core, result) = task::spawn_blocking(move || {
            let result = operation(core.as_ref());
            (core, result)
        })
        .await
        .map_err(|e| RenderError::ActorUnavailable(e.to_string()))?;
        self.core = Some(core);
        Ok(result)
    }
}

#[cfg(test)]
mod tests {

    mod helpers {
        use super::super::*;
        use crate::{infrastructure::shell::OsShell, render::ShellRenderService};
        use std::sync::Arc;

        pub(super) fn spawn_test_actor() -> RenderHandle {
            RenderActor::spawn(Box::new(ShellRenderService::new(Arc::new(OsShell))))
        }
    }
    use helpers::*;

    use crate::{
        render::RenderPatchsetRequest,
        render_prefs::{CoverRenderer, PatchRenderer},
    };

    #[tokio::test]
    async fn render_patchset_preview_returns_one_entry_per_patch() {
        let handle = spawn_test_actor();
        let request = RenderPatchsetRequest::new(
            vec![
                "subject\n\nbody\n---\n+line\n".to_string(),
                "other\n\nbody\n---\n-line\n".to_string(),
            ],
            PatchRenderer::Default,
            CoverRenderer::Default,
        );

        let rendered = handle
            .render_patchset_preview(request)
            .await
            .expect("default renderers should not spawn");

        assert_eq!(rendered.entries.len(), 2);
        assert!(rendered.entries[0].rendered_text.contains("+line"));
        assert!(rendered.entries[1].rendered_text.contains("-line"));
    }
}
