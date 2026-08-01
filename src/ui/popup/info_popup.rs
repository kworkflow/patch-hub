use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
};

use super::PopUp;
use crate::input::event::InputEvent;

#[derive(Debug)]
pub struct InfoPopUp {
    title: String,
    info: String,
    offset: (u16, u16),
    max_offset: (u16, u16),
    dimensions: (u16, u16),
}

impl InfoPopUp {
    /// Generate a pop-up with a title and an arbitrary information.
    pub fn generate_info_popup(title: &str, info: &str) -> Box<dyn PopUp> {
        let mut lines = 0;
        let mut columns = 0;

        for line in info.lines() {
            lines += 1;
            let line_len = line.len() as u16;
            if line_len > columns {
                columns = line_len;
            }
        }

        let dimensions = (30, 50); // TODO: Calculate percentage based on the lines and screen size

        Box::new(InfoPopUp {
            title: title.to_string(),
            info: info.to_string(),
            offset: (0, 0),
            max_offset: (lines, columns),
            dimensions,
        })
    }
}

impl PopUp for InfoPopUp {
    fn dimensions(&self) -> (u16, u16) {
        self.dimensions
    }

    /// Renders a centered overlaying pop-up with a title and an arbitrary info.
    fn render(&self, f: &mut ratatui::Frame, chunk: Rect) {
        let bold_blue = Style::default()
            .add_modifier(Modifier::BOLD)
            .fg(Color::Blue);
        let block = Block::default()
            .title(self.title.clone())
            .title_alignment(Alignment::Center)
            .title_style(bold_blue)
            .title_bottom(Line::styled("(ESC / q) Close", bold_blue))
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .style(Style::default());

        let pop_up = Paragraph::new(self.info.clone())
            .block(block)
            .alignment(Alignment::Left)
            .wrap(Wrap { trim: true })
            .scroll(self.offset);

        f.render_widget(Clear, chunk);
        f.render_widget(pop_up, chunk);
    }

    /// Handles simple one-char width navigation.
    fn handle(&mut self, input: InputEvent) -> color_eyre::Result<()> {
        match input {
            InputEvent::NavigateUp => {
                if self.offset.0 > 0 {
                    self.offset.0 -= 1;
                }
            }
            InputEvent::NavigateDown => {
                if self.offset.0 < self.max_offset.0 {
                    self.offset.0 += 1;
                }
            }
            InputEvent::NavigateLeft => {
                if self.offset.1 > 0 {
                    self.offset.1 -= 1;
                }
            }
            InputEvent::NavigateRight => {
                if self.offset.1 < self.max_offset.1 {
                    self.offset.1 += 1;
                }
            }
            _ => {}
        }

        Ok(())
    }
}
