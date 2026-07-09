use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::Line,
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use crate::{
    app::view_model::{PopupViewBody, PopupViewModel},
    ui::scene::{PopupBody, PopupScene},
};
pub fn build_scene(vm: &PopupViewModel) -> PopupScene {
    let body = match &vm.body {
        PopupViewBody::Text(text) => PopupBody::Text(text.clone()),
        PopupViewBody::Keybinds {
            description,
            formatted_keybinds,
        } => PopupBody::Keybinds {
            description: description.clone(),
            formatted_keybinds: formatted_keybinds.clone(),
        },
        PopupViewBody::ReviewTrailers {
            reviewed_by,
            tested_by,
            acked_by,
        } => PopupBody::ReviewTrailers {
            reviewed_by: reviewed_by.clone(),
            tested_by: tested_by.clone(),
            acked_by: acked_by.clone(),
        },
    };

    PopupScene {
        title: vm.title.clone(),
        body,
        scroll_offset: vm.scroll_offset,
        dimensions: vm.dimensions,
    }
}

pub fn paint(f: &mut Frame, scene: &PopupScene, chunk: Rect) {
    match &scene.body {
        PopupBody::Text(body) => paint_info(f, &scene.title, body, scene.scroll_offset, chunk),
        PopupBody::Keybinds {
            description,
            formatted_keybinds,
        } => paint_help(
            f,
            &scene.title,
            description.as_deref(),
            formatted_keybinds,
            scene.scroll_offset,
            chunk,
        ),
        PopupBody::ReviewTrailers {
            reviewed_by,
            tested_by,
            acked_by,
        } => paint_review_trailers(
            f,
            reviewed_by,
            tested_by,
            acked_by,
            scene.scroll_offset,
            chunk,
        ),
    }
}

fn paint_info(f: &mut Frame, title: &str, body: &str, scroll: (u16, u16), chunk: Rect) {
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

fn paint_help(
    f: &mut Frame,
    title: &str,
    description: Option<&str>,
    formatted_keybinds: &str,
    scroll: (u16, u16),
    chunk: Rect,
) {
    let block = Block::default()
        .title(title.to_string())
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

fn paint_review_trailers(
    f: &mut Frame,
    reviewed_by: &str,
    tested_by: &str,
    acked_by: &str,
    scroll: (u16, u16),
    chunk: Rect,
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
