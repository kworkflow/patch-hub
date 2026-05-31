use ratatui::{backend::Backend, Terminal};

use crate::{
    app::{screens::CurrentScreen, App},
    infrastructure::terminal::{setup_user_io, teardown_user_io},
    input::{
        event::{InputEvent, ScrollAmount},
        terminal_source::{wait_for_enter_press, CrosstermEventSource},
    },
    ui::popup::{help::HelpPopUpBuilder, review_trailers::ReviewTrailersPopUp, PopUp},
};

pub async fn handle_patchset_details<B: Backend>(
    app: &mut App,
    input: InputEvent,
    terminal: &mut Terminal<B>,
) -> color_eyre::Result<()> {
    let patchset_details_and_actions = app.state.lore.details.as_mut().unwrap();

    match input {
        InputEvent::OpenHelp => {
            let popup = generate_help_popup();
            app.state.popup = Some(popup);
        }
        InputEvent::Back => {
            let ps_da_clone = patchset_details_and_actions.last_screen.clone();
            app.set_current_screen(ps_da_clone);
            app.reset_details_actions();
        }
        InputEvent::ToggleApply => {
            patchset_details_and_actions.toggle_apply_action();
        }
        InputEvent::PreviewScrollDown(amount) => {
            let lines = preview_scroll_lines(amount, terminal);
            patchset_details_and_actions.preview_scroll_down(lines);
        }
        InputEvent::PreviewScrollUp(amount) => {
            let lines = preview_scroll_lines(amount, terminal);
            patchset_details_and_actions.preview_scroll_up(lines);
        }
        InputEvent::PreviewPanLeft => {
            patchset_details_and_actions.preview_pan_left();
        }
        InputEvent::PreviewPanRight => {
            patchset_details_and_actions.preview_pan_right();
        }
        InputEvent::PreviewGoToBeginningOfLine => {
            patchset_details_and_actions.go_to_beg_of_line();
        }
        InputEvent::PreviewGoToFirstLine => {
            patchset_details_and_actions.go_to_first_line();
        }
        InputEvent::PreviewGoToLastLine => {
            patchset_details_and_actions.go_to_last_line();
        }
        InputEvent::TogglePreviewFullscreen => {
            patchset_details_and_actions.toggle_preview_fullscreen();
        }
        InputEvent::PreviewNext => {
            patchset_details_and_actions.preview_next_patch();
        }
        InputEvent::PreviewPrevious => {
            patchset_details_and_actions.preview_previous_patch();
        }
        InputEvent::ToggleBookmark => {
            patchset_details_and_actions.toggle_bookmark_action();
        }
        InputEvent::ToggleReplyWithReviewedBy => {
            patchset_details_and_actions.toggle_reply_with_reviewed_by_action(false);
        }
        InputEvent::ToggleReplyWithReviewedByAll => {
            patchset_details_and_actions.toggle_reply_with_reviewed_by_action(true);
        }
        InputEvent::ShowReviewTrailers => {
            let popup = ReviewTrailersPopUp::generate_trailers_popup(patchset_details_and_actions);
            app.state.popup = Some(popup);
        }
        InputEvent::ConsolidatePatchsetActions => {
            if patchset_details_and_actions.actions_require_user_io() {
                setup_user_io(terminal)?;
                app.consolidate_patchset_actions().await?;
                println!("\nPress ENTER continue...");
                let mut event_source = CrosstermEventSource;
                wait_for_enter_press(&mut event_source)?;
                teardown_user_io(terminal)?;
            } else {
                app.consolidate_patchset_actions().await?;
            }
            app.set_current_screen(CurrentScreen::PatchsetDetails);
        }
        _ => {}
    }
    Ok(())
}

fn preview_scroll_lines<B: Backend>(amount: ScrollAmount, terminal: &Terminal<B>) -> usize {
    match amount {
        ScrollAmount::Line => 1,
        ScrollAmount::HalfPage => terminal.size().unwrap().height as usize / 2,
        ScrollAmount::Page => terminal.size().unwrap().height as usize,
    }
}

pub fn generate_help_popup() -> Box<dyn PopUp> {
    let popup = HelpPopUpBuilder::new()
        .title("Patchset Details and Actions")
        .description("This screen displays the details of a patchset and allows you to perform actions on it.\nA series of actions are available to you, they are:\n - Bookmark: Save the patchset for later\n - Reply with Reviewed-by: Reply to the patchset with a Reviewed-by tag")
        .keybind("ESC", "Exit")
        .keybind("ENTER", "Consolidate marked actions")
        .keybind("?", "Show this help screen")
        .keybind("j/🡇", "Scroll down")
        .keybind("k/🡅", "Scroll up")
        .keybind("h/🡄", "Pan left")
        .keybind("l/🡆", "Pan right")
        .keybind("0", "Go to start of line")
        .keybind("g", "Go to first line")
        .keybind("G", "Go to last line")
        .keybind("f", "Toggle fullscreen")
        .keybind("n", "Preview next patch")
        .keybind("p", "Preview previous patch")
        .keybind("b", "Toggle bookmark action")
        .keybind("r", "Toggle reply with Reviewed-by action")
        .keybind("Shift+r", "Toggle reply with Reviewed-by action for all patches")
        .keybind("Ctrl+t", "Show code-review trailers details")
        .build();

    Box::new(popup)
}
