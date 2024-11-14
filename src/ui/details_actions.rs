use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Padding, Paragraph, Wrap},
    Frame,
};

use crate::app::{screens::details_actions::PatchsetAction, App};

fn render_details_and_actions(f: &mut Frame, app: &App, details_chunk: Rect, actions_chunk: Rect) {
    let patchset_details_and_actions = app.details_actions.as_ref().unwrap();

    let patchset_details = &patchset_details_and_actions.representative_patch;
    let patchset_details = vec![
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
    ];

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
            if *patchset_actions
                .get(&PatchsetAction::ReplyWithReviewedBy)
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
    if let Some(successful_indexes) = app.reviewed_patchsets.get(representative_patch_message_id) {
        if successful_indexes.contains(&preview_index) {
            preview_title = " Preview [REVIEWED] ".to_string();
        }
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
        "(ESC) to return | (ENTER) run actions | (?) help",
        Style::default().fg(Color::Red),
    )
}
