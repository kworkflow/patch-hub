use tracing::debug;

use crate::{
    app::{loading::LoadingIndicator, popup::AppPopup, screens::CurrentScreen, App},
    input::event::InputEvent,
};

use super::open_patchset::apply_open_patchset_result;

pub async fn handle_latest_patchsets(
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
            app.reset_latest_patchsets();
            app.set_current_screen(CurrentScreen::MailingListSelection);
        }
        InputEvent::NavigateDown => {
            app.state
                .lore
                .latest_patchsets
                .as_mut()
                .expect("invariant: latest_patchsets must be initialised on LatestPatchsets screen")
                .select_below_patchset();
        }
        InputEvent::NavigateUp => {
            app.state
                .lore
                .latest_patchsets
                .as_mut()
                .expect("invariant: latest_patchsets must be initialised on LatestPatchsets screen")
                .select_above_patchset();
        }
        InputEvent::NextPage => {
            let list_name = app
                .state
                .lore
                .latest_patchsets
                .as_ref()
                .expect("invariant: latest_patchsets must be initialised on LatestPatchsets screen")
                .target_list()
                .to_string();
            debug!(list = list_name, "fetching next page of patchsets");
            loading.start(format!("Fetching patchsets from {list_name}"));
            app.state
                .lore
                .latest_patchsets
                .as_mut()
                .expect("invariant: latest_patchsets must be initialised on LatestPatchsets screen")
                .increment_page();
            let result = app.fetch_latest_current_page().await;
            loading.stop()?;
            result?;
        }
        InputEvent::PreviousPage => {
            app.state
                .lore
                .latest_patchsets
                .as_mut()
                .expect("invariant: latest_patchsets must be initialised on LatestPatchsets screen")
                .decrement_page();
            // Reload from cache (no network call since LoreAPI caches all pages)
            app.fetch_latest_current_page().await?;
        }
        InputEvent::OpenPatchsetDetails => {
            debug!("loading patchset details from latest");
            loading.start("Loading patchset".to_string());
            let result = app.open_patchset_details().await;
            loading.stop()?;
            apply_open_patchset_result(app, CurrentScreen::LatestPatchsets, result);
        }
        _ => {}
    }
    Ok(())
}

pub fn generate_help_popup() -> AppPopup {
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
