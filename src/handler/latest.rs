use crate::app::{screens::CurrentScreen, App};
use ratatui::crossterm::event::{KeyCode, KeyEvent};

pub fn handle_latest_patchsets(app: &mut App, key: KeyEvent) -> color_eyre::Result<()> {
    let latest_patchsets = app.latest_patchsets_state.as_mut().unwrap();

    match key.code {
        KeyCode::Esc => {
            app.reset_latest_patchsets_state();
            app.set_current_screen(CurrentScreen::MailingListSelection);
        }
        KeyCode::Char('j') | KeyCode::Down => {
            latest_patchsets.select_below_patchset();
        }
        KeyCode::Char('k') | KeyCode::Up => {
            latest_patchsets.select_above_patchset();
        }
        KeyCode::Char('l') | KeyCode::Right => {
            latest_patchsets.increment_page();
            latest_patchsets.fetch_current_page()?;
        }
        KeyCode::Char('h') | KeyCode::Left => {
            latest_patchsets.decrement_page();
        }
        KeyCode::Enter => {
            app.init_patchset_details_and_actions_state(CurrentScreen::LatestPatchsets)?;
            app.set_current_screen(CurrentScreen::PatchsetDetails);
        }
        _ => {}
    }
    Ok(())
}
