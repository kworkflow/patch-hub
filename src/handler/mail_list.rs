use std::ops::ControlFlow;

use crate::{
    app::{screens::CurrentScreen, App},
    loading_screen,
    ui::popup::{help::HelpPopUpBuilder, PopUp},
};
use ratatui::{
    crossterm::event::{KeyCode, KeyEvent},
    prelude::Backend,
    Terminal,
};

pub async fn handle_mailing_list_selection<B>(
    app: &mut App,
    key: KeyEvent,
    mut terminal: Terminal<B>,
) -> color_eyre::Result<ControlFlow<(), Terminal<B>>>
where
    B: Backend + Send + 'static,
{
    match key.code {
        KeyCode::Char('?') => {
            let popup = generate_help_popup();
            app.popup = Some(popup);
        }
        KeyCode::Enter => {
            if app.mailing_list_selection.has_valid_target_list() {
                app.init_latest_patchsets().await;
                let list_name = app
                    .latest_patchsets
                    .as_ref()
                    .unwrap()
                    .target_list()
                    .to_string();

                terminal = loading_screen! {
                    terminal,
                    format!("Fetching patchsets from {}", list_name) => {
                        app.latest_patchsets.as_mut().unwrap().fetch_current_page()?;
                        app.mailing_list_selection.clear_target_list();
                        app.set_current_screen(CurrentScreen::LatestPatchsets);
                    }
                };
            }
        }
        KeyCode::F(5) => {
            terminal = loading_screen! {
                terminal,
                "Refreshing lists" => {
                    app.mailing_list_selection
                        .refresh_available_mailing_lists()?;
                }
            };
        }
        KeyCode::F(2) => {
            app.init_edit_config().await;
            app.set_current_screen(CurrentScreen::EditConfig);
        }
        KeyCode::F(1) => {
            if !app.bookmarked_patchsets.bookmarked_patchsets.is_empty() {
                app.mailing_list_selection.clear_target_list();
                app.set_current_screen(CurrentScreen::BookmarkedPatchsets);
            }
        }
        KeyCode::Backspace => {
            app.mailing_list_selection.remove_last_target_list_char();
        }
        KeyCode::Esc => {
            return Ok(ControlFlow::Break(()));
        }
        KeyCode::Char(ch) => {
            app.mailing_list_selection.push_char_to_target_list(ch);
        }
        KeyCode::Down => {
            app.mailing_list_selection.highlight_below_list();
        }
        KeyCode::Up => {
            app.mailing_list_selection.highlight_above_list();
        }
        _ => {}
    }
    Ok(ControlFlow::Continue(terminal))
}

// TODO: Move this to a more appropriate place
pub fn generate_help_popup() -> Box<dyn PopUp> {
    let popup = HelpPopUpBuilder::new()
        .title("Mailing List Selection")
        .description("This is the mailing list selection screen.\nYou can select a mailing list by typing the name of the list.")
        .keybind("ESC", "Exit")
        .keybind("ENTER", "Open the selected mailing list")
        .keybind("?", "Show this help screen")
        .keybind("🡇", "Down")
        .keybind("🡅", "Up")
        .keybind("F1", "Show bookmarked patchsets")
        .keybind("F2", "Edit config options")
        .keybind("F5", "Refresh lists")
        .build();

    Box::new(popup)
}
