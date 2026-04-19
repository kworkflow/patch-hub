use ratatui::{
    crossterm::event::{KeyCode, KeyEvent},
    prelude::Backend,
    Terminal,
};

use std::ops::ControlFlow;

use crate::{
    app::{screens::CurrentScreen, App, B4Result},
    loading_screen,
    ui::popup::{help::HelpPopUpBuilder, info_popup::InfoPopUp, PopUp},
};

pub fn handle_bookmarked_patchsets<B>(
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
            app.state.popup = Some(popup);
        }
        KeyCode::Esc | KeyCode::Char('q') => {
            app.state.user_state.bookmarked_patchsets.patchset_index = 0;
            app.set_current_screen(CurrentScreen::MailingListSelection);
        }
        KeyCode::Char('j') | KeyCode::Down => {
            app.state
                .user_state
                .bookmarked_patchsets
                .select_below_patchset();
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.state
                .user_state
                .bookmarked_patchsets
                .select_above_patchset();
        }
        KeyCode::Enter => {
            terminal = loading_screen! {
                terminal,
                "Loading patchset" => {
                    let result = app.init_details_actions();
                    if result.is_ok() {
                        // If a patchset has been bookmarked UI, this means that
                        // b4 was successful in fetching it, so it shouldn't be
                        // necessary to handle this, but we can't assume that a
                        // patchset in this list was bookmarked through the UI
                        match result.unwrap() {
                            B4Result::PatchFound => {
                                app.set_current_screen(CurrentScreen::PatchsetDetails);
                            }
                            B4Result::PatchNotFound(err_cause) => {
                                app.state.popup = Some(InfoPopUp::generate_info_popup(
                                    "Error",&format!("The selected patchset couldn't be retrieved.\nReason: {err_cause}\nPlease choose another patchset.")
                                ));
                                app.set_current_screen(CurrentScreen::BookmarkedPatchsets);
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
        .title("Bookmarked Patchsets")
        .description("This screen shows all the patchsets you have bookmarked.\nThis is quite useful to keep track of patchsets you are interested in take a look later.")
        .keybind("ESC", "Exit")
        .keybind("ENTER", "See details of the selected patchset")
        .keybind("?", "Show this help screen")
        .keybind("j/🡇", "Down")
        .keybind("k/🡅", "Up")
        .build();

    Box::new(popup)
}
