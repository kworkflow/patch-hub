use std::time::Duration;

use tokio::sync::mpsc;

use crate::{
    input::{
        context::InputContext,
        event::{InputEvent, TerminalEvent},
        handle::InputHandle,
        mapper::InputMapper,
        messages::InputMessage,
    },
    terminal::handle::TerminalHandle,
};

pub const DEFAULT_INPUT_CHANNEL_SIZE: usize = 32;
const EVENT_POLL_TIMEOUT: Duration = Duration::from_millis(50);

pub struct InputActor {
    rx: mpsc::Receiver<InputMessage>,
    terminal_handle: TerminalHandle,
    mapper: InputMapper,
    context: InputContext,
    subscriber: Option<mpsc::Sender<InputEvent>>,
}

impl InputActor {
    pub fn spawn(terminal_handle: TerminalHandle, initial_context: InputContext) -> InputHandle {
        let (tx, rx) = mpsc::channel(DEFAULT_INPUT_CHANNEL_SIZE);
        tracing::debug!(
            channel_size = DEFAULT_INPUT_CHANNEL_SIZE,
            "spawning input actor"
        );
        tokio::spawn(
            Self {
                rx,
                terminal_handle,
                mapper: InputMapper::default(),
                context: initial_context,
                subscriber: None,
            }
            .run(),
        );
        InputHandle::new(tx)
    }

    pub async fn run(mut self) {
        tracing::info!("input actor started");

        // Spawn a dedicated pump subtask that polls the terminal and buffers raw
        // events.  A dedicated subtask is used so that an in-flight poll_event
        // future is never abandoned mid-read when a control message wins the
        // select! race — abandoning it would silently discard the event because
        // the TerminalActor already consumed it from the OS queue.
        let (event_tx, mut event_rx) = mpsc::channel::<TerminalEvent>(8);
        let terminal_handle = self.terminal_handle.clone();
        tokio::spawn(async move {
            loop {
                match terminal_handle.poll_event(EVENT_POLL_TIMEOUT).await {
                    Ok(Some(event)) => {
                        tracing::debug!(?event, "terminal event captured by input pump");
                        if event_tx.send(event).await.is_err() {
                            break; // InputActor stopped; pump exits cleanly.
                        }
                    }
                    Ok(None) => {} // poll timeout with no event — retry
                    Err(err) => {
                        tracing::warn!(error = %err, "terminal poll error; input pump stopping");
                        break;
                    }
                }
            }
            tracing::debug!("input event pump stopped");
        });

        loop {
            tokio::select! {
                msg = self.rx.recv() => match msg {
                    Some(InputMessage::SubscribeApp { tx }) => {
                        tracing::debug!("app subscribed to input events");
                        self.subscriber = Some(tx);
                    }
                    Some(InputMessage::UpdateContext { context }) => {
                        tracing::debug!(?context, "input context updated");
                        self.context = context;
                    }
                    Some(InputMessage::Shutdown) | None => {
                        tracing::info!("input actor stopping");
                        break;
                    }
                },
                event = event_rx.recv() => match event {
                    Some(terminal_event) => {
                        tracing::debug!(?terminal_event, "terminal event received by input actor");
                        if let Some(input_event) =
                            self.mapper.map_terminal_event(terminal_event, &self.context)
                        {
                            tracing::debug!(?input_event, "input event mapped and delivering");
                            self.deliver(input_event).await;
                        } else {
                            tracing::debug!("terminal event discarded (no mapping for current context)");
                        }
                    }
                    None => {
                        // Pump exited (terminal closed or error) — stop the actor.
                        tracing::warn!("input event pump closed; stopping input actor");
                        break;
                    }
                },
            }
        }

        tracing::info!("input actor stopped");
    }

    async fn deliver(&mut self, event: InputEvent) {
        let Some(tx) = &self.subscriber else {
            return;
        };
        if tx.send(event).await.is_err() {
            tracing::warn!("input subscriber channel closed; clearing subscriber");
            self.subscriber = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use ratatui::crossterm::event::KeyCode;

    use crate::{
        app::screens::CurrentScreen,
        input::{
            context::InputContext,
            event::{InputEvent, KeyInput, TerminalEvent},
        },
        terminal::{actor::TerminalActor, session::MockTerminalSessionApi},
    };

    use super::*;

    fn mailing_list_context() -> InputContext {
        InputContext::new(CurrentScreen::MailingListSelection)
    }

    fn details_context() -> InputContext {
        InputContext::new(CurrentScreen::PatchsetDetails)
    }

    fn spawn_test_actor(
        session: MockTerminalSessionApi,
        context: InputContext,
    ) -> (InputHandle, TerminalHandle) {
        let terminal_handle = TerminalActor::spawn(Box::new(session));
        let input_handle = InputActor::spawn(terminal_handle.clone(), context);
        (input_handle, terminal_handle)
    }

    #[tokio::test]
    async fn key_event_in_mailing_list_context_delivers_navigate_down() {
        let mut session = MockTerminalSessionApi::new();
        // First poll returns the key; subsequent polls time-out (return None).
        session
            .expect_poll_event()
            .times(1)
            .returning(|_| Ok(Some(TerminalEvent::Key(KeyInput::press(KeyCode::Down)))));
        session.expect_poll_event().returning(|_| Ok(None));

        let (input_handle, _terminal_handle) = spawn_test_actor(session, mailing_list_context());
        let (sub_tx, mut sub_rx) = mpsc::channel::<InputEvent>(8);

        input_handle.subscribe_app(sub_tx).await.unwrap();

        let received = sub_rx.recv().await;
        assert_eq!(received, Some(InputEvent::NavigateDown));
    }

    #[tokio::test]
    async fn updating_context_to_details_maps_escape_to_back() {
        let mut session = MockTerminalSessionApi::new();
        // First call returns None so the actor can process the context update
        // before the key arrives.  Second call returns the Esc key.
        // Remaining calls time-out.
        session.expect_poll_event().times(1).returning(|_| Ok(None));
        session
            .expect_poll_event()
            .times(1)
            .returning(|_| Ok(Some(TerminalEvent::Key(KeyInput::press(KeyCode::Esc)))));
        session.expect_poll_event().returning(|_| Ok(None));

        let (input_handle, _terminal_handle) = spawn_test_actor(session, mailing_list_context());
        let (sub_tx, mut sub_rx) = mpsc::channel::<InputEvent>(8);

        input_handle.subscribe_app(sub_tx).await.unwrap();
        // In MailingListSelection, Esc maps to Quit. Switch to PatchsetDetails
        // so Esc maps to Back instead.
        input_handle
            .update_context(details_context())
            .await
            .unwrap();

        let received = sub_rx.recv().await;
        assert_eq!(received, Some(InputEvent::Back));
    }

    #[tokio::test]
    async fn unmapped_terminal_event_is_discarded_without_error() {
        let mut session = MockTerminalSessionApi::new();
        // F6 has no mapping in any screen — it should be silently dropped.
        session
            .expect_poll_event()
            .times(1)
            .returning(|_| Ok(Some(TerminalEvent::Key(KeyInput::press(KeyCode::F(6))))));
        // Second event — Down — is delivered so we can wait for it to confirm
        // the actor kept running after discarding the unmapped event.
        session
            .expect_poll_event()
            .times(1)
            .returning(|_| Ok(Some(TerminalEvent::Key(KeyInput::press(KeyCode::Down)))));
        session.expect_poll_event().returning(|_| Ok(None));

        let (input_handle, _terminal_handle) = spawn_test_actor(session, mailing_list_context());
        let (sub_tx, mut sub_rx) = mpsc::channel::<InputEvent>(8);
        input_handle.subscribe_app(sub_tx).await.unwrap();

        // Only NavigateDown arrives; the F6 event is silently discarded.
        let received = sub_rx.recv().await;
        assert_eq!(received, Some(InputEvent::NavigateDown));
    }

    #[tokio::test]
    async fn shutdown_closes_subscriber_channel() {
        let mut session = MockTerminalSessionApi::new();
        session.expect_poll_event().returning(|_| Ok(None));

        let (input_handle, _terminal_handle) = spawn_test_actor(session, mailing_list_context());
        let (sub_tx, mut sub_rx) = mpsc::channel::<InputEvent>(8);

        input_handle.subscribe_app(sub_tx).await.unwrap();
        input_handle.shutdown().await.unwrap();

        // When the actor stops it drops the subscriber Sender, closing the
        // channel. recv() returns None once all senders are gone.
        assert!(sub_rx.recv().await.is_none());
    }

    #[tokio::test]
    async fn popup_open_context_maps_escape_to_close_popup() {
        let mut session = MockTerminalSessionApi::new();
        session
            .expect_poll_event()
            .times(1)
            .returning(|_| Ok(Some(TerminalEvent::Key(KeyInput::press(KeyCode::Esc)))));
        session.expect_poll_event().returning(|_| Ok(None));

        let context = details_context().with_popup_open(true);
        let (input_handle, _terminal_handle) = spawn_test_actor(session, context);
        let (sub_tx, mut sub_rx) = mpsc::channel::<InputEvent>(8);
        input_handle.subscribe_app(sub_tx).await.unwrap();

        let received = sub_rx.recv().await;
        assert_eq!(received, Some(InputEvent::ClosePopup));
    }
}
