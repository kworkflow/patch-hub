use std::time::Duration;

use ratatui::crossterm::event::KeyCode;
use tokio::sync::{mpsc, oneshot};

use crate::{
    input::event::TerminalEvent,
    terminal::{
        messages::{TerminalFrame, TerminalMessage, TerminalResult},
        TerminalError,
    },
};

#[derive(Clone)]
pub struct TerminalHandle {
    tx: mpsc::Sender<TerminalMessage>,
}

impl TerminalHandle {
    pub fn new(tx: mpsc::Sender<TerminalMessage>) -> Self {
        Self { tx }
    }

    pub async fn draw(&self, frame: TerminalFrame) -> TerminalResult<()> {
        self.request_result(|reply| TerminalMessage::Draw { frame, reply })
            .await
    }

    pub async fn read_event(&self) -> TerminalResult<Option<TerminalEvent>> {
        self.request_result(|reply| TerminalMessage::ReadEvent { reply })
            .await
    }

    #[allow(dead_code)] // Reserved for Phase 10 non-blocking input polling.
    pub async fn poll_event(&self, timeout: Duration) -> TerminalResult<Option<TerminalEvent>> {
        self.request_result(|reply| TerminalMessage::PollEvent { timeout, reply })
            .await
    }

    pub async fn setup_user_io(&self) -> TerminalResult<()> {
        self.request_result(|reply| TerminalMessage::SetupUserIo { reply })
            .await
    }

    pub async fn teardown_user_io(&self) -> TerminalResult<()> {
        self.request_result(|reply| TerminalMessage::TeardownUserIo { reply })
            .await
    }

    pub async fn wait_for_key_press(
        &self,
        key: KeyCode,
        timeout: Duration,
    ) -> TerminalResult<bool> {
        self.request_result(|reply| TerminalMessage::WaitForKeyPress {
            key,
            timeout,
            reply,
        })
        .await
    }

    pub async fn size(&self) -> TerminalResult<(u16, u16)> {
        self.request_result(|reply| TerminalMessage::GetSize { reply })
            .await
    }

    pub async fn shutdown(&self) -> TerminalResult<()> {
        self.request_result(|reply| TerminalMessage::Shutdown { reply })
            .await
    }

    async fn request_result<T>(
        &self,
        build_message: impl FnOnce(oneshot::Sender<TerminalResult<T>>) -> TerminalMessage,
    ) -> TerminalResult<T>
    where
        T: Send + 'static,
    {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(build_message(reply))
            .await
            .map_err(|_| TerminalError::ActorUnavailable("request channel closed".to_string()))?;
        rx.await
            .map_err(|_| TerminalError::ActorUnavailable("reply channel closed".to_string()))?
    }
}
