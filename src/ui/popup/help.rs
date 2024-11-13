use ratatui::{
    crossterm::event::KeyCode,
    layout::Alignment,
    style::Stylize,
    widgets::{Clear, Paragraph},
};
use std::fmt::Display;

use super::PopUp;

/// A popup that displays a help message
///
/// This popup is used to display a help message with a title, a description and a list of keybinds.
/// It's meant to produce a new instance for each screen that needs a help popup and then the handler of this screen
/// will be responsible for pushing the popup to the app when the user presses the help key (`?` is the suggested default)
///
/// The title is displayed at the top center of the popup
/// The description is displayed below the title and is optional
/// The keybinds (also optional) are displayed in a table format with the key on the left and the help message on the right
#[allow(dead_code)]
#[derive(Debug)]
pub struct HelpPopUp {
    title: Option<String>,
    description: Option<String>,
    keybinds: String,
    offset: (u16, u16),
    max_offset: (u16, u16),
    lines: u16,
    columns: u16,
    dimensions: (u16, u16),
}

/// A helper struct to build a `HelpPopUp`
#[derive(Debug, Default)]
pub struct HelpPopUpBuilder {
    title: Option<String>,
    description: Option<String>,
    keybinds: Vec<(String, String)>,
}

impl HelpPopUpBuilder {
    /// Creates a new empty `HelpPopUpBuilder`
    pub fn new() -> Self {
        Self {
            title: None,
            description: None,
            keybinds: Vec::new(),
        }
    }

    /// Defines the title of the popup
    ///
    /// The title is displayed at the top center of the popup and
    /// it's recommended to be short and be the name of the screen
    pub fn title(mut self, title: &str) -> Self {
        self.title = Some(title.to_string());
        self
    }

    /// Defines the description of the popup
    ///
    /// The description is displayed below the title and is optional as its meant
    /// only to give some extra information about the screen for the user
    pub fn description(mut self, description: &str) -> Self {
        self.description = Some(description.to_string());
        self
    }

    /// Adds a new help entry to the popup
    ///
    /// A help entry is composed of a key and a help message
    ///
    /// The keybinds are listed in order of insertion under a `Keybinds` section in the pop-up body
    pub fn keybind<K: Display, H: Display>(mut self, key: K, help: H) -> Self {
        self.keybinds.push((key.to_string(), help.to_string()));
        self
    }

    /// Builds the `HelpPopUp` with the given parameters
    pub fn build(self) -> HelpPopUp {
        let key_len = self
            .keybinds
            .iter()
            .fold(0, |acc, (k, _)| if k.len() > acc { k.len() } else { acc });

        let help = self.keybinds.iter().fold(String::new(), |acc, (k, v)| {
            acc + &format!("{:>width$}: {}\n", k, v, width = key_len)
        });

        let lines = self.keybinds.len() as u16;

        let columns = self.keybinds.iter().fold(0, |acc, (k, v)| {
            let len = (k.len() + v.len()) as u16;
            if len > acc {
                len
            } else {
                acc
            }
        });

        let dimensions = (50, 50); // TODO: Calculate percentage based on the lines and screen size

        HelpPopUp {
            title: self.title,
            description: self.description,
            keybinds: help,
            offset: (0, 0),
            max_offset: (lines, columns),
            columns: columns + 2,
            lines,
            dimensions,
        }
    }
}

impl PopUp for HelpPopUp {
    fn dimensions(&self) -> (u16, u16) {
        self.dimensions
    }

    fn render(&self, f: &mut ratatui::Frame, chunk: ratatui::prelude::Rect) {
        let block = ratatui::widgets::Block::default()
            .title(self.to_string())
            .title_alignment(Alignment::Center)
            .title_style(ratatui::style::Style::default().bold().blue())
            .borders(ratatui::widgets::Borders::ALL)
            .border_type(ratatui::widgets::BorderType::Double)
            .style(ratatui::style::Style::default());

        // Push the description
        let text = if let Some(description) = &self.description {
            format!("{}\n\n", description)
        } else {
            String::new()
        };

        // Push the help entries
        let text = if self.keybinds.is_empty() {
            text
        } else {
            format!("{} \u{1F836} Keybinds\n{}", text, self.keybinds)
        };

        let text = Paragraph::new(text)
            .style(ratatui::style::Style::default())
            .block(block)
            .alignment(ratatui::layout::Alignment::Left)
            .scroll(self.offset);

        f.render_widget(Clear, chunk);
        f.render_widget(text, chunk);
    }

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

impl Display for HelpPopUp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(title) = &self.title {
            write!(f, "{}", title)?;
        } else {
            write!(f, "Help")?;
        }

        Ok(())
    }
}
