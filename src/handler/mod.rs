mod bookmarked;
mod details_actions;
mod edit_config;
mod latest;
mod mail_list;

use std::{
    ops::ControlFlow,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use tokio::{sync::mpsc, task::JoinHandle};

use crate::{
    app::{screens::CurrentScreen, App},
    input::{event::InputEvent, handle::InputHandle},
    terminal::{handle::TerminalHandle, messages::TerminalFrame, TerminalError},
    ui::handle::UiHandle,
};

use bookmarked::handle_bookmarked_patchsets;
use details_actions::handle_patchset_details;
use edit_config::handle_edit_config;
use latest::handle_latest_patchsets;
use mail_list::handle_mailing_list_selection;

const LOADING_FRAME_INTERVAL: Duration = Duration::from_millis(200);

pub(crate) trait LoadingIndicator: Send {
    fn start(&mut self, title: String);
    fn stop(&mut self) -> color_eyre::Result<()>;
}

struct TerminalLoadingIndicator {
    terminal_handle: TerminalHandle,
    running: Option<Arc<AtomicBool>>,
    spinner_task: Option<JoinHandle<()>>,
}

impl TerminalLoadingIndicator {
    fn new(terminal_handle: TerminalHandle) -> Self {
        Self {
            terminal_handle,
            running: None,
            spinner_task: None,
        }
    }
}

impl LoadingIndicator for TerminalLoadingIndicator {
    fn start(&mut self, title: String) {
        if self.spinner_task.is_some() {
            return;
        }

        let running = Arc::new(AtomicBool::new(true));
        let running_clone = Arc::clone(&running);
        let terminal_handle = self.terminal_handle.clone();

        self.running = Some(running);
        self.spinner_task = Some(tokio::spawn(async move {
            while running_clone.load(Ordering::Relaxed) {
                if terminal_handle
                    .draw(TerminalFrame::Loading(title.clone()))
                    .await
                    .is_err()
                {
                    break;
                }

                std::thread::sleep(LOADING_FRAME_INTERVAL);
            }
        }));

        std::thread::sleep(LOADING_FRAME_INTERVAL);
    }

    fn stop(&mut self) -> color_eyre::Result<()> {
        let Some(spinner_task) = self.spinner_task.take() else {
            return Ok(());
        };

        if let Some(running) = self.running.take() {
            running.store(false, Ordering::Relaxed);
        }

        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async { spinner_task.await.ok() });
        });

        Ok(())
    }
}

fn terminal_error(error: TerminalError) -> color_eyre::Report {
    color_eyre::eyre::eyre!("{error}")
}

async fn input_handling(
    app: &mut App,
    input: InputEvent,
    terminal_handle: &TerminalHandle,
    loading: &mut TerminalLoadingIndicator,
) -> color_eyre::Result<ControlFlow<()>> {
    if let Some(popup) = app.state.popup.as_mut() {
        if input == InputEvent::ClosePopup {
            app.state.popup = None;
        } else {
            popup.handle_scroll(input);
        }
    } else {
        match app.state.navigation.current_screen {
            CurrentScreen::MailingListSelection => {
                match handle_mailing_list_selection(app, input, loading).await? {
                    ControlFlow::Continue(()) => {}
                    ControlFlow::Break(()) => return Ok(ControlFlow::Break(())),
                }
            }
            CurrentScreen::BookmarkedPatchsets => {
                handle_bookmarked_patchsets(app, input, loading).await?;
            }
            CurrentScreen::PatchsetDetails => {
                handle_patchset_details(app, input, terminal_handle).await?;
            }
            CurrentScreen::EditConfig => {
                handle_edit_config(app, input)?;
            }
            CurrentScreen::LatestPatchsets => {
                handle_latest_patchsets(app, input, loading).await?;
            }
        }
    }
    Ok(ControlFlow::Continue(()))
}

pub(crate) async fn run_app(
    mut app: App,
    terminal_handle: TerminalHandle,
    ui_handle: UiHandle,
    input_handle: InputHandle,
    mut app_input_rx: mpsc::Receiver<InputEvent>,
) -> color_eyre::Result<()> {
    let mut loading = TerminalLoadingIndicator::new(terminal_handle.clone());

    loop {
        app.process_system_updates(&mut loading).await?;

        let scene = ui_handle
            .build_scene(app.present())
            .await
            .map_err(|e| color_eyre::eyre::eyre!("{e}"))?;
        terminal_handle
            .draw(TerminalFrame::Main(Box::new(scene)))
            .await
            .map_err(terminal_error)?;

        match app_input_rx.recv().await {
            Some(input) => {
                match input_handling(&mut app, input, &terminal_handle, &mut loading).await? {
                    ControlFlow::Continue(()) => {}
                    ControlFlow::Break(()) => return Ok(()),
                }
                input_handle.update_context(app.input_context()).await.ok();
            }
            None => return Ok(()), // InputActor stopped
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::terminal::{actor::TerminalActor, session::MockTerminalSessionApi};

    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn loading_indicator_draws_loading_frame_through_terminal_handle() {
        let mut session = MockTerminalSessionApi::new();
        session
            .expect_draw()
            .withf(|frame| matches!(frame, TerminalFrame::Loading(_)))
            .times(1..)
            .returning(|_| Ok(()));
        let handle = TerminalActor::spawn(Box::new(session));
        let mut loading = TerminalLoadingIndicator::new(handle);

        loading.start("Fetching mailing lists".to_string());
        std::thread::sleep(LOADING_FRAME_INTERVAL);
        loading.stop().unwrap();
    }
}
