use crate::views::View;
use ratatui::{
    crossterm::event::{KeyCode, KeyEvent},
    prelude::Backend,
    Terminal,
};

use std::ops::ControlFlow;

use crate::{
    loading_screen,
    model::Model,
    views::popup::{help::HelpPopUpBuilder, PopUp},
};

pub fn handle_mailing_list_selection<B>(
    model: &mut Model,
    key: KeyEvent,
    mut terminal: Terminal<B>,
) -> color_eyre::Result<ControlFlow<(), Terminal<B>>>
where
    B: Backend + Send + 'static,
{
    match key.code {
        KeyCode::Char('?') => {
            let popup = generate_help_popup();
            model.popup = Some(popup);
        }
        KeyCode::Enter => {
            if model.mailing_list_selection.has_valid_target_list() {
                model.init_latest_patchsets();
                let list_name = model
                    .latest_patchsets
                    .as_ref()
                    .unwrap()
                    .target_list()
                    .to_string();

                terminal = loading_screen! {
                    terminal,
                    format!("Fetching patchsets from {}", list_name) => {
                        let result =
                        model.latest_patchsets.as_mut().unwrap()
                        .fetch_current_page();
                        if result.is_ok() {
                            model.mailing_list_selection.clear_target_list();
                            model.set_current_screen(View::LatestPatchsets);
                        }
                        result
                    }
                };
            }
        }
        KeyCode::F(5) => {
            terminal = loading_screen! {
                terminal,
                "Refreshing lists" => {
                    model.mailing_list_selection
                        .refresh_available_mailing_lists()
                }
            };
        }
        KeyCode::F(2) => {
            model.init_edit_config();
            model.set_current_screen(View::EditConfig);
        }
        KeyCode::F(1) => {
            if !model.bookmarked_patchsets.bookmarked_patchsets.is_empty() {
                model.mailing_list_selection.clear_target_list();
                model.set_current_screen(View::BookmarkedPatchsets);
            }
        }
        KeyCode::Backspace => {
            model.mailing_list_selection.remove_last_target_list_char();
        }
        KeyCode::Esc => {
            return Ok(ControlFlow::Break(()));
        }
        KeyCode::Char(ch) => {
            model.mailing_list_selection.push_char_to_target_list(ch);
        }
        KeyCode::Down => {
            model.mailing_list_selection.highlight_below_list();
        }
        KeyCode::Up => {
            model.mailing_list_selection.highlight_above_list();
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
