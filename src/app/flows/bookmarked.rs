use tracing::debug;

use crate::{
    app::{loading::LoadingIndicator, popup::AppPopup, screens::CurrentScreen, App, B4Result},
    input::event::InputEvent,
};

pub async fn handle_bookmarked_patchsets(
    app: &mut App,
    input: InputEvent,
    loading: &mut dyn LoadingIndicator,
) -> color_eyre::Result<()> {
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
                        app.state.popup = Some(AppPopup::info(
                            "Error",
                            format!("The selected patchset couldn't be retrieved.\nReason: {err_cause}\nPlease choose another patchset."),
                        ));
                        app.set_current_screen(CurrentScreen::BookmarkedPatchsets);
                    }
                }
            }
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
