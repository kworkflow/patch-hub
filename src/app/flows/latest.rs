use tracing::debug;

use color_eyre::Result;

use crate::{
    app::{loading::LoadingIndicator, models::popup::AppPopup, screens::CurrentScreen, App},
    input::event::InputEvent,
};

impl App {
    pub async fn handle_latest_patchsets(
        &mut self,
        input: InputEvent,
        loading: &mut dyn LoadingIndicator,
    ) -> Result<()> {
        match input {
            InputEvent::OpenHelp => {
                let popup = Self::build_latest_help_popup();
                self.state.popup = Some(popup);
            }
            InputEvent::Back => {
                self.reset_latest_patchsets();
                self.set_current_screen(CurrentScreen::MailingListSelection);
            }
            InputEvent::NavigateDown => {
                self.state
                    .lore
                    .latest_patchsets
                    .as_mut()
                    .expect(
                        "invariant: latest_patchsets must be initialised on LatestPatchsets screen",
                    )
                    .select_below_patchset();
            }
            InputEvent::NavigateUp => {
                self.state
                    .lore
                    .latest_patchsets
                    .as_mut()
                    .expect(
                        "invariant: latest_patchsets must be initialised on LatestPatchsets screen",
                    )
                    .select_above_patchset();
            }
            InputEvent::NextPage => {
                let list_name = self
                    .state
                    .lore
                    .latest_patchsets
                    .as_ref()
                    .expect(
                        "invariant: latest_patchsets must be initialised on LatestPatchsets screen",
                    )
                    .target_list()
                    .to_string();
                debug!(list = list_name, "fetching next page of patchsets");
                loading.start(format!("Fetching patchsets from {list_name}"));
                self.state
                    .lore
                    .latest_patchsets
                    .as_mut()
                    .expect(
                        "invariant: latest_patchsets must be initialised on LatestPatchsets screen",
                    )
                    .increment_page();
                let result = self.fetch_latest_current_page().await;
                loading.stop()?;
                result?;
            }
            InputEvent::PreviousPage => {
                self.state
                    .lore
                    .latest_patchsets
                    .as_mut()
                    .expect(
                        "invariant: latest_patchsets must be initialised on LatestPatchsets screen",
                    )
                    .decrement_page();
                // Reload from cache (no network call since LoreAPI caches all pages)
                self.fetch_latest_current_page().await?;
            }
            InputEvent::OpenPatchsetDetails => {
                debug!("loading patchset details from latest");
                loading.start("Loading patchset".to_string());
                let result = self.open_patchset_details().await;
                loading.stop()?;
                self.apply_open_patchset_result(CurrentScreen::LatestPatchsets, result);
            }
            _ => {}
        }
        Ok(())
    }

    pub fn build_latest_help_popup() -> AppPopup {
        AppPopup::help()
        .title("Latest Patchsets")
        .description("This screen allows you to see a list of the latest patchsets from a mailing list.\nYou might also be able to view the details of a patchset.")
        .keybind("ESC", "Exit")
        .keybind("ENTER", "See details of the selected patchset")
        .keybind("?", "Show this help screen")
        .keybind("j/🡇", "Down")
        .keybind("k/🡅", "Up")
        .keybind("l/🡆", "Next page")
        .keybind("h/🡄", "Previous page")
        .build()
    }
}
