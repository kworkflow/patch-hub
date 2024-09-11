use app::{logging::Logger, App, CurrentScreen};
use clap::Parser;
use cli::Cli;
use ratatui::{
    backend::Backend,
    crossterm::event::{self, Event, KeyCode, KeyEventKind},
    Terminal,
};
use ui::draw_ui;

mod app;
mod cli;
mod ui;
mod utils;

fn main() -> color_eyre::Result<()> {
    // We use an unused var because we only parse for `-h|--help` and `-V|--version`
    let _args = Cli::parse();

    utils::install_hooks()?;
    let mut terminal = utils::init()?;
    let mut app = App::new();
    run_app(&mut terminal, &mut app)?;
    utils::restore()?;

    Logger::get_logger().flush();

    Ok(())
}

fn run_app<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> color_eyre::Result<()> {
    loop {
        terminal.draw(|f| draw_ui(f, app))?;

        match app.current_screen {
            CurrentScreen::MailingListSelection => {
                if app.mailing_list_selection_state.mailing_lists.is_empty() {
                    app.mailing_list_selection_state
                        .refresh_available_mailing_lists()?;
                }
            }
            CurrentScreen::BookmarkedPatchsets => {
                if app
                    .bookmarked_patchsets_state
                    .bookmarked_patchsets
                    .is_empty()
                {
                    app.set_current_screen(CurrentScreen::MailingListSelection);
                }
            }
            _ => {}
        }

        if event::poll(std::time::Duration::from_millis(16))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == event::KeyEventKind::Release {
                    // Skip events that are not KeyEventKind::Press
                    continue;
                }
                match app.current_screen {
                    CurrentScreen::MailingListSelection if key.kind == KeyEventKind::Press => {
                        match key.code {
                            KeyCode::Enter => {
                                if app.mailing_list_selection_state.has_valid_target_list() {
                                    app.init_latest_patchsets_state();
                                    app.latest_patchsets_state
                                        .as_mut()
                                        .unwrap()
                                        .fetch_current_page()?;
                                    app.mailing_list_selection_state.clear_target_list();
                                    app.set_current_screen(CurrentScreen::LatestPatchsets);
                                }
                            }
                            KeyCode::F(5) => {
                                app.mailing_list_selection_state
                                    .refresh_available_mailing_lists()?;
                            }
                            KeyCode::F(1) => {
                                if !app
                                    .bookmarked_patchsets_state
                                    .bookmarked_patchsets
                                    .is_empty()
                                {
                                    app.mailing_list_selection_state.clear_target_list();
                                    app.set_current_screen(CurrentScreen::BookmarkedPatchsets);
                                }
                            }
                            KeyCode::Backspace => {
                                app.mailing_list_selection_state
                                    .remove_last_target_list_char();
                            }
                            KeyCode::Esc => {
                                return Ok(());
                            }
                            KeyCode::Char(ch) => {
                                app.mailing_list_selection_state
                                    .push_char_to_target_list(ch);
                            }
                            KeyCode::Down => {
                                app.mailing_list_selection_state.highlight_below_list();
                            }
                            KeyCode::Up => {
                                app.mailing_list_selection_state.highlight_above_list();
                            }
                            _ => {}
                        }
                    }
                    CurrentScreen::LatestPatchsets if key.kind == KeyEventKind::Press => {
                        match key.code {
                            KeyCode::Esc => {
                                app.reset_latest_patchsets_state();
                                app.set_current_screen(CurrentScreen::MailingListSelection);
                            }
                            KeyCode::Char('j') | KeyCode::Down => {
                                app.latest_patchsets_state
                                    .as_mut()
                                    .unwrap()
                                    .select_below_patchset();
                            }
                            KeyCode::Char('k') | KeyCode::Up => {
                                app.latest_patchsets_state
                                    .as_mut()
                                    .unwrap()
                                    .select_above_patchset();
                            }
                            KeyCode::Char('l') | KeyCode::Right => {
                                app.latest_patchsets_state
                                    .as_mut()
                                    .unwrap()
                                    .increment_page();
                                app.latest_patchsets_state
                                    .as_mut()
                                    .unwrap()
                                    .fetch_current_page()?;
                            }
                            KeyCode::Char('h') | KeyCode::Left => {
                                app.latest_patchsets_state
                                    .as_mut()
                                    .unwrap()
                                    .decrement_page();
                            }
                            KeyCode::Enter => {
                                app.init_patchset_details_and_actions_state(
                                    CurrentScreen::LatestPatchsets,
                                )?;
                                app.set_current_screen(CurrentScreen::PatchsetDetails);
                            }
                            _ => {}
                        }
                    }
                    CurrentScreen::BookmarkedPatchsets if key.kind == KeyEventKind::Press => {
                        match key.code {
                            KeyCode::Esc => {
                                app.bookmarked_patchsets_state.patchset_index = 0;
                                app.set_current_screen(CurrentScreen::MailingListSelection);
                            }
                            KeyCode::Char('j') | KeyCode::Down => {
                                app.bookmarked_patchsets_state.select_below_patchset();
                            }
                            KeyCode::Char('k') | KeyCode::Up => {
                                app.bookmarked_patchsets_state.select_above_patchset();
                            }
                            KeyCode::Enter => {
                                app.init_patchset_details_and_actions_state(
                                    CurrentScreen::BookmarkedPatchsets,
                                )?;
                                app.set_current_screen(CurrentScreen::PatchsetDetails);
                            }
                            _ => {}
                        }
                    }
                    CurrentScreen::PatchsetDetails if key.kind == KeyEventKind::Press => {
                        match key.code {
                            KeyCode::Esc => {
                                app.set_current_screen(
                                    app.patchset_details_and_actions_state
                                        .as_ref()
                                        .unwrap()
                                        .last_screen
                                        .clone(),
                                );
                                app.reset_patchset_details_and_actions_state();
                            }
                            KeyCode::Char('j') | KeyCode::Down => {
                                app.patchset_details_and_actions_state
                                    .as_mut()
                                    .unwrap()
                                    .preview_scroll_down();
                            }
                            KeyCode::Char('k') | KeyCode::Up => {
                                app.patchset_details_and_actions_state
                                    .as_mut()
                                    .unwrap()
                                    .preview_scroll_up();
                            }
                            KeyCode::Char('n') => {
                                app.patchset_details_and_actions_state
                                    .as_mut()
                                    .unwrap()
                                    .preview_next_patch();
                            }
                            KeyCode::Char('p') => {
                                app.patchset_details_and_actions_state
                                    .as_mut()
                                    .unwrap()
                                    .preview_previous_patch();
                            }
                            KeyCode::Char('b') => {
                                app.patchset_details_and_actions_state
                                    .as_mut()
                                    .unwrap()
                                    .toggle_bookmark_action();
                            }
                            KeyCode::Char('r') => {
                                app.patchset_details_and_actions_state
                                    .as_mut()
                                    .unwrap()
                                    .toggle_reply_with_reviewed_by_action();
                            }
                            KeyCode::Enter => {
                                if app
                                    .patchset_details_and_actions_state
                                    .as_ref()
                                    .unwrap()
                                    .actions_require_user_io()
                                {
                                    utils::setup_user_io(terminal)?;
                                    app.consolidate_patchset_actions()?;
                                    println!("\nPress ENTER continue...");
                                    loop {
                                        if let Event::Key(key) = event::read()? {
                                            if key.kind == event::KeyEventKind::Press
                                                && key.code == KeyCode::Enter
                                            {
                                                break;
                                            }
                                        }
                                    }
                                    utils::teardown_user_io(terminal)?;
                                } else {
                                    app.consolidate_patchset_actions()?;
                                }
                                app.set_current_screen(CurrentScreen::PatchsetDetails);
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}
