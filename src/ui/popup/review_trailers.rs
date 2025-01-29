use std::collections::HashSet;

use patch_hub::lore::patch::Author;
use ratatui::{
    crossterm::event::KeyCode,
    layout::Alignment,
    style::{Color, Modifier, Style, Stylize},
    text::Line,
    widgets::{Clear, Paragraph},
};

use crate::app::screens::details_actions::DetailsActions;

use super::PopUp;

#[derive(Debug)]
pub struct ReviewTrailersPopUp {
    reviewed_by_text: String,
    tested_by_text: String,
    acked_by_text: String,
    offset: (u16, u16),
    max_offset: (u16, u16),
    dimensions: (u16, u16),
}

impl ReviewTrailersPopUp {
    /// Generate a pop-up that contains details about the code-review trailers
    /// of a specific patch.
    ///
    /// The specific patch is defined by the currently previewing patch of
    /// `@details_actions` and the information about the trailers is stored in
    /// the fields `reviewed_by`, `tested_by`, and `acked_by`, which are the
    /// tags considered for the generated pop-up. This function succeeds regardless if
    /// there are no code-review trailers for the specific patch.
    pub fn generate_trailers_popup(details_actions: &DetailsActions) -> Box<dyn PopUp> {
        let i = details_actions.preview_index;
        let mut reviewed_by_text = String::new();
        let mut tested_by_text = String::new();
        let mut acked_by_text = String::new();
        let mut columns = 0;

        // Auxiliary routines to avoid code duplications
        let mut update_columns = |line_len: usize| {
            if line_len > columns {
                columns = line_len;
            }
        };
        let mut fill_text = |text: &mut String, authors: &HashSet<Author>| {
            for author in authors {
                let author = format!(" - {}\n", author);
                text.push_str(&author);
                update_columns(author.len());
            }
        };

        fill_text(&mut reviewed_by_text, &details_actions.reviewed_by[i]);
        fill_text(&mut tested_by_text, &details_actions.tested_by[i]);
        fill_text(&mut acked_by_text, &details_actions.acked_by[i]);

        let lines = (3
            + details_actions.reviewed_by[i].len()
            + details_actions.tested_by[i].len()
            + details_actions.acked_by[i].len()) as u16;

        // TODO: Calculate percentage based on the lines and screen size
        let dimensions = (50, 40);

        Box::new(ReviewTrailersPopUp {
            reviewed_by_text,
            tested_by_text,
            acked_by_text,
            offset: (0, 0),
            max_offset: (lines, columns as u16),
            dimensions,
        })
    }
}

impl PopUp for ReviewTrailersPopUp {
    fn dimensions(&self) -> (u16, u16) {
        self.dimensions
    }

    /// Renders a centered overlaying pop-up with entries for code-review
    /// trailers of "Reviewed-by", "Tested-by", and "Acked-by".
    fn render(&self, f: &mut ratatui::Frame, chunk: ratatui::prelude::Rect) {
        let mut contents = vec![];

        // Auxilliary closure to avoid code duplication
        let mut add_entry_to_contents = |name: &str, text: &str| {
            contents.push(Line::styled(
                name.to_string(),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
                    .add_modifier(Modifier::UNDERLINED),
            ));
            for line in text.lines() {
                contents.push(Line::styled(
                    line.to_string(),
                    Style::default().fg(Color::White),
                ));
            }
            contents.push(Line::from("")); // equivalent to newline
        };

        add_entry_to_contents("Reviewed-by", &self.reviewed_by_text);
        add_entry_to_contents("Tested-by", &self.tested_by_text);
        add_entry_to_contents("Acked-by", &self.acked_by_text);

        let block = ratatui::widgets::Block::default()
            .title("Code-Review Trailers")
            .title_alignment(Alignment::Center)
            .title_style(ratatui::style::Style::default().bold().blue())
            .title_bottom(Line::styled("(ESC) Close", Style::default().bold().blue()))
            .borders(ratatui::widgets::Borders::ALL)
            .border_type(ratatui::widgets::BorderType::Double)
            .style(ratatui::style::Style::default());

        let pop_up = Paragraph::new(contents)
            .style(ratatui::style::Style::default())
            .block(block)
            .alignment(ratatui::layout::Alignment::Left)
            .scroll(self.offset);

        f.render_widget(Clear, chunk);
        f.render_widget(pop_up, chunk);
    }

    /// Handles simple one-char width navigation.
    fn handle(&mut self, key: ratatui::crossterm::event::KeyEvent) -> color_eyre::Result<()> {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if self.offset.0 > 0 {
                    self.offset.0 -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.offset.0 < self.max_offset.0 {
                    self.offset.0 += 1;
                }
            }
            KeyCode::Left | KeyCode::Char('h') => {
                if self.offset.1 > 0 {
                    self.offset.1 -= 1;
                }
            }
            KeyCode::Right | KeyCode::Char('l') => {
                if self.offset.1 < self.max_offset.1 {
                    self.offset.1 += 1;
                }
            }
            _ => {}
        }

        Ok(())
    }
}
