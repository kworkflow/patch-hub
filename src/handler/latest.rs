use crate::{
    app::{screens::CurrentScreen, App, B4Result},
    handler::LoadingIndicator,
    input::event::InputEvent,
    ui::popup::{help::HelpPopUpBuilder, info_popup::InfoPopUp, PopUp},
};

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
                .unwrap()
                .select_below_patchset();
        }
        InputEvent::NavigateUp => {
            app.state
                .lore
                .latest_patchsets
                .as_mut()
                .unwrap()
                .select_above_patchset();
        }
        InputEvent::NextPage => {
            let list_name = app
                .state
                .lore
                .latest_patchsets
                .as_ref()
                .unwrap()
                .target_list()
                .to_string();
            loading.start(format!("Fetching patchsets from {}", list_name));
            app.state
                .lore
                .latest_patchsets
                .as_mut()
                .unwrap()
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
                .unwrap()
                .decrement_page();
            // Reload from cache (no network call since LoreAPI caches all pages)
            app.fetch_latest_current_page().await?;
        }
        InputEvent::OpenPatchsetDetails => {
            loading.start("Loading patchset".to_string());
            let result = app.open_patchset_details().await;
            loading.stop()?;
            if result.is_ok() {
                match result.unwrap() {
                    B4Result::PatchFound => {
                        app.set_current_screen(CurrentScreen::PatchsetDetails);
                    }
                    B4Result::PatchNotFound(err_cause) => {
                        app.state.popup = Some(InfoPopUp::generate_info_popup(
                            "Error",&format!("The selected patchset couldn't be retrieved.\nReason: {err_cause}\nPlease choose another patchset.")
                        ));
                        app.set_current_screen(CurrentScreen::LatestPatchsets);
                    }
                }
            }
        }
        _ => {}
    }
    Ok(())
}

pub fn generate_help_popup() -> Box<dyn PopUp> {
    let popup = HelpPopUpBuilder::new()
        .title("Latest Patchsets")
        .description("This screen allows you to see a list of the latest patchsets from a mailing list.\nYou might also be able to view the details of a patchset.")
        .keybind("ESC", "Exit")
        .keybind("ENTER", "See details of the selected patchset")
        .keybind("?", "Show this help screen")
        .keybind("j/🡇", "Down")
        .keybind("k/🡅", "Up")
        .keybind("l/🡆", "Next page")
        .keybind("h/🡄", "Previous page")
        .build();
    Box::new(popup)
}
