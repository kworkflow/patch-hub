use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use tokio::task::JoinHandle;

use crate::terminal::{handle::TerminalHandle, messages::TerminalFrame, TerminalError};

pub(crate) const LOADING_FRAME_INTERVAL: Duration = Duration::from_millis(200);

pub(crate) trait LoadingIndicator: Send {
    fn start(&mut self, title: String);
    fn stop(&mut self) -> color_eyre::Result<()>;
}

pub(crate) struct TerminalLoadingIndicator {
    terminal_handle: TerminalHandle,
    running: Option<Arc<AtomicBool>>,
    spinner_task: Option<JoinHandle<()>>,
}

impl TerminalLoadingIndicator {
    pub(crate) fn new(terminal_handle: TerminalHandle) -> Self {
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

pub(crate) fn terminal_error(error: TerminalError) -> color_eyre::Report {
    color_eyre::eyre::eyre!("{error}")
}

#[cfg(test)]
mod tests {
    use crate::terminal::{
        actor::TerminalActor, messages::TerminalFrame, session::MockTerminalSessionApi,
    };

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
