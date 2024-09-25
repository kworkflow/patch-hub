use crate::app::{screens::CurrentScreen, App};
use ratatui::crossterm::event::{KeyCode, KeyEvent};

pub fn handle_bookmarked_patchsets(app: &mut App, key: KeyEvent) -> color_eyre::Result<()> {
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
            app.init_patchset_details_and_actions_state(CurrentScreen::BookmarkedPatchsets)?;
            app.set_current_screen(CurrentScreen::PatchsetDetails);
        }
        _ => {}
    }
    Ok(())
}
