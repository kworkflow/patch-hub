use std::time::Duration;

use crate::{
    app::{screens::CurrentScreen, App},
    ui::popup::{help::HelpPopUpBuilder, review_trailers::ReviewTrailersPopUp, PopUp},
    utils,
};
use ratatui::{
    backend::Backend,
    crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    Terminal,
};

use super::wait_key_press;

pub async fn handle_patchset_details<B: Backend>(
    app: &mut App,
    key: KeyEvent,
    terminal: &mut Terminal<B>,
) -> color_eyre::Result<()> {
    let patchset_details_and_actions = app.details_actions.as_mut().unwrap();

    if key.modifiers.contains(KeyModifiers::SHIFT) {
        match key.code {
            KeyCode::Char('G') => patchset_details_and_actions.go_to_last_line(),
            KeyCode::Char('R') => {
                patchset_details_and_actions.toggle_reply_with_reviewed_by_action(true);
            }
            _ => {}
        }
        return Ok(());
    }

    if key.modifiers.contains(KeyModifiers::CONTROL) {
        // TODO: Get preview sub-window height w/out coupling it to UI
        let terminal_height = terminal.size().unwrap().height as usize;
        match key.code {
            KeyCode::Char('b') => {
                patchset_details_and_actions.preview_scroll_up(terminal_height);
            }
            KeyCode::Char('f') => {
                patchset_details_and_actions.preview_scroll_down(terminal_height);
            }
            KeyCode::Char('u') => {
                patchset_details_and_actions.preview_scroll_up(terminal_height / 2);
            }
            KeyCode::Char('d') => {
                patchset_details_and_actions.preview_scroll_down(terminal_height / 2);
            }
            KeyCode::Char('t') => {
                let popup =
                    ReviewTrailersPopUp::generate_trailers_popup(patchset_details_and_actions);
                app.popup = Some(popup);
            }
            _ => {}
        }
        return Ok(());
    }

    match key.code {
        KeyCode::Char('?') => {
            let popup = generate_help_popup();
            app.popup = Some(popup);
        }
        KeyCode::Esc | KeyCode::Char('q') => {
            let ps_da_clone = patchset_details_and_actions.last_screen.clone();
            app.set_current_screen(ps_da_clone);
            app.reset_details_actions();
        }
        KeyCode::Char('a') => {
            patchset_details_and_actions.toggle_apply_action();
        }
        KeyCode::Char('j') | KeyCode::Down => {
            patchset_details_and_actions.preview_scroll_down(1);
        }
        KeyCode::Char('k') | KeyCode::Up => {
            patchset_details_and_actions.preview_scroll_up(1);
        }
        KeyCode::Char('h') | KeyCode::Left => {
            patchset_details_and_actions.preview_pan_left();
        }
        KeyCode::Char('l') | KeyCode::Right => {
            patchset_details_and_actions.preview_pan_right();
        }
        KeyCode::Char('0') => {
            patchset_details_and_actions.go_to_beg_of_line();
        }
        KeyCode::Char('g') => {
            if let Ok(true) = wait_key_press('g', Duration::from_millis(500)) {
                patchset_details_and_actions.go_to_first_line();
            }
        }
        KeyCode::Char('f') => {
            patchset_details_and_actions.toggle_preview_fullscreen();
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
            patchset_details_and_actions.toggle_reply_with_reviewed_by_action(false);
        }
        KeyCode::Enter => {
            if patchset_details_and_actions.actions_require_user_io() {
                utils::setup_user_io(terminal)?;
                app.consolidate_patchset_actions().await?;
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
                app.consolidate_patchset_actions().await?;
            }
            app.set_current_screen(CurrentScreen::PatchsetDetails);
        }
        _ => {}
    }
    Ok(())
}

pub fn generate_help_popup() -> Box<dyn PopUp> {
    let popup = HelpPopUpBuilder::new()
        .title("Patchset Details and Actions")
        .description("This screen displays the details of a patchset and allows you to perform actions on it.\nA series of actions are available to you, they are:\n - Bookmark: Save the patchset for later\n - Reply with Reviewed-by: Reply to the patchset with a Reviewed-by tag")
        .keybind("ESC", "Exit")
        .keybind("ENTER", "Consolidate marked actions")
        .keybind("?", "Show this help screen")
        .keybind("j/🡇", "Scroll down")
        .keybind("k/🡅", "Scroll up")
        .keybind("h/🡄", "Pan left")
        .keybind("l/🡆", "Pan right")
        .keybind("0", "Go to start of line")
        .keybind("g", "Go to first line")
        .keybind("G", "Go to last line")
        .keybind("f", "Toggle fullscreen")
        .keybind("n", "Preview next patch")
        .keybind("p", "Preview previous patch")
        .keybind("b", "Toggle bookmark action")
        .keybind("r", "Toggle reply with Reviewed-by action")
        .keybind("Shift+r", "Toggle reply with Reviewed-by action for all patches")
        .keybind("Ctrl+t", "Show code-review trailers details")
        .build();

    Box::new(popup)
}
