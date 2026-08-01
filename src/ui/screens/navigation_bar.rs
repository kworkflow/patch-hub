use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    text::Line,
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::ui::scene::NavigationBarScene;

pub fn paint(f: &mut Frame, scene: &NavigationBarScene, chunk: Rect) {
    let mode_footer = Paragraph::new(Line::from(scene.mode_spans.clone()))
        .block(Block::default().borders(Borders::ALL))
        .centered();

    let keys_hint_footer = Paragraph::new(Line::from(scene.keys_hint.clone()))
        .block(Block::default().borders(Borders::ALL))
        .centered();

    let footer_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(80)])
        .split(chunk);

    f.render_widget(mode_footer, footer_chunks[0]);
    f.render_widget(keys_hint_footer, footer_chunks[1]);
}
