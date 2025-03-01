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

pub async fn handle_bookmarked_patchsets<B>(
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
        KeyCode::Esc | KeyCode::Char('q') => {
            app.bookmarked_patchsets.patchset_index = 0;
            app.set_current_screen(CurrentScreen::MailingListSelection);
        }
        KeyCode::Char('j') | KeyCode::Down => {
            app.bookmarked_patchsets.select_below_patchset();
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.bookmarked_patchsets.select_above_patchset();
        }
        KeyCode::Enter => {
            terminal = loading_screen! {
                terminal,
                "Loading patchset" => {
                    app.init_details_actions().await?;
                    app.set_current_screen(CurrentScreen::PatchsetDetails);
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
