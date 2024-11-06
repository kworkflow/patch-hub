use std::ops::ControlFlow;

use crate::{
    app::{screens::CurrentScreen, App},
    loading_screen,
};
use ratatui::{
    crossterm::event::{KeyCode, KeyEvent},
    prelude::Backend,
    Terminal,
};

pub fn handle_bookmarked_patchsets<B>(
    app: &mut App,
    key: KeyEvent,
    mut terminal: Terminal<B>,
) -> color_eyre::Result<ControlFlow<(), Terminal<B>>>
where
    B: Backend + Send + 'static,
{
    match key.code {
        KeyCode::Esc => {
            app.bookmarked_patchsets_state.patchset_index = 0;
            app.set_current_screen(CurrentScreen::MailingListSelection);
        }
        KeyCode::Char('j') | KeyCode::Down => {
            app.bookmarked_patchsets_state.select_below_patchset();
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.bookmarked_patchsets_state.select_above_patchset();
        }
        KeyCode::Enter => {
            terminal = loading_screen! {
                terminal,
                "Loading patchset" => {
                    app.init_details_actions(CurrentScreen::BookmarkedPatchsets)?;
                    app.set_current_screen(CurrentScreen::PatchsetDetails);
                }
            };
        }
        _ => {}
    }
    Ok(ControlFlow::Continue(terminal))
}
