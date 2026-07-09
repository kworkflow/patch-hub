//! Terminal session actor: owns raw TUI I/O on a dedicated task.
//!
//! [`TerminalHandle`](crate::terminal::handle::TerminalHandle) exposes draw,
//! poll/read event, size, and user-I/O setup as typed messages. The session
//! implementation ([`TerminalSessionApi`](crate::terminal::session::TerminalSessionApi),
//! e.g. crossterm) is moved into the actor at spawn time so no other component
//! holds the terminal directly.
use tokio::{
    spawn,
    sync::{mpsc, oneshot},
    task,
};

use crate::terminal::{
    handle::TerminalHandle,
    messages::{TerminalMessage, TerminalResult},
    session::TerminalSessionApi,
    TerminalError,
};

pub const DEFAULT_TERMINAL_CHANNEL_SIZE: usize = 32;

pub struct TerminalActor {
    session: Option<Box<dyn TerminalSessionApi>>,
    rx: mpsc::Receiver<TerminalMessage>,
}

impl TerminalActor {
    pub fn new(session: Box<dyn TerminalSessionApi>, rx: mpsc::Receiver<TerminalMessage>) -> Self {
        Self {
            session: Some(session),
            rx,
        }
    }

    pub fn spawn(session: Box<dyn TerminalSessionApi>) -> TerminalHandle {
        let (tx, rx) = mpsc::channel(DEFAULT_TERMINAL_CHANNEL_SIZE);
        tracing::debug!(
            channel_size = DEFAULT_TERMINAL_CHANNEL_SIZE,
            "spawning terminal actor"
        );
        spawn(Self::new(session, rx).run());
        TerminalHandle::new(tx)
    }

    pub async fn run(mut self) {
        tracing::info!("terminal actor started");
        while let Some(message) = self.rx.recv().await {
            self.handle_message(message).await;
        }
        tracing::info!("terminal actor stopped");
    }

    async fn handle_message(&mut self, message: TerminalMessage) {
        let message_name = message.name();
        tracing::debug!(message = message_name, "terminal request received");

        match message {
            TerminalMessage::Draw { frame, reply } => {
                let result = self
                    .with_session(move |session| session.draw(frame))
                    .await
                    .and_then(|result| result);
                send_terminal_reply(message_name, reply, result);
            }
            #[cfg(test)]
            TerminalMessage::ReadEvent { reply } => {
                let result = self
                    .with_session(|session| session.read_event())
                    .await
                    .and_then(|result| result);
                send_terminal_reply(message_name, reply, result);
            }
            TerminalMessage::PollEvent { timeout, reply } => {
                let result = self
                    .with_session(move |session| session.poll_event(timeout))
                    .await
                    .and_then(|result| result);
                send_terminal_reply(message_name, reply, result);
            }
            TerminalMessage::SetupUserIo { reply } => {
                let result = self
                    .with_session(|session| session.setup_user_io())
                    .await
                    .and_then(|result| result);
                send_terminal_reply(message_name, reply, result);
            }
            TerminalMessage::TeardownUserIo { reply } => {
                let result = self
                    .with_session(|session| session.teardown_user_io())
                    .await
                    .and_then(|result| result);
                send_terminal_reply(message_name, reply, result);
            }
            TerminalMessage::WaitForKeyPress {
                key,
                timeout,
                reply,
            } => {
                let result = self
                    .with_session(move |session| session.wait_for_key_press(key, timeout))
                    .await
                    .and_then(|result| result);
                send_terminal_reply(message_name, reply, result);
            }
            TerminalMessage::GetSize { reply } => {
                let result = self
                    .with_session(|session| session.size())
                    .await
                    .and_then(|result| result);
                send_terminal_reply(message_name, reply, result);
            }
            TerminalMessage::Shutdown { reply } => {
                let result = self
                    .with_session(|session| session.shutdown())
                    .await
                    .and_then(|result| result);
                send_terminal_reply(message_name, reply, result);
            }
        }
    }

    async fn with_session<T, F>(&mut self, operation: F) -> TerminalResult<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut dyn TerminalSessionApi) -> T + Send + 'static,
    {
        let mut session = self
            .session
            .take()
            .ok_or_else(|| TerminalError::ActorUnavailable("session unavailable".to_string()))?;
        let (session, result) = task::spawn_blocking(move || {
            let result = operation(session.as_mut());
            (session, result)
        })
        .await
        .map_err(|e| TerminalError::ActorUnavailable(e.to_string()))?;
        self.session = Some(session);
        Ok(result)
    }
}

fn send_terminal_reply<T>(
    message_name: &'static str,
    reply: oneshot::Sender<TerminalResult<T>>,
    result: TerminalResult<T>,
) {
    if let Err(error) = &result {
        tracing::warn!(
            message = message_name,
            error = %error,
            "terminal request failed"
        );
    }

    if reply.send(result).is_err() {
        tracing::warn!(
            message = message_name,
            "terminal reply receiver dropped before response"
        );
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use ratatui::crossterm::event::KeyCode;

    use crate::{
        input::event::{KeyInput, TerminalEvent},
        terminal::messages::TerminalFrame,
        terminal::session::MockTerminalSessionApi,
    };

    use super::*;

    fn spawn_test_actor(session: MockTerminalSessionApi) -> TerminalHandle {
        TerminalActor::spawn(Box::new(session))
    }

    #[tokio::test]
    async fn draw_returns_ok_from_actor() {
        let mut session = MockTerminalSessionApi::new();
        session
            .expect_draw()
            .withf(|frame| matches!(frame, TerminalFrame::Empty))
            .times(1)
            .returning(|_| Ok(()));
        let handle = spawn_test_actor(session);

        let result = handle.draw(TerminalFrame::Empty).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn read_event_returns_terminal_event_from_actor() {
        let expected = TerminalEvent::Key(KeyInput::press(KeyCode::Char('j')));
        let mut session = MockTerminalSessionApi::new();
        session
            .expect_read_event()
            .times(1)
            .returning(move || Ok(Some(expected.clone())));
        let handle = spawn_test_actor(session);

        let result = handle.read_event().await.unwrap();

        assert_eq!(
            result,
            Some(TerminalEvent::Key(KeyInput::press(KeyCode::Char('j'))))
        );
    }

    #[tokio::test]
    async fn sequential_requests_preserve_session() {
        let mut session = MockTerminalSessionApi::new();
        session
            .expect_draw()
            .withf(|frame| matches!(frame, TerminalFrame::Empty))
            .times(1)
            .returning(|_| Ok(()));
        session.expect_size().times(1).returning(|| Ok((120, 40)));
        let handle = spawn_test_actor(session);

        handle.draw(TerminalFrame::Empty).await.unwrap();
        let size = handle.size().await.unwrap();

        assert_eq!(size, (120, 40));
    }

    #[tokio::test]
    async fn wait_for_key_press_uses_requested_key_and_timeout() {
        let mut session = MockTerminalSessionApi::new();
        session
            .expect_wait_for_key_press()
            .withf(|key, timeout| *key == KeyCode::Enter && *timeout == Duration::from_millis(50))
            .times(1)
            .returning(|_, _| Ok(true));
        let handle = spawn_test_actor(session);

        let pressed = handle
            .wait_for_key_press(KeyCode::Enter, Duration::from_millis(50))
            .await
            .unwrap();

        assert!(pressed);
    }

    #[tokio::test]
    async fn setup_user_io_delegates_to_session() {
        let mut session = MockTerminalSessionApi::new();
        session.expect_setup_user_io().times(1).returning(|| Ok(()));
        let handle = spawn_test_actor(session);

        handle.setup_user_io().await.unwrap();
    }

    #[tokio::test]
    async fn teardown_user_io_delegates_to_session() {
        let mut session = MockTerminalSessionApi::new();
        session
            .expect_teardown_user_io()
            .times(1)
            .returning(|| Ok(()));
        let handle = spawn_test_actor(session);

        handle.teardown_user_io().await.unwrap();
    }

    #[tokio::test]
    async fn shutdown_delegates_to_session_and_is_safe_to_repeat() {
        let mut session = MockTerminalSessionApi::new();
        session.expect_shutdown().times(2).returning(|| Ok(()));
        let handle = spawn_test_actor(session);

        handle.shutdown().await.unwrap();
        handle.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn closed_channel_returns_actor_unavailable() {
        let (tx, rx) = mpsc::channel(DEFAULT_TERMINAL_CHANNEL_SIZE);
        let handle = TerminalHandle::new(tx);
        drop(rx);

        let result = handle.draw(TerminalFrame::Empty).await;

        assert!(matches!(result, Err(TerminalError::ActorUnavailable(_))));
    }
}
