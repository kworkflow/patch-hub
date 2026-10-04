use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::{app::models::view_model::EditConfigViewModel, ui::scene::EditConfigScene};

pub struct EditConfigPainter;

impl EditConfigPainter {
    pub fn build_scene(vm: &EditConfigViewModel) -> EditConfigScene {
        EditConfigScene {
            entries: vm.entries.clone(),
        }
    }

    pub fn paint(f: &mut Frame, scene: &EditConfigScene, chunk: Rect) {
        let mut constraints = Vec::new();

        for _ in 0..(chunk.height / 3) {
            constraints.push(Constraint::Length(3));
        }

        let config_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(chunk);

        for (i, entry) in scene.entries.iter().enumerate() {
            if i + 1 > config_chunks.len() {
                break;
            }

            let value = Line::from(if entry.is_editing {
                vec![
                    Span::styled(entry.edit_cursor_value.clone(), Style::default()),
                    Span::styled(" ", Style::default().bg(Color::White)),
                ]
            } else {
                vec![Span::from(entry.value.clone())]
            });

            let config_entry = Paragraph::new(value)
                .centered()
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(entry.label.clone()),
                )
                .style(if entry.is_editing {
                    Style::default()
                        .fg(Color::LightYellow)
                        .add_modifier(Modifier::BOLD)
                } else if entry.is_highlighted {
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                });

            f.render_widget(config_entry, config_chunks[i]);
        }
    }

    pub fn build_mode_spans(vm: &EditConfigViewModel) -> Vec<Span<'static>> {
        vec![if vm.is_editing_mode {
            Span::styled("Editing...", Style::default().fg(Color::LightYellow))
        } else {
            Span::styled("Edit Configurations", Style::default().fg(Color::Green))
        }]
    }

    pub fn build_keys_hint_span(vm: &EditConfigViewModel) -> Span<'static> {
        if vm.editing_tree_selector {
            Span::styled(
                "(←/→) cycle | (ENTER) confirm | (ESC) cancel",
                Style::default().fg(Color::Red),
            )
        } else if vm.is_editing_mode {
            Span::styled(
                "(ESC) cancel | (ENTER) confirm",
                Style::default().fg(Color::Red),
            )
        } else {
            Span::styled(
                "(ESC / q) save and exit | (ENTER) edit | (jk| 🡇 🡅 ) down up",
                Style::default().fg(Color::Red),
            )
        }
    }
}

#[cfg(test)]
mod tests {

    mod helpers {

        use crate::app::models::view_model::EditConfigViewModel;

        pub(super) fn vm(
            is_editing_mode: bool,
            editing_tree_selector: bool,
        ) -> EditConfigViewModel {
            EditConfigViewModel {
                entries: vec![],
                is_editing_mode,
                editing_tree_selector,
            }
        }
    }
    use super::*;
    use helpers::*;

    #[test]
    fn keys_hint_shows_cycle_bindings_on_the_tree_row() {
        let hint = EditConfigPainter::build_keys_hint_span(&vm(true, true));
        assert!(hint.content.contains("(←/→) cycle"));
        assert!(hint.content.contains("(ENTER) confirm"));
    }

    #[test]
    fn keys_hint_keeps_text_edit_bindings_on_other_rows() {
        let hint = EditConfigPainter::build_keys_hint_span(&vm(true, false));
        assert!(hint.content.contains("(ESC) cancel"));
        assert!(!hint.content.contains("cycle"));
    }

    #[test]
    fn keys_hint_says_save_and_exit_when_browsing() {
        let hint = EditConfigPainter::build_keys_hint_span(&vm(false, false));
        assert!(hint.content.contains("(ESC / q) save and exit"));
    }
}
