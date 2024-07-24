use app::{
    App, CurrentScreen
};
use ratatui::{
    backend::Backend,
    crossterm::event::{
        self, Event, KeyCode, KeyEventKind
    },
    Terminal
};
use ui::draw_ui;

mod app;
mod ui;
mod utils;

fn main() -> color_eyre::Result<()> {
    utils::install_hooks()?;
    let mut terminal = utils::init()?;
    let mut app = App::new();
    run_app(&mut terminal, &mut app)?;
    utils::restore()?;
    Ok(())
}

fn run_app<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> color_eyre::Result<()> {
    loop {
        terminal.draw(|f| draw_ui(f, &app))?;

        if app.current_screen == CurrentScreen::MailingListSelection
            && app.mailing_list_selection_state.mailing_lists.len() == 0
        {
            app.refresh_available_mailing_lists()?;
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
                                if let Some(_) = &app.latest_patchsets_state {
                                    app.reset_latest_patchsets_state();
                                }
                                app.init_latest_patchsets_state();
                                app.latest_patchsets_state.as_mut().unwrap().fetch_current_page()?;
                                app.mailing_list_selection_state.clear_target_list();
                                app.set_current_screen(CurrentScreen::LatestPatchsets);
                            }
                            KeyCode::F(5) => {
                                app.refresh_available_mailing_lists()?;
                            }
                            KeyCode::Tab => {
                                if !app.bookmarked_patchsets_state.bookmarked_patchsets.is_empty() {
                                    app.mailing_list_selection_state.clear_target_list();
                                    app.set_current_screen(CurrentScreen::BookmarkedPatchsets);
                                }
                            }
                            KeyCode::Backspace => {
                                app.mailing_list_selection_state.remove_last_target_list_char();
                            }
                            KeyCode::Esc => {
                                app.save_bookmarked_patchsets()?;
                                return Ok(());
                            }
                            KeyCode::Char(ch) => {
                                app.mailing_list_selection_state.push_char_to_target_list(ch);
                            }
                            _ => {}
                        }
                    },
                    CurrentScreen::LatestPatchsets if key.kind == KeyEventKind::Press => {
                        match key.code {
                            KeyCode::Esc => {
                                app.reset_latest_patchsets_state();
                                app.set_current_screen(CurrentScreen::MailingListSelection);
                            },
                            KeyCode::Char('j') | KeyCode::Down => {
                                app.latest_patchsets_state.as_mut().unwrap().select_below_patchset();
                            },
                            KeyCode::Char('k') | KeyCode::Up => {
                                app.latest_patchsets_state.as_mut().unwrap().select_above_patchset();
                            },
                            KeyCode::Char('l') | KeyCode::Right => {
                                app.latest_patchsets_state.as_mut().unwrap().increment_page();
                                app.latest_patchsets_state.as_mut().unwrap().fetch_current_page()?;
                            },
                            KeyCode::Char('h') | KeyCode::Left => {
                                app.latest_patchsets_state.as_mut().unwrap().decrement_page();
                            },
                            KeyCode::Enter => {
                                app.init_patchset_details_and_actions_state(CurrentScreen::LatestPatchsets)?;
                                app.set_current_screen(CurrentScreen::PatchsetDetails);
                            },
                            _ => {}
                        }
                    },
                    CurrentScreen::BookmarkedPatchsets if key.kind == KeyEventKind::Press => {
                        match key.code {
                            KeyCode::Esc => {
                                app.bookmarked_patchsets_state.patchset_index = 0;
                                app.set_current_screen(CurrentScreen::MailingListSelection);
                            },
                            KeyCode::Char('j') | KeyCode::Down => {
                                app.bookmarked_patchsets_state.select_below_patchset();
                            },
                            KeyCode::Char('k') | KeyCode::Up => {
                                app.bookmarked_patchsets_state.select_above_patchset();
                            },
                            KeyCode::Enter => {
                                app.init_patchset_details_and_actions_state(CurrentScreen::BookmarkedPatchsets)?;
                                app.set_current_screen(CurrentScreen::PatchsetDetails);
                            },
                            _ => {}
                        }
                    },
                    CurrentScreen::PatchsetDetails if key.kind == KeyEventKind::Press => {
                        match key.code {
                            KeyCode::Esc => {
                                app.set_current_screen(
                                    app.patchset_details_and_actions_state.as_ref().unwrap().last_screen.clone()
                                );
                                app.reset_patchset_details_and_actions_state();
                            },
                            KeyCode::Char('j') | KeyCode::Down => {
                                app.patchset_details_and_actions_state.as_mut().unwrap().preview_scroll_down();
                            },
                            KeyCode::Char('k') | KeyCode::Up => {
                                app.patchset_details_and_actions_state.as_mut().unwrap().preview_scroll_up();
                            },
                            KeyCode::Char('n') => {
                                app.patchset_details_and_actions_state.as_mut().unwrap().preview_next_patch();
                            },
                            KeyCode::Char('p') => {
                                app.patchset_details_and_actions_state.as_mut().unwrap().preview_previous_patch();
                            },
                            KeyCode::Char('b') => {
                                app.patchset_details_and_actions_state.as_mut().unwrap().toggle_bookmark_action();
                            },
                            KeyCode::Enter => {
                                app.consolidate_patchset_actions();
                                app.set_current_screen(CurrentScreen::PatchsetDetails);
                            },
                            _ => {}
                        }
                    },
                    _ => {},
                }
            }
        }
    }
}