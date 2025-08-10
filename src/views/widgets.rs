use ratatui::{
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

pub fn centered_text_widget(text: Vec<Span>) -> Paragraph<'_> {
    Paragraph::new(Line::from(text))
        .block(Block::default().borders(Borders::ALL))
        .centered()
}
