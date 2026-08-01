use crate::app::{loading::LoadingIndicator, screens::CurrentScreen, App};

impl App {
    /// Processes app-driven updates that are not direct user input.
    pub async fn process_system_updates(
        &mut self,
        loading: &mut (dyn LoadingIndicator + Send),
    ) -> color_eyre::Result<()> {
        match self.state.navigation.current_screen {
            CurrentScreen::MailingListSelection => {
                if self
                    .state
                    .lore
                    .mailing_list_selection
                    .mailing_lists
                    .is_empty()
                {
                    loading.start("Fetching mailing lists".to_string());
                    let result = self.refresh_mailing_lists().await;
                    loading.stop()?;
                    result?;
                }
            }
            CurrentScreen::LatestPatchsets => {
                let patchsets_state = self.state.lore.latest_patchsets.as_ref().unwrap();

                if patchsets_state.processed_patchsets_count() == 0 {
                    let target_list = patchsets_state.target_list().to_string();
                    loading.start(format!("Fetching patchsets from {target_list}"));
                    let result = self.fetch_latest_current_page().await;
                    loading.stop()?;
                    result?;

                    self.state.lore.mailing_list_selection.clear_target_list();
                }
            }
            CurrentScreen::BookmarkedPatchsets => {
                if self
                    .state
                    .user_state
                    .bookmarked_patchsets
                    .bookmarked_patchsets
                    .is_empty()
                {
                    self.set_current_screen(CurrentScreen::MailingListSelection);
                }
            }
            _ => {}
        }

        Ok(())
    }
}
