mod bookmarked;
mod details_actions;
mod edit_config;
mod latest;
mod mail_list;

use ratatui::{prelude::Backend, Terminal};

use std::{
    ops::ControlFlow,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::JoinHandle,
    time::Duration,
};

use crate::{
    app::{screens::CurrentScreen, App},
    infrastructure::terminal::{setup_user_io, teardown_user_io},
    input::{event::InputEvent, mapper::InputMapper},
    terminal::handle::TerminalHandle,
    ui::draw_ui,
};

use bookmarked::handle_bookmarked_patchsets;
use details_actions::handle_patchset_details;
use edit_config::handle_edit_config;
use latest::handle_latest_patchsets;
use mail_list::handle_mailing_list_selection;

pub(crate) trait LoadingIndicator {
    fn start(&mut self, title: String);
    fn stop(&mut self) -> color_eyre::Result<()>;
}

pub(crate) trait TerminalController {
    fn setup_user_io(&mut self) -> color_eyre::Result<()>;
    fn teardown_user_io(&mut self) -> color_eyre::Result<()>;
    fn size(&self) -> color_eyre::Result<(u16, u16)>;
}

struct TerminalLoadingIndicator<B: Backend + Send + 'static> {
    terminal: Option<Terminal<B>>,
    running: Option<Arc<AtomicBool>>,
    handle: Option<JoinHandle<Terminal<B>>>,
}

impl<B> TerminalLoadingIndicator<B>
where
    B: Backend + Send + 'static,
{
    fn new(terminal: Terminal<B>) -> Self {
        Self {
            terminal: Some(terminal),
            running: None,
            handle: None,
        }
    }

    fn terminal_mut(&mut self) -> color_eyre::Result<&mut Terminal<B>> {
        self.terminal
            .as_mut()
            .ok_or_else(|| color_eyre::eyre::eyre!("terminal unavailable while loading"))
    }

    fn into_terminal(mut self) -> color_eyre::Result<Terminal<B>> {
        self.stop()?;
        self.terminal
            .take()
            .ok_or_else(|| color_eyre::eyre::eyre!("terminal unavailable after loading"))
    }
}

impl<B> LoadingIndicator for TerminalLoadingIndicator<B>
where
    B: Backend + Send + 'static,
{
    fn start(&mut self, title: String) {
        if self.handle.is_some() {
            return;
        }

        let Some(mut terminal) = self.terminal.take() else {
            return;
        };
        let loading = Arc::new(AtomicBool::new(true));
        let loading_clone = Arc::clone(&loading);

        self.running = Some(loading);
        self.handle = Some(std::thread::spawn(move || {
            while loading_clone.load(Ordering::Relaxed) {
                terminal = crate::ui::loading_screen::render(terminal, &title);
                std::thread::sleep(Duration::from_millis(200));
            }

            terminal
        }));

        std::thread::sleep(Duration::from_millis(200));
    }

    fn stop(&mut self) -> color_eyre::Result<()> {
        let Some(handle) = self.handle.take() else {
            return Ok(());
        };

        if let Some(running) = self.running.take() {
            running.store(false, Ordering::Relaxed);
        }

        self.terminal = Some(
            handle
                .join()
                .map_err(|_| color_eyre::eyre::eyre!("loading screen thread panicked"))?,
        );
        Ok(())
    }
}

impl<B> TerminalController for TerminalLoadingIndicator<B>
where
    B: Backend + Send + 'static,
{
    fn setup_user_io(&mut self) -> color_eyre::Result<()> {
        setup_user_io(self.terminal_mut()?)
    }

    fn teardown_user_io(&mut self) -> color_eyre::Result<()> {
        teardown_user_io(self.terminal_mut()?)
    }

    fn size(&self) -> color_eyre::Result<(u16, u16)> {
        let size = self
            .terminal
            .as_ref()
            .ok_or_else(|| color_eyre::eyre::eyre!("terminal unavailable while loading"))?
            .size()?;
        Ok((size.width, size.height))
    }
}

async fn input_handling<B>(
    terminal: Terminal<B>,
    app: &mut App,
    input: InputEvent,
    terminal_handle: &TerminalHandle,
) -> color_eyre::Result<ControlFlow<(), Terminal<B>>>
where
    B: Backend + Send + 'static,
{
    let mut loading = TerminalLoadingIndicator::new(terminal);
    if let Some(popup) = app.state.popup.as_mut() {
        if input == InputEvent::ClosePopup {
            app.state.popup = None;
        } else {
            popup.handle(input)?;
        }
    } else {
        match app.state.navigation.current_screen {
            CurrentScreen::MailingListSelection => {
                match handle_mailing_list_selection(app, input, &mut loading).await? {
                    ControlFlow::Continue(()) => {}
                    ControlFlow::Break(()) => return Ok(ControlFlow::Break(())),
                }
            }
            CurrentScreen::BookmarkedPatchsets => {
                handle_bookmarked_patchsets(app, input, &mut loading).await?;
            }
            CurrentScreen::PatchsetDetails => {
                handle_patchset_details(app, input, &mut loading, terminal_handle).await?;
            }
            CurrentScreen::EditConfig => {
                handle_edit_config(app, input)?;
            }
            CurrentScreen::LatestPatchsets => {
                handle_latest_patchsets(app, input, &mut loading).await?;
            }
        }
    }
    Ok(ControlFlow::Continue(loading.into_terminal()?))
}

pub async fn run_app<B>(
    mut terminal: Terminal<B>,
    mut app: App,
    terminal_handle: TerminalHandle,
) -> color_eyre::Result<()>
where
    B: Backend + Send + 'static,
{
    let mut input_mapper = InputMapper::default();

    loop {
        let mut loading = TerminalLoadingIndicator::new(terminal);
        app.process_system_updates(&mut loading).await?;
        terminal = loading.into_terminal()?;

        terminal.draw(|f| draw_ui(f, &app.to_view_model()))?;

        if let Some(terminal_event) = terminal_handle.read_event().await? {
            let input = input_mapper.map_terminal_event(terminal_event, &app.input_context());
            if let Some(input) = input {
                match input_handling(terminal, &mut app, input, &terminal_handle).await? {
                    ControlFlow::Continue(t) => terminal = t,
                    ControlFlow::Break(_) => return Ok(()),
                }
            }
        }
    }
}
