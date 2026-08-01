//! Application-layer popup state.
//!
//! `AppPopup` replaces the old `Box<dyn PopUp>` trait object in `AppState`.
//! Each variant owns all data required to present the popup and tracks scroll
//! position so the presentation layer receives a read-only snapshot.

use crate::{app::screens::details_actions::PatchsetDetailsState, input::event::InputEvent};

// ---------------------------------------------------------------------------
// Main enum
// ---------------------------------------------------------------------------

/// Concrete, cloneable popup state stored in `AppState`.
#[derive(Clone, Debug)]
pub enum AppPopup {
    Info {
        title: String,
        body: String,
        scroll: (u16, u16),
        max_scroll: (u16, u16),
        dimensions: (u16, u16),
    },
    Help {
        title: Option<String>,
        description: Option<String>,
        formatted_keybinds: String,
        scroll: (u16, u16),
        max_scroll: (u16, u16),
        dimensions: (u16, u16),
    },
    ReviewTrailers {
        reviewed_by: String,
        tested_by: String,
        acked_by: String,
        scroll: (u16, u16),
        max_scroll: (u16, u16),
        dimensions: (u16, u16),
    },
}

impl AppPopup {
    // -----------------------------------------------------------------------
    // Factories
    // -----------------------------------------------------------------------

    /// Create an informational text popup.
    pub fn info(title: impl Into<String>, body: impl Into<String>) -> Self {
        let title = title.into();
        let body = body.into();
        let mut lines = 0u16;
        let mut columns = 0u16;
        for line in body.lines() {
            lines += 1;
            let len = line.len() as u16;
            if len > columns {
                columns = len;
            }
        }
        AppPopup::Info {
            title,
            body,
            scroll: (0, 0),
            max_scroll: (lines, columns),
            dimensions: (30, 50),
        }
    }

    /// Start building a help popup using a fluent builder.
    pub fn help() -> AppHelpBuilder {
        AppHelpBuilder::default()
    }

    /// Create a code-review-trailers popup from the current patchset details
    /// state. The preview index selects which patch's trailers are shown.
    pub fn review_trailers(details: &PatchsetDetailsState) -> Self {
        let i = details.preview_index;
        let mut columns: usize = 0;

        let mut format_section =
            |authors: &std::collections::HashSet<crate::lore::domain::patch::Author>| -> String {
                let mut text = String::new();
                for author in authors {
                    let line = format!(" - {author}\n");
                    if line.len() > columns {
                        columns = line.len();
                    }
                    text.push_str(&line);
                }
                text
            };

        let reviewed_by = format_section(&details.reviewed_by[i]);
        let tested_by = format_section(&details.tested_by[i]);
        let acked_by = format_section(&details.acked_by[i]);

        let lines = (3
            + details.reviewed_by[i].len()
            + details.tested_by[i].len()
            + details.acked_by[i].len()) as u16;

        AppPopup::ReviewTrailers {
            reviewed_by,
            tested_by,
            acked_by,
            scroll: (0, 0),
            max_scroll: (lines, columns as u16),
            dimensions: (50, 40),
        }
    }

    // -----------------------------------------------------------------------
    // Input handling
    // -----------------------------------------------------------------------

    /// Advance scroll position in response to a navigation input.
    ///
    /// All popup variants share identical two-axis scroll semantics.
    pub fn handle_scroll(&mut self, input: InputEvent) {
        let (scroll, max_scroll) = match self {
            AppPopup::Info {
                scroll, max_scroll, ..
            }
            | AppPopup::Help {
                scroll, max_scroll, ..
            }
            | AppPopup::ReviewTrailers {
                scroll, max_scroll, ..
            } => (scroll, max_scroll),
        };

        match input {
            InputEvent::NavigateUp => {
                scroll.0 = scroll.0.saturating_sub(1);
            }
            InputEvent::NavigateDown => {
                if scroll.0 < max_scroll.0 {
                    scroll.0 += 1;
                }
            }
            InputEvent::NavigateLeft => {
                if scroll.1 > 0 {
                    scroll.1 -= 1;
                }
            }
            InputEvent::NavigateRight => {
                if scroll.1 < max_scroll.1 {
                    scroll.1 += 1;
                }
            }
            _ => {}
        }
    }

    /// `(width_percent, height_percent)` used for the centred popup rect.
    #[allow(dead_code)]
    pub fn dimensions(&self) -> (u16, u16) {
        match self {
            AppPopup::Info { dimensions, .. }
            | AppPopup::Help { dimensions, .. }
            | AppPopup::ReviewTrailers { dimensions, .. } => *dimensions,
        }
    }
}

// ---------------------------------------------------------------------------
// Help builder
// ---------------------------------------------------------------------------

/// Fluent builder for `AppPopup::Help`.
///
/// Mirrors the API of the old `HelpPopUpBuilder` so handler call sites change
/// minimally.
#[derive(Default)]
pub struct AppHelpBuilder {
    title: Option<String>,
    description: Option<String>,
    keybinds: Vec<(String, String)>,
}

impl AppHelpBuilder {
    pub fn title(mut self, t: &str) -> Self {
        self.title = Some(t.to_string());
        self
    }

    pub fn description(mut self, d: &str) -> Self {
        self.description = Some(d.to_string());
        self
    }

    pub fn keybind(mut self, key: impl Into<String>, help: impl Into<String>) -> Self {
        self.keybinds.push((key.into(), help.into()));
        self
    }

    pub fn build(self) -> AppPopup {
        let key_len = self
            .keybinds
            .iter()
            .fold(0usize, |acc, (k, _)| acc.max(k.len()));

        let formatted_keybinds = self.keybinds.iter().fold(String::new(), |acc, (k, v)| {
            acc + &format!("{k:>key_len$}: {v}\n")
        });

        let lines = self.keybinds.len() as u16;
        let columns = self
            .keybinds
            .iter()
            .fold(0u16, |acc, (k, v)| acc.max((k.len() + v.len()) as u16));

        AppPopup::Help {
            title: self.title,
            description: self.description,
            formatted_keybinds,
            scroll: (0, 0),
            max_scroll: (lines, columns),
            dimensions: (50, 50),
        }
    }
}
