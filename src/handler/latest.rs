use crate::app::{screens::CurrentScreen, App};
use ratatui::crossterm::event::{KeyCode, KeyEvent};

pub fn handle_latest_patchsets(app: &mut App, key: KeyEvent) -> color_eyre::Result<()> {
    match key.code {
        KeyCode::Esc => {
            app.reset_latest_patchsets_state();
            app.set_current_screen(CurrentScreen::MailingListSelection);
        }
        KeyCode::Char('j') | KeyCode::Down => {
            app.latest_patchsets_state
                .as_mut()
                .unwrap()
                .select_below_patchset();
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.latest_patchsets_state
                .as_mut()
                .unwrap()
                .select_above_patchset();
        }
        KeyCode::Char('l') | KeyCode::Right => {
            app.latest_patchsets_state
                .as_mut()
                .unwrap()
                .increment_page();
            app.latest_patchsets_state
                .as_mut()
                .unwrap()
                .fetch_current_page()?;
        }
        KeyCode::Char('h') | KeyCode::Left => {
            app.latest_patchsets_state
                .as_mut()
                .unwrap()
                .decrement_page();
        }
        KeyCode::Enter => {
            app.init_patchset_details_and_actions_state(CurrentScreen::LatestPatchsets)?;
            app.set_current_screen(CurrentScreen::PatchsetDetails);
        }
        _ => {}
    }
    Ok(())
}
