use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use tokio::sync::mpsc;

use crate::{
    app::actor::AppActor,
    input::{actor::InputActor, event::InputEvent},
    terminal::{actor::TerminalActor, messages::TerminalFrame, session::MockTerminalSessionApi},
    ui::actor::UiActor,
};

use super::helpers::app_harness::minimal_app;

#[tokio::test(flavor = "multi_thread")]
async fn main_like_lifecycle_shuts_input_down_before_terminal() {
    let input_shutdown_complete = Arc::new(AtomicBool::new(false));
    let input_shutdown_complete_for_terminal = Arc::clone(&input_shutdown_complete);

    let mut session = MockTerminalSessionApi::new();
    session
        .expect_draw()
        .withf(|frame| matches!(frame, TerminalFrame::Main(_)))
        .times(1..)
        .returning(|_| Ok(()));
    session
        .expect_poll_event()
        .withf(|timeout| *timeout == std::time::Duration::from_millis(50))
        .times(4)
        .returning(|_| Ok(None));
    session
        .expect_shutdown()
        .withf(|| true)
        .times(1)
        .returning(move || {
            assert!(
                input_shutdown_complete_for_terminal.load(Ordering::SeqCst),
                "terminal shutdown should happen after input shutdown"
            );
            Ok(())
        });

    let terminal_handle = TerminalActor::spawn(Box::new(session));
    let ui_handle = UiActor::spawn();
    let app = minimal_app();
    let initial_input_context = app.input_context();

    let input_handle = InputActor::spawn(terminal_handle.clone(), initial_input_context);
    let input_shutdown_handle = input_handle.clone();
    let (app_input_tx, app_input_rx) = mpsc::channel::<InputEvent>(8);
    input_handle
        .subscribe_app(app_input_tx.clone())
        .await
        .expect("app subscribes");

    let app_handle = AppActor::spawn(
        app,
        terminal_handle.clone(),
        ui_handle.clone(),
        input_handle,
        app_input_rx,
    );

    app_input_tx
        .send(InputEvent::Quit)
        .await
        .expect("quit sends");
    app_handle.run_until_done().await.expect("actor finishes");

    input_shutdown_handle
        .shutdown()
        .await
        .expect("actor shuts down");
    input_shutdown_complete.store(true, Ordering::SeqCst);

    ui_handle.shutdown().await;
    terminal_handle.shutdown().await.expect("actor shuts down");
}
