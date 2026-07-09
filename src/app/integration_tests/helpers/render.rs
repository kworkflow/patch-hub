use tokio::{spawn, sync::mpsc};

use crate::render::{
    handle::RenderHandle, messages::RenderMessage, RenderError, RenderedPatchPreview,
    RenderedPatchsetPreview,
};

pub(crate) fn render_handle_with_successful_preview() -> RenderHandle {
    let (tx, mut rx) = mpsc::channel(8);
    spawn(async move {
        while let Some(message) = rx.recv().await {
            match message {
                RenderMessage::RenderPatchsetPreview { reply, .. } => {
                    reply.send(Ok(sample_rendered_preview())).ok();
                }
                RenderMessage::Shutdown => break,
            }
        }
    });
    RenderHandle::new(tx)
}

pub(crate) fn render_handle_with_preview_failure() -> RenderHandle {
    let (tx, mut rx) = mpsc::channel(8);
    spawn(async move {
        while let Some(message) = rx.recv().await {
            match message {
                RenderMessage::RenderPatchsetPreview { reply, .. } => {
                    reply
                        .send(Err(RenderError::ActorUnavailable(
                            "render actor unavailable in test".to_string(),
                        )))
                        .ok();
                }
                RenderMessage::Shutdown => break,
            }
        }
    });
    RenderHandle::new(tx)
}

pub(crate) fn sample_rendered_preview() -> RenderedPatchsetPreview {
    RenderedPatchsetPreview::new(vec![RenderedPatchPreview::new(
        "Subject: [PATCH] test\n---\n+added line".to_string(),
    )])
}
