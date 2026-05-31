use ratatui::{prelude::Backend, Terminal};

use crate::{
    app::{screens::CurrentScreen, App},
    loading_screen,
};

impl App {
    /// Processes app-driven updates that are not direct user input.
    pub async fn process_system_updates<B>(
        &mut self,
        mut terminal: Terminal<B>,
    ) -> color_eyre::Result<Terminal<B>>
    where
        B: Backend + Send + 'static,
    {
        match self.state.navigation.current_screen {
            CurrentScreen::MailingListSelection => {
                if self
                    .state
                    .lore
                    .mailing_list_selection
                    .mailing_lists
                    .is_empty()
                {
                    terminal = loading_screen! {
                        terminal, "Fetching mailing lists" => {
                            self.refresh_mailing_lists().await
                        }
                    };
                }
            }
            CurrentScreen::LatestPatchsets => {
                let patchsets_state = self.state.lore.latest_patchsets.as_ref().unwrap();

                if patchsets_state.processed_patchsets_count() == 0 {
                    let target_list = patchsets_state.target_list().to_string();
                    terminal = loading_screen! {
                        terminal,
                        format!("Fetching patchsets from {}", target_list) => {
                            self.fetch_latest_current_page().await
                        }
                    };

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

        Ok(terminal)
    }
}
