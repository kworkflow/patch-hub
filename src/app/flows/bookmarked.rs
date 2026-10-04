use tracing::debug;

use color_eyre::Result;

use crate::{
    app::{loading::LoadingIndicator, models::popup::AppPopup, screens::CurrentScreen, App},
    input::event::InputEvent,
};

impl App {
    pub async fn handle_bookmarked_patchsets(
        &mut self,
        input: InputEvent,
        loading: &mut dyn LoadingIndicator,
    ) -> Result<()> {
        match input {
            InputEvent::OpenHelp => {
                let popup = Self::build_bookmarked_help_popup();
                self.state.popup = Some(popup);
            }
            InputEvent::Back => {
                self.state.user_state.bookmarked_patchsets.patchset_index = 0;
                self.set_current_screen(CurrentScreen::MailingListSelection);
            }
            InputEvent::NavigateDown => {
                self.state
                    .user_state
                    .bookmarked_patchsets
                    .select_below_patchset();
            }
            InputEvent::NavigateUp => {
                self.state
                    .user_state
                    .bookmarked_patchsets
                    .select_above_patchset();
            }
            InputEvent::OpenPatchsetDetails => {
                debug!("loading patchset details from bookmarks");
                loading.start("Loading patchset".to_string());
                let result = self.open_patchset_details().await;
                loading.stop()?;
                // If a patchset has been bookmarked via the UI, b4 was already
                // successful for it, but patchsets may also arrive here from
                // other sources where the fetch could fail.
                self.apply_open_patchset_result(CurrentScreen::BookmarkedPatchsets, result);
            }
            _ => {}
        }
        Ok(())
    }

    pub fn build_bookmarked_help_popup() -> AppPopup {
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
}
