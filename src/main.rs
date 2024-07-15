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
                                app.set_current_screen(CurrentScreen::LatestPatchsets);
                            }
                            KeyCode::Backspace => {
                                if !app.target_list.is_empty() {
                                    app.target_list.pop();
                                }
                            }
                            KeyCode::Esc => {
                                return Ok(());
                            }
                            KeyCode::Char(value) => {
                                app.target_list.push(value);
                            }
                            _ => {}
                        }
                    },
                    CurrentScreen::LatestPatchsets if key.kind == KeyEventKind::Press => {
                        match key.code {
                            KeyCode::Esc => {
                                app.reset_latest_patchsets_state();
                                app.target_list.clear();
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
                                app.init_patchset_details_and_actions_state()?;
                                app.set_current_screen(CurrentScreen::PatchsetDetails);
                            },
                            _ => {}
                        }
                    },
                    CurrentScreen::PatchsetDetails if key.kind == KeyEventKind::Press => {
                        match key.code {
                            KeyCode::Esc => {
                                app.reset_patchset_details_and_actions_state();
                                app.set_current_screen(CurrentScreen::LatestPatchsets);
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
                            _ => {}
                        }
                    },
                    _ => {},
                }
            }
        }
    }
}