use crate::app::{screens::CurrentScreen, App};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style, Stylize},
    text::Text,
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

mod bookmarked;
mod details_actions;
mod edit_config;
mod latest;
pub mod loading_screen;
mod mail_list;
mod navigation_bar;
pub mod popup;

pub fn draw_ui(f: &mut Frame, app: &App, page_size: usize) {
    // Clear the whole screen for sanitizing reasons
    f.render_widget(Clear, f.area());

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(3),
        ])
        .split(f.area());

    render_title(f, chunks[0]);

    match app.current_screen {
        CurrentScreen::MailingListSelection => mail_list::render_main(f, app, chunks[1]),
        CurrentScreen::BookmarkedPatchsets => {
            bookmarked::render_main(f, &app.bookmarked_patchsets, chunks[1])
        }
        CurrentScreen::LatestPatchsets => latest::render_main(f, app, chunks[1], page_size),
        CurrentScreen::PatchsetDetails => details_actions::render_main(f, app, chunks[1]),
        CurrentScreen::EditConfig => edit_config::render_main(f, app, chunks[1]),
    }

    navigation_bar::render(f, app, chunks[2]);

    app.popup.as_ref().inspect(|p| {
        let (x, y) = p.dimensions();
        let rect = centered_rect(x, y, f.area());
        p.render(f, rect);
    });
}

fn render_title(f: &mut Frame, chunk: Rect) {
    let title_block = Block::default()
        .borders(Borders::ALL)
        .style(Style::default())
        .title_alignment(Alignment::Center);

    let title_content: String = "patch-hub".to_string();

    let title = Paragraph::new(Text::styled(
        title_content,
        Style::default().fg(Color::Green).bold(),
    ))
    .centered()
    .block(title_block);

    f.render_widget(title, chunk);
}

/// helper function to create a centered rect using up certain percentage of the available rect `r`
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    // Cut the given rectangle into three vertical pieces
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    // Then cut the middle vertical piece into three width-wise pieces
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1] // Return the middle chunk
}
