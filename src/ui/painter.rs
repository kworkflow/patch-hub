//! Terminal painter — renders a [`UiScene`] onto a Ratatui [`Frame`].
//!
//! This module is intentionally "dumb": it receives a fully projected scene
//! and maps each node to Ratatui widgets without making any presentation
//! decisions of its own.

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style, Stylize},
    text::Text,
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::ui::{
    scene::{UiBody, UiScene},
    screens,
};

pub struct FramePainter;

impl FramePainter {
    /// Paint `scene` onto `f`, replacing the entire frame contents.
    pub fn paint(f: &mut Frame, scene: &UiScene) {
        f.render_widget(Clear, f.area());

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(1),
                Constraint::Length(3),
            ])
            .split(f.area());

        Self::paint_title(f, chunks[0]);

        match &scene.body {
            UiBody::MailingListSelection(s) => {
                screens::mailing_list::MailingListPainter::paint(f, s, chunks[1])
            }
            UiBody::Bookmarked(s) => screens::bookmarked::BookmarkedPainter::paint(f, s, chunks[1]),
            UiBody::Latest(s) => screens::latest::LatestPainter::paint(f, s, chunks[1]),
            UiBody::PatchsetDetails(s) => screens::details::DetailsPainter::paint(f, s, chunks[1]),
            UiBody::EditConfig(s) => {
                screens::edit_config::EditConfigPainter::paint(f, s, chunks[1])
            }
            UiBody::KwOps(s) => screens::kw_ops::KwOpsPainter::paint(f, s, chunks[1]),
        }

        screens::navigation_bar::NavigationBarPainter::paint(f, &scene.navigation, chunks[2]);

        if let Some(popup) = &scene.popup {
            let (x, y) = popup.dimensions;
            let rect = Self::center_rect(x, y, f.area());
            screens::popup::PopupPainter::paint(f, popup, rect);
        }
    }

    pub(super) fn center_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
        let popup_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage((100 - percent_y) / 2),
                Constraint::Percentage(percent_y),
                Constraint::Percentage((100 - percent_y) / 2),
            ])
            .split(r);

        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage((100 - percent_x) / 2),
                Constraint::Percentage(percent_x),
                Constraint::Percentage((100 - percent_x) / 2),
            ])
            .split(popup_layout[1])[1]
    }
}

impl FramePainter {
    fn paint_title(f: &mut Frame, chunk: Rect) {
        let title_block = Block::default()
            .borders(Borders::ALL)
            .style(Style::default())
            .title_alignment(Alignment::Center);

        let title = Paragraph::new(Text::styled(
            "patch-hub",
            Style::default().fg(Color::Green).bold(),
        ))
        .centered()
        .block(title_block);

        f.render_widget(title, chunk);
    }
}
