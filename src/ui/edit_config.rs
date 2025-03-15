use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::app::App;

pub fn render_main(f: &mut Frame, app: &App, chunk: Rect) {
    let edit_config = app.edit_config.as_ref().unwrap();
    let mut constraints = Vec::new();

    for _ in 0..(chunk.height / 3) {
        constraints.push(Constraint::Length(3));
    }

    let config_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(chunk);

    let highlighted_entry = edit_config.highlighted();
    for i in 0..edit_config.config_count() {
        if i + 1 > config_chunks.len() {
            break;
        }

        let (config, value) = edit_config.config(i);
        let value = Line::from(if edit_config.is_editing() && i == highlighted_entry {
            vec![
                Span::styled(edit_config.curr_edit().to_string(), Style::default()),
                Span::styled(" ", Style::default().bg(Color::White)),
            ]
        } else {
            vec![Span::from(value)]
        });

        let config_entry = Paragraph::new(value)
            .centered()
            .block(Block::default().borders(Borders::ALL).title(config))
            .style(if i == highlighted_entry && edit_config.is_editing() {
                Style::default()
                    .fg(Color::LightYellow)
                    .add_modifier(Modifier::BOLD)
            } else if i == highlighted_entry {
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            });

        f.render_widget(config_entry, config_chunks[i]);
    }
}

pub fn mode_footer_text(app: &App) -> Vec<Span> {
    let edit_config_state = app.edit_config.as_ref().unwrap();
    vec![if edit_config_state.is_editing() {
        Span::styled("Editing...", Style::default().fg(Color::LightYellow))
    } else {
        Span::styled("Edit Configurations", Style::default().fg(Color::Green))
    }]
}

pub fn keys_hint(app: &App) -> Span {
    let edit_config_state = app.edit_config.as_ref().unwrap();
    match edit_config_state.is_editing() {
        true => Span::styled(
            "(ESC) cancel | (ENTER) confirm",
            Style::default().fg(Color::Red),
        ),
        false => Span::styled(
            "(ESC / q) exit | (ENTER) edit | (jk| 🡇 🡅 ) down up",
            Style::default().fg(Color::Red),
        ),
    }
}
