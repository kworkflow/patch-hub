use ratatui::{
    crossterm::event::{KeyCode, KeyEvent},
    prelude::Backend,
    Terminal,
};

use std::ops::ControlFlow;

use crate::{
    loading_screen,
    lore::lore_session::B4Result,
    model::Model,
    views::{
        popup::{help::HelpPopUpBuilder, info_popup::InfoPopUp, PopUp},
        View,
    },
};

pub fn handle_latest_patchsets<B>(
    model: &mut Model,
    key: KeyEvent,
    mut terminal: Terminal<B>,
) -> color_eyre::Result<ControlFlow<(), Terminal<B>>>
where
    B: Backend + Send + 'static,
{
    let latest_patchsets = model.latest_patchsets.as_mut().unwrap();

    match key.code {
        KeyCode::Char('?') => {
            let popup = generate_help_popup();
            model.popup = Some(popup);
        }
        KeyCode::Esc | KeyCode::Char('q') => {
            model.reset_latest_patchsets();
            model.set_current_screen(View::MailingLists);
        }
        KeyCode::Char('j') | KeyCode::Down => {
            latest_patchsets.select_below_patchset();
        }
        KeyCode::Char('k') | KeyCode::Up => {
            latest_patchsets.select_above_patchset();
        }
        KeyCode::Char('l') | KeyCode::Right => {
            let list_name = latest_patchsets.target_list().to_string();
            terminal = loading_screen! {
                terminal,
                format!("Fetching patchsets from {}", list_name) => {
                    latest_patchsets.increment_page();
                    latest_patchsets.fetch_current_page()
                }
            };
        }
        KeyCode::Char('h') | KeyCode::Left => {
            latest_patchsets.decrement_page();
        }
        KeyCode::Enter => {
            terminal = loading_screen! {
                terminal,
                "Loading patchset" => {
                    let result = model.init_details_actions();
                    if result.is_ok() {
                        match result.unwrap() {
                            B4Result::PatchFound(_) => {
                                model.set_current_screen(View::PatchsetDetails);
                            }
                            B4Result::PatchNotFound(err_cause) => {
                                model.popup = Some(InfoPopUp::generate_info_popup(
                                    "Error",&format!("The selected patchset couldn't be retrieved.\nReason: {err_cause}\nPlease choose another patchset.")
                                ));
                                model.set_current_screen(View::LatestPatchsets);
                            }
                        }
                    }
                    color_eyre::eyre::Ok(())
                }
            };
        }
        _ => {}
    }
    Ok(ControlFlow::Continue(terminal))
}

pub fn generate_help_popup() -> Box<dyn PopUp> {
    let popup = HelpPopUpBuilder::new()
        .title("Latest Patchsets")
        .description("This screen allows you to see a list of the latest patchsets from a mailing list.\nYou might also be able to view the details of a patchset.")
        .keybind("ESC", "Exit")
        .keybind("ENTER", "See details of the selected patchset")
        .keybind("?", "Show this help screen")
        .keybind("j/🡇", "Down")
        .keybind("k/🡅", "Up")
        .keybind("l/🡆", "Next page")
        .keybind("h/🡄", "Previous page")
        .build();
    Box::new(popup)
}
