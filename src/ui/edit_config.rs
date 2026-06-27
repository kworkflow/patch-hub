use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::app::view_model::EditConfigViewModel;

pub fn render_main(f: &mut Frame, vm: &EditConfigViewModel, chunk: Rect) {
    let mut constraints = Vec::new();

    for _ in 0..(chunk.height / 3) {
        constraints.push(Constraint::Length(3));
    }

    let config_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(chunk);

    for (i, entry) in vm.entries.iter().enumerate() {
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

pub fn mode_footer_text(vm: &EditConfigViewModel) -> Vec<Span<'static>> {
    vec![if vm.is_editing_mode {
        Span::styled("Editing...", Style::default().fg(Color::LightYellow))
    } else {
        Span::styled("Edit Configurations", Style::default().fg(Color::Green))
    }]
}

pub fn keys_hint(vm: &EditConfigViewModel) -> Span<'static> {
    if vm.is_editing_mode {
        Span::styled(
            "(ESC) cancel | (ENTER) confirm",
            Style::default().fg(Color::Red),
        )
    } else {
        Span::styled(
            "(ESC / q) exit | (ENTER) edit | (jk| 🡇 🡅 ) down up",
            Style::default().fg(Color::Red),
        )
    }
}
