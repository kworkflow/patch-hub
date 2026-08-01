use std::ops::ControlFlow;

use color_eyre::Result;
use tracing::debug;

use crate::{
    app::{loading::LoadingIndicator, popup::AppPopup, screens::CurrentScreen, App},
    input::event::InputEvent,
};

pub async fn handle_mailing_list_selection(
    app: &mut App,
    input: InputEvent,
    loading: &mut dyn LoadingIndicator,
) -> Result<ControlFlow<(), ()>> {
    match input {
        InputEvent::OpenHelp => {
            let popup = generate_help_popup();
            app.state.popup = Some(popup);
        }
        InputEvent::OpenLatestPatchsets => {
            if app
                .state
                .lore
                .mailing_list_selection
                .has_valid_target_list()
            {
                app.init_latest_patchsets();
                let list_name = app
                    .state
                    .lore
                    .latest_patchsets
                    .as_ref()
                    .expect("invariant: init_latest_patchsets was just called")
                    .target_list()
                    .to_string();

                debug!(list = list_name, "fetching latest patchsets");
                loading.start(format!("Fetching patchsets from {list_name}"));
                let result = app.fetch_latest_current_page().await;
                loading.stop()?;
                if result.is_ok() {
                    app.state.lore.mailing_list_selection.clear_target_list();
                    app.set_current_screen(CurrentScreen::LatestPatchsets);
                }
                result?;
            }
        }
        InputEvent::RefreshMailingLists => {
            debug!("refreshing available mailing lists");
            loading.start("Refreshing lists".to_string());
            let result = app.refresh_mailing_lists().await;
            loading.stop()?;
            result?;
        }
        InputEvent::OpenEditConfig => {
            app.init_edit_config();
            app.set_current_screen(CurrentScreen::EditConfig);
        }
        InputEvent::OpenBookmarkedPatchsets => {
            if !app
                .state
                .user_state
                .bookmarked_patchsets
                .bookmarked_patchsets
                .is_empty()
            {
                app.state.lore.mailing_list_selection.clear_target_list();
                app.set_current_screen(CurrentScreen::BookmarkedPatchsets);
            }
        }
        InputEvent::Backspace => {
            app.state
                .lore
                .mailing_list_selection
                .remove_last_target_list_char();
        }
        InputEvent::Quit => {
            return Ok(ControlFlow::Break(()));
        }
        InputEvent::TextInput(ch) => {
            app.state
                .lore
                .mailing_list_selection
                .push_char_to_target_list(ch);
        }
        InputEvent::NavigateDown => {
            app.state.lore.mailing_list_selection.highlight_below_list();
        }
        InputEvent::NavigateUp => {
            app.state.lore.mailing_list_selection.highlight_above_list();
        }
        _ => {}
    }
    Ok(ControlFlow::Continue(()))
}

pub fn generate_help_popup() -> AppPopup {
    AppPopup::help()
        .title("Mailing List Selection")
        .description("This is the mailing list selection screen.\nYou can select a mailing list by typing the name of the list.")
        .keybind("ESC", "Exit")
        .keybind("ENTER", "Open the selected mailing list")
        .keybind("?", "Show this help screen")
        .keybind("🡇", "Down")
        .keybind("🡅", "Up")
        .keybind("F1", "Show bookmarked patchsets")
        .keybind("F2", "Edit config options")
        .keybind("F5", "Refresh lists")
        .build()
}
