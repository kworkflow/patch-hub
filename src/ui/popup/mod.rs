//! Popup rendering for the UI layer.
//!
//! The old `PopUp` trait object is gone. Each popup variant is represented
//! in `AppState` as a concrete `AppPopup` enum; this module provides the
//! single `render_popup` function that paints any variant onto a Ratatui
//! frame. The per-variant rendering logic mirrors what the old concrete types
//! did, without the dynamic dispatch overhead.

use ratatui::{
    layout::Alignment,
    style::{Color, Modifier, Style, Stylize},
    text::Line,
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use crate::app::popup::AppPopup;

/// Paint `popup` centred inside `chunk`.
pub fn render_popup(f: &mut Frame, popup: &AppPopup, chunk: ratatui::layout::Rect) {
    match popup {
        AppPopup::Info {
            title,
            body,
            scroll,
            ..
        } => render_info(f, title, body, *scroll, chunk),
        AppPopup::Help {
            title,
            description,
            formatted_keybinds,
            scroll,
            ..
        } => render_help(
            f,
            title.as_deref(),
            description.as_deref(),
            formatted_keybinds,
            *scroll,
            chunk,
        ),
        AppPopup::ReviewTrailers {
            reviewed_by,
            tested_by,
            acked_by,
            scroll,
            ..
        } => render_review_trailers(f, reviewed_by, tested_by, acked_by, *scroll, chunk),
    }
}

// ---------------------------------------------------------------------------
// Info popup
// ---------------------------------------------------------------------------

fn render_info(
    f: &mut Frame,
    title: &str,
    body: &str,
    scroll: (u16, u16),
    chunk: ratatui::layout::Rect,
) {
    let bold_blue = Style::default()
        .add_modifier(Modifier::BOLD)
        .fg(Color::Blue);
    let block = Block::default()
        .title(title.to_string())
        .title_alignment(Alignment::Center)
        .title_style(bold_blue)
        .title_bottom(Line::styled("(ESC / q) Close", bold_blue))
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .style(Style::default());

    let paragraph = Paragraph::new(body.to_string())
        .block(block)
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: true })
        .scroll(scroll);

    f.render_widget(Clear, chunk);
    f.render_widget(paragraph, chunk);
}

// ---------------------------------------------------------------------------
// Help popup
// ---------------------------------------------------------------------------

fn render_help(
    f: &mut Frame,
    title: Option<&str>,
    description: Option<&str>,
    formatted_keybinds: &str,
    scroll: (u16, u16),
    chunk: ratatui::layout::Rect,
) {
    let title_str = title.unwrap_or("Help").to_string();

    let block = Block::default()
        .title(title_str)
        .title_alignment(Alignment::Center)
        .title_style(Style::default().bold().blue())
        .title_bottom(Line::styled(
            "(ESC / q) Close",
            Style::default().bold().blue(),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .style(Style::default());

    let mut text = description.map_or_else(String::new, |d| format!("{d}\n\n"));
    if !formatted_keybinds.is_empty() {
        text.push_str(" \u{1F836} Keybinds\n");
        text.push_str(formatted_keybinds);
    }

    let paragraph = Paragraph::new(text)
        .style(Style::default())
        .block(block)
        .alignment(Alignment::Left)
        .scroll(scroll);

    f.render_widget(Clear, chunk);
    f.render_widget(paragraph, chunk);
}

// ---------------------------------------------------------------------------
// Review-trailers popup
// ---------------------------------------------------------------------------

fn render_review_trailers(
    f: &mut Frame,
    reviewed_by: &str,
    tested_by: &str,
    acked_by: &str,
    scroll: (u16, u16),
    chunk: ratatui::layout::Rect,
) {
    let header_style = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD)
        .add_modifier(Modifier::UNDERLINED);

    let mut contents: Vec<Line<'static>> = vec![];

    let mut push_section = |name: &str, text: &str| {
        contents.push(Line::styled(name.to_string(), header_style));
        for line in text.lines() {
            contents.push(Line::styled(
                line.to_string(),
                Style::default().fg(Color::White),
            ));
        }
        contents.push(Line::from(""));
    };

    push_section("Reviewed-by", reviewed_by);
    push_section("Tested-by", tested_by);
    push_section("Acked-by", acked_by);

    let block = Block::default()
        .title("Code-Review Trailers")
        .title_alignment(Alignment::Center)
        .title_style(Style::default().bold().blue())
        .title_bottom(Line::styled(
            "(ESC / q) Close",
            Style::default().bold().blue(),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .style(Style::default());

    let paragraph = Paragraph::new(contents)
        .style(Style::default())
        .block(block)
        .alignment(Alignment::Left)
        .scroll(scroll);

    f.render_widget(Clear, chunk);
    f.render_widget(paragraph, chunk);
}
