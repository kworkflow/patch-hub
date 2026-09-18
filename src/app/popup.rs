//! Application-layer popup state.
//!
//! `AppPopup` replaces the old `Box<dyn PopUp>` trait object in `AppState`.
//! Each variant owns all data required to present the popup and tracks scroll
//! position so the presentation layer receives a read-only snapshot.

use std::collections::HashSet;

use crate::{
    app::screens::details_actions::PatchsetDetailsState, input::event::InputEvent,
    lore::domain::patch::Author,
};

/// Action taken when the user confirms a choice popup.
///
/// Step 7's boot-once gate will add its own variant; do not reuse
/// [`ConfirmAction::Wait`] or [`ConfirmAction::CancelKwAndQuit`] for that.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfirmAction {
    CancelKwAndQuit,
    Wait,
}

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
    Confirm {
        title: String,
        body: String,
        options: Vec<(String, ConfirmAction)>,
        selected: usize,
        dimensions: (u16, u16),
    },
}

impl AppPopup {
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

        let mut format_section = |authors: &HashSet<Author>| -> String {
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

    /// Choice popup shown when the user tries to quit while a kw job runs.
    ///
    /// [`ConfirmAction::Wait`] is the highlighted default so Enter, like
    /// Esc, stays in the app unless the user explicitly picks cancel.
    pub fn quit_while_job_running() -> Self {
        AppPopup::Confirm {
            title: "Cancel build and quit?".to_string(),
            body: "A kw job is still running. Cancel it and quit, or wait and stay in the app?"
                .to_string(),
            options: vec![
                (
                    "Cancel and quit".to_string(),
                    ConfirmAction::CancelKwAndQuit,
                ),
                ("Wait".to_string(), ConfirmAction::Wait),
            ],
            selected: 1,
            dimensions: (50, 30),
        }
    }

    /// The currently highlighted confirm action, if this is a choice popup.
    pub fn selected_confirm_action(&self) -> Option<ConfirmAction> {
        match self {
            AppPopup::Confirm {
                options, selected, ..
            } => options.get(*selected).map(|(_, action)| *action),
            _ => None,
        }
    }

    /// Handle input for whichever popup is open.
    ///
    /// Info/Help/ReviewTrailers scroll. Confirm moves the highlighted
    /// option; Enter is handled by the caller via [`InputEvent::ConfirmPopup`].
    pub fn handle_input(&mut self, input: InputEvent) {
        match self {
            AppPopup::Confirm {
                options, selected, ..
            } => match input {
                InputEvent::NavigateLeft | InputEvent::NavigateUp => {
                    *selected = selected.saturating_sub(1);
                }
                InputEvent::NavigateRight | InputEvent::NavigateDown => {
                    if *selected + 1 < options.len() {
                        *selected += 1;
                    }
                }
                _ => {}
            },
            _ => self.handle_scroll(input),
        }
    }

    /// Advance scroll position in response to a navigation input.
    ///
    /// Scrollable popup variants share identical two-axis scroll semantics.
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
            AppPopup::Confirm { .. } => return,
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
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quit_confirm_defaults_to_wait() {
        let popup = AppPopup::quit_while_job_running();
        assert_eq!(Some(ConfirmAction::Wait), popup.selected_confirm_action());
    }

    #[test]
    fn quit_confirm_left_selects_cancel_and_quit() {
        let mut popup = AppPopup::quit_while_job_running();
        popup.handle_input(InputEvent::NavigateLeft);
        assert_eq!(
            Some(ConfirmAction::CancelKwAndQuit),
            popup.selected_confirm_action()
        );
        popup.handle_input(InputEvent::NavigateLeft);
        assert_eq!(
            Some(ConfirmAction::CancelKwAndQuit),
            popup.selected_confirm_action()
        );
    }

    #[test]
    fn quit_confirm_right_stays_on_wait() {
        let mut popup = AppPopup::quit_while_job_running();
        popup.handle_input(InputEvent::NavigateRight);
        assert_eq!(Some(ConfirmAction::Wait), popup.selected_confirm_action());
    }

    #[test]
    fn info_popup_still_scrolls() {
        let mut popup = AppPopup::info("Title", "line 1\nline 2\nline 3");
        popup.handle_input(InputEvent::NavigateDown);
        let AppPopup::Info { scroll, .. } = popup else {
            panic!("expected Info popup");
        };
        assert_eq!((1, 0), scroll);
    }
}
