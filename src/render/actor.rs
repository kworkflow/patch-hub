use tokio::{
    sync::{mpsc, oneshot},
    task,
};

use crate::render::{
    handle::RenderHandle,
    messages::{RenderMessage, RenderResult},
    RenderError, RenderServiceApi,
};

pub const DEFAULT_RENDER_CHANNEL_SIZE: usize = 32;

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
        tokio::spawn(Self::new(core, rx).run());
        RenderHandle::new(tx)
    }

    pub async fn run(mut self) {
        tracing::info!("render actor started");
        while let Some(message) = self.rx.recv().await {
            self.handle_message(message).await;
        }
        tracing::info!("render actor stopped");
    }

    async fn handle_message(&mut self, message: RenderMessage) {
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
                send_render_reply(message_name, reply, result);
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

fn send_render_reply<T>(
    message_name: &'static str,
    reply: oneshot::Sender<RenderResult<T>>,
    result: RenderResult<T>,
) {
    if let Err(error) = &result {
        tracing::warn!(
            message = message_name,
            error = %error,
            "render request failed"
        );
    }

    if reply.send(result).is_err() {
        tracing::warn!(
            message = message_name,
            "render reply receiver dropped before response"
        );
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::{
        infrastructure::shell::OsShell,
        render::{RenderPatchsetRequest, ShellRenderService},
        render_prefs::{CoverRenderer, PatchRenderer},
    };

    use super::*;

    fn spawn_test_actor() -> RenderHandle {
        RenderActor::spawn(Box::new(ShellRenderService::new(Arc::new(OsShell))))
    }

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
