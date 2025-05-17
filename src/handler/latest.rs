use std::ops::ControlFlow;

use crate::{
    app::{screens::CurrentScreen, App, PatchFound},
    loading_screen,
    ui::popup::{help::HelpPopUpBuilder, info_popup::InfoPopUp, PopUp},
};
use color_eyre::eyre::Ok;
use ratatui::{
    crossterm::event::{KeyCode, KeyEvent},
    prelude::Backend,
    Terminal,
};

pub fn handle_latest_patchsets<B>(
    app: &mut App,
    key: KeyEvent,
    mut terminal: Terminal<B>,
) -> color_eyre::Result<ControlFlow<(), Terminal<B>>>
where
    B: Backend + Send + 'static,
{
    let latest_patchsets = app.latest_patchsets.as_mut().unwrap();

    match key.code {
        KeyCode::Char('?') => {
            let popup = generate_help_popup();
            app.popup = Some(popup);
        }
        KeyCode::Esc | KeyCode::Char('q') => {
            app.reset_latest_patchsets();
            app.set_current_screen(CurrentScreen::MailingListSelection);
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
                    let result = app.init_details_actions();
                    if result.is_ok() {
                        match result.unwrap() {
                            PatchFound::Found => {
                                app.set_current_screen(CurrentScreen::PatchsetDetails);
                            }

                            PatchFound::NotFound => {
                                app.popup = Some(InfoPopUp::generate_info_popup("Error","The selected patchset couldn't be retrieved.\nPlease choose another patchset."));
						        app.set_current_screen(CurrentScreen::LatestPatchsets);
                            }
                        }   
                    }
                    Ok(())
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
