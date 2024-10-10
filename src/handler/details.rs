use crate::{
    app::{screens::CurrentScreen, App},
    utils,
};
use ratatui::{
    backend::Backend,
    crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind},
    Terminal,
};

pub fn handle_patchset_details<B: Backend>(
    app: &mut App,
    key: KeyEvent,
    terminal: &mut Terminal<B>,
) -> color_eyre::Result<()> {
    let patchset_details_and_actions = app.patchset_details_and_actions_state.as_mut().unwrap();

    match key.code {
        KeyCode::Esc => {
            let ps_da_clone = patchset_details_and_actions.last_screen.clone();
            app.set_current_screen(ps_da_clone);
            app.reset_patchset_details_and_actions_state();
        }
        KeyCode::Char('j') | KeyCode::Down => {
            patchset_details_and_actions.preview_scroll_down();
        }
        KeyCode::Char('k') | KeyCode::Up => {
            patchset_details_and_actions.preview_scroll_up();
        }
        KeyCode::Char('h') | KeyCode::Left => {
            patchset_details_and_actions.preview_pan_left();
        }
        KeyCode::Char('l') | KeyCode::Right => {
            patchset_details_and_actions.preview_pan_right();
        }
        KeyCode::Char('n') => {
            patchset_details_and_actions.preview_next_patch();
        }
        KeyCode::Char('p') => {
            patchset_details_and_actions.preview_previous_patch();
        }
        KeyCode::Char('b') => {
            patchset_details_and_actions.toggle_bookmark_action();
        }
        KeyCode::Char('r') => {
            patchset_details_and_actions.toggle_reply_with_reviewed_by_action();
        }
        KeyCode::Enter => {
            if patchset_details_and_actions.actions_require_user_io() {
                utils::setup_user_io(terminal)?;
                app.consolidate_patchset_actions()?;
                println!("\nPress ENTER continue...");
                loop {
                    if let Event::Key(key) = event::read()? {
                        if key.kind == KeyEventKind::Press && key.code == KeyCode::Enter {
                            break;
                        }
                    }
                }
                utils::teardown_user_io(terminal)?;
            } else {
                app.consolidate_patchset_actions()?;
            }
            app.set_current_screen(CurrentScreen::PatchsetDetails);
        }
        _ => {}
    }
    Ok(())
}
