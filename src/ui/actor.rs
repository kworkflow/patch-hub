use std::ops::ControlFlow;

use tokio::sync::{mpsc, oneshot};

use crate::ui::{
    core::UiCore,
    handle::UiHandle,
    messages::{UiMessage, UiResult},
};

pub const DEFAULT_UI_CHANNEL_SIZE: usize = 32;

pub struct UiActor {
    core: UiCore,
    rx: mpsc::Receiver<UiMessage>,
}

impl UiActor {
    pub fn new(rx: mpsc::Receiver<UiMessage>) -> Self {
        Self {
            core: UiCore::new(),
            rx,
        }
    }

    pub fn spawn() -> UiHandle {
        let (tx, rx) = mpsc::channel(DEFAULT_UI_CHANNEL_SIZE);
        tracing::debug!(channel_size = DEFAULT_UI_CHANNEL_SIZE, "spawning ui actor");
        tokio::spawn(Self::new(rx).run());
        UiHandle::new(tx)
    }

    pub async fn run(mut self) {
        tracing::info!("ui actor started");
        while let Some(message) = self.rx.recv().await {
            if let ControlFlow::Break(()) = self.handle_message(message) {
                break;
            }
        }
        tracing::info!("ui actor stopped");
    }

    fn handle_message(&self, message: UiMessage) -> ControlFlow<()> {
        let message_name = message.name();
        tracing::debug!(message = message_name, "ui request received");

        match message {
            UiMessage::BuildScene { app_view, reply_to } => {
                tracing::debug!(
                    screen = ?std::mem::discriminant(&app_view.screen),
                    has_popup = app_view.popup.is_some(),
                    "building ui scene"
                );
                let result = self.core.build_scene(&app_view);
                tracing::debug!(ok = result.is_ok(), "ui scene built");
                send_ui_reply(message_name, reply_to, result);
                ControlFlow::Continue(())
            }
            UiMessage::Shutdown => {
                tracing::debug!("ui actor shutting down");
                ControlFlow::Break(())
            }
        }
    }
}

fn send_ui_reply<T>(
    message_name: &'static str,
    reply: oneshot::Sender<UiResult<T>>,
    result: UiResult<T>,
) {
    if let Err(error) = &result {
        tracing::warn!(
            message = message_name,
            error = %error,
            "ui request failed"
        );
    }

    if reply.send(result).is_err() {
        tracing::warn!(
            message = message_name,
            "ui reply receiver dropped before response"
        );
    }
}

#[cfg(test)]
mod tests {
    use tokio::sync::mpsc;

    use crate::{
        app::view_model::{
            AppViewModel, MailingListSelectionViewModel, ScreenViewModel, TargetListStatus,
        },
        ui::{errors::UiError, handle::UiHandle},
    };

    use super::{UiActor, DEFAULT_UI_CHANNEL_SIZE};

    fn minimal_vm() -> AppViewModel {
        AppViewModel {
            screen: ScreenViewModel::MailingListSelection(MailingListSelectionViewModel {
                entries: vec![],
                highlighted_index: 0,
                target_list: String::new(),
                target_list_status: TargetListStatus::Empty,
            }),
            popup: None,
        }
    }

    fn spawn_test_actor() -> UiHandle {
        UiActor::spawn()
    }

    #[tokio::test]
    async fn build_scene_returns_ok_for_valid_view_model() {
        let handle = spawn_test_actor();

        let result = handle.build_scene(minimal_vm()).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn sequential_build_scene_calls_succeed() {
        let handle = spawn_test_actor();

        let result1 = handle.build_scene(minimal_vm()).await;
        let result2 = handle.build_scene(minimal_vm()).await;

        assert!(result1.is_ok());
        assert!(result2.is_ok());
    }

    #[tokio::test]
    async fn closed_channel_returns_build_error() {
        let (tx, rx) = mpsc::channel(DEFAULT_UI_CHANNEL_SIZE);
        let handle = UiHandle::new(tx);
        drop(rx);

        let result = handle.build_scene(minimal_vm()).await;

        assert!(matches!(result, Err(UiError::Build(_))));
    }
}
