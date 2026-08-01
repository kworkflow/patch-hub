use tracing::debug;

use color_eyre::Result;

use crate::{
    app::{loading::LoadingIndicator, popup::AppPopup, screens::CurrentScreen, App},
    input::event::InputEvent,
};

use super::open_patchset::apply_open_patchset_result;

pub async fn handle_bookmarked_patchsets(
    app: &mut App,
    input: InputEvent,
    loading: &mut dyn LoadingIndicator,
) -> Result<()> {
    match input {
        InputEvent::OpenHelp => {
            let popup = generate_help_popup();
            app.state.popup = Some(popup);
        }
        InputEvent::Back => {
            app.state.user_state.bookmarked_patchsets.patchset_index = 0;
            app.set_current_screen(CurrentScreen::MailingListSelection);
        }
        InputEvent::NavigateDown => {
            app.state
                .user_state
                .bookmarked_patchsets
                .select_below_patchset();
        }
        InputEvent::NavigateUp => {
            app.state
                .user_state
                .bookmarked_patchsets
                .select_above_patchset();
        }
        InputEvent::OpenPatchsetDetails => {
            debug!("loading patchset details from bookmarks");
            loading.start("Loading patchset".to_string());
            let result = app.open_patchset_details().await;
            loading.stop()?;
            // If a patchset has been bookmarked via the UI, b4 was already
            // successful for it, but patchsets may also arrive here from
            // other sources where the fetch could fail.
            apply_open_patchset_result(app, CurrentScreen::BookmarkedPatchsets, result);
        }
        _ => {}
    }
    Ok(())
}

pub fn generate_help_popup() -> AppPopup {
    AppPopup::help()
        .title("Bookmarked Patchsets")
        .description("This screen shows all the patchsets you have bookmarked.\nThis is quite useful to keep track of patchsets you are interested in take a look later.")
        .keybind("ESC", "Exit")
        .keybind("ENTER", "See details of the selected patchset")
        .keybind("?", "Show this help screen")
        .keybind("j/🡇", "Down")
        .keybind("k/🡅", "Up")
        .build()
}
