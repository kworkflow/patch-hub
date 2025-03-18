use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Padding, Paragraph, Wrap},
    Frame,
};

use crate::app::{
    screens::details_actions::{DetailsActions, PatchsetAction},
    App,
};

/// Returns a `Line` type that represents a line containing stats about reply
/// trailers. It currently considers the _Reviewed-by_, _Tested-by_, and
/// _Acked-by_ trailers and colors them depending if they are 0 or not.  Example
/// of line returned:
///
/// _**Reviewed-by: 1 | Tested-by: 0 | Acked-by: 2**_
fn review_trailers_details(details_actions: &DetailsActions) -> Line<'static> {
    let i = details_actions.preview_index;

    let resolve_color = |n_trailers: usize| -> Style {
        if n_trailers == 0 {
            Style::default().fg(Color::White)
        } else {
            Style::default().fg(Color::Green)
        }
    };

    Line::from(vec![
        Span::styled("Reviewed-by: ", Style::default().fg(Color::Cyan)),
        Span::styled(
            details_actions.reviewed_by[i].len().to_string(),
            resolve_color(details_actions.reviewed_by[i].len()),
        ),
        Span::styled(" | Tested-by: ", Style::default().fg(Color::Cyan)),
        Span::styled(
            details_actions.tested_by[i].len().to_string(),
            resolve_color(details_actions.tested_by[i].len()),
        ),
        Span::styled(" | Acked-by: ", Style::default().fg(Color::Cyan)),
        Span::styled(
            details_actions.acked_by[i].len().to_string(),
            resolve_color(details_actions.acked_by[i].len()),
        ),
    ])
}

fn render_details_and_actions(f: &mut Frame, app: &App, details_chunk: Rect, actions_chunk: Rect) {
    let patchset_details_and_actions = app.details_actions.as_ref().unwrap();

    let mut staged_to_reply = String::new();
    if let Some(true) = patchset_details_and_actions
        .patchset_actions
        .get(&PatchsetAction::ReplyWithReviewedBy)
    {
        staged_to_reply.push('(');
        let number_offset = if patchset_details_and_actions.has_cover_letter {
            0
        } else {
            1
        };
        let patches_to_reply_numbers: Vec<usize> = patchset_details_and_actions
            .patches_to_reply
            .iter()
            .enumerate()
            .filter_map(|(i, &val)| if val { Some(i + number_offset) } else { None })
            .collect();
        for number in patches_to_reply_numbers {
            staged_to_reply.push_str(&format!("{number}, "));
        }
        staged_to_reply.pop();
        staged_to_reply = format!("{})", &staged_to_reply[..staged_to_reply.len() - 1]);
    }

    let patchset_details = &patchset_details_and_actions.representative_patch;
    let mut patchset_details = vec![
        Line::from(vec![
            Span::styled(r#"  Title: "#, Style::default().fg(Color::Cyan)),
            Span::styled(
                patchset_details.title().to_string(),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled("Author: ", Style::default().fg(Color::Cyan)),
            Span::styled(
                patchset_details.author().name.to_string(),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled("Version: ", Style::default().fg(Color::Cyan)),
            Span::styled(
                format!("{}", patchset_details.version()),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled("Patch count: ", Style::default().fg(Color::Cyan)),
            Span::styled(
                format!("{}", patchset_details.total_in_series()),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled("Last updated: ", Style::default().fg(Color::Cyan)),
            Span::styled(
                patchset_details.updated().to_string(),
                Style::default().fg(Color::White),
            ),
        ]),
        review_trailers_details(patchset_details_and_actions),
    ];
    if !staged_to_reply.is_empty() {
        patchset_details.push(Line::from(vec![
            Span::styled("Staged to reply: ", Style::default().fg(Color::Cyan)),
            Span::styled(staged_to_reply, Style::default().fg(Color::White)),
        ]));
    }

    let patchset_details = Paragraph::new(patchset_details)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(ratatui::widgets::BorderType::Double)
                .title(Line::styled(" Details ", Style::default().fg(Color::Green)).left_aligned())
                .padding(Padding::vertical(1)),
        )
        .left_aligned()
        .wrap(Wrap { trim: true });

    f.render_widget(patchset_details, details_chunk);

    let patchset_actions = &patchset_details_and_actions.patchset_actions;
    // TODO: Create a function to produce new action lines
    let patchset_actions = vec![
        Line::from(vec![
            if *patchset_actions.get(&PatchsetAction::Bookmark).unwrap() {
                Span::styled("[x] ", Style::default().fg(Color::Green))
            } else {
                Span::styled("[ ] ", Style::default().fg(Color::Cyan))
            },
            Span::styled(
                "b",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::UNDERLINED)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("ookmark", Style::default().fg(Color::Cyan)),
        ]),
        Line::from(vec![
            if *patchset_actions.get(&PatchsetAction::Apply).unwrap() {
                Span::styled("[x] ", Style::default().fg(Color::Green))
            } else {
                Span::styled("[ ] ", Style::default().fg(Color::Cyan))
            },
            Span::styled(
                "a",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::UNDERLINED)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("pply", Style::default().fg(Color::Cyan)),
        ]),
        Line::from(vec![
            if *patchset_details_and_actions
                .patches_to_reply
                .get(patchset_details_and_actions.preview_index)
                .unwrap()
            {
                Span::styled("[x] ", Style::default().fg(Color::Green))
            } else {
                Span::styled("[ ] ", Style::default().fg(Color::Cyan))
            },
            Span::styled(
                "r",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::UNDERLINED)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("eviewed-by", Style::default().fg(Color::Cyan)),
        ]),
    ];
    let patchset_actions = Paragraph::new(patchset_actions)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(ratatui::widgets::BorderType::Double)
                .title(Line::styled(" Actions ", Style::default().fg(Color::Green)).left_aligned())
                .padding(Padding::vertical(1)),
        )
        .centered();

    f.render_widget(patchset_actions, actions_chunk);
}

fn render_preview(f: &mut Frame, app: &App, chunk: Rect) {
    let patchset_details_and_actions = app.details_actions.as_ref().unwrap();

    let preview_index = patchset_details_and_actions.preview_index;

    let representative_patch_message_id = &patchset_details_and_actions
        .representative_patch
        .message_id()
        .href;
    let mut preview_title = String::from(" Preview ");
    if matches!(
        app.reviewed_patchsets.get(representative_patch_message_id),
        Some(successful_indexes) if successful_indexes.contains(&preview_index)
    ) {
        preview_title = " Preview [REVIEWED-BY] ".to_string();
    } else if *patchset_details_and_actions
        .patches_to_reply
        .get(preview_index)
        .unwrap()
    {
        preview_title = " Preview [REVIEWED-BY]* ".to_string();
    };

    let preview_offset = patchset_details_and_actions.preview_scroll_offset;
    let preview_pan = patchset_details_and_actions.preview_pan;
    let patch_preview = patchset_details_and_actions.patches_preview[preview_index].clone();

    let patch_preview = Paragraph::new(patch_preview)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(ratatui::widgets::BorderType::Double)
                .title(
                    Line::styled(preview_title, Style::default().fg(Color::Green)).left_aligned(),
                )
                .padding(Padding::vertical(1)),
        )
        .left_aligned()
        .scroll((preview_offset as u16, preview_pan as u16));

    f.render_widget(patch_preview, chunk);
}

pub fn render_main(f: &mut Frame, app: &App, chunk: Rect) {
    let patchset_details_and_actions = app.details_actions.as_ref().unwrap();

    if patchset_details_and_actions.preview_fullscreen {
        render_preview(f, app, chunk);
    } else {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
            .split(chunk);

        let details_and_actions_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(chunks[0]);

        render_details_and_actions(
            f,
            app,
            details_and_actions_chunks[0],
            details_and_actions_chunks[1],
        );
        render_preview(f, app, chunks[1]);
    }
}

pub fn mode_footer_text() -> Vec<Span<'static>> {
    vec![Span::styled(
        "Patchset Details and Actions",
        Style::default().fg(Color::Green),
    )]
}

pub fn keys_hint() -> Span<'static> {
    Span::styled(
        "(ESC / q) to return | (ENTER) run actions | (?) help",
        Style::default().fg(Color::Red),
    )
}
