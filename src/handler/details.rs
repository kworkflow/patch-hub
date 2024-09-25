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
    match key.code {
        KeyCode::Esc => {
            app.set_current_screen(
                app.patchset_details_and_actions_state
                    .as_ref()
                    .unwrap()
                    .last_screen
                    .clone(),
            );
            app.reset_patchset_details_and_actions_state();
        }
        KeyCode::Char('j') | KeyCode::Down => {
            app.patchset_details_and_actions_state
                .as_mut()
                .unwrap()
                .preview_scroll_down();
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.patchset_details_and_actions_state
                .as_mut()
                .unwrap()
                .preview_scroll_up();
        }
        KeyCode::Char('n') => {
            app.patchset_details_and_actions_state
                .as_mut()
                .unwrap()
                .preview_next_patch();
        }
        KeyCode::Char('p') => {
            app.patchset_details_and_actions_state
                .as_mut()
                .unwrap()
                .preview_previous_patch();
        }
        KeyCode::Char('b') => {
            app.patchset_details_and_actions_state
                .as_mut()
                .unwrap()
                .toggle_bookmark_action();
        }
        KeyCode::Char('r') => {
            app.patchset_details_and_actions_state
                .as_mut()
                .unwrap()
                .toggle_reply_with_reviewed_by_action();
        }
        KeyCode::Enter => {
            if app
                .patchset_details_and_actions_state
                .as_ref()
                .unwrap()
                .actions_require_user_io()
            {
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
