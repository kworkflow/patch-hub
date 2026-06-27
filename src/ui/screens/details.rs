use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Padding, Paragraph, Wrap},
    Frame,
};

use crate::{
    app::view_model::PatchsetDetailsViewModel,
    ui::scene::{PatchsetDetailsScene, TagTrailerCounts},
};

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

pub fn build_scene(vm: &PatchsetDetailsViewModel) -> PatchsetDetailsScene {
    PatchsetDetailsScene {
        patch_title: vm.patch_title.clone(),
        author_name: vm.author_name.clone(),
        version: vm.version,
        patch_count: vm.patch_count,
        last_updated: vm.last_updated.clone(),
        tag_trailer_counts: vm.tag_trailer_counts.clone(),
        staged_to_reply: vm.staged_to_reply.clone(),
        preview_entries: vm.preview_entries.clone(),
        preview_index: vm.preview_index,
        preview_scroll_offset: vm.preview_scroll_offset,
        preview_pan: vm.preview_pan,
        preview_fullscreen: vm.preview_fullscreen,
        preview_title: vm.preview_title.clone(),
        is_bookmarked: vm.is_bookmarked,
        is_apply_staged: vm.is_apply_staged,
        is_current_patch_reply_staged: vm.is_current_patch_reply_staged,
    }
}

// ---------------------------------------------------------------------------
// Painter
// ---------------------------------------------------------------------------

pub fn paint(f: &mut Frame, scene: &PatchsetDetailsScene, chunk: Rect) {
    if scene.preview_fullscreen {
        paint_preview(f, scene, chunk);
    } else {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
            .split(chunk);

        let details_and_actions_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(chunks[0]);

        paint_details_and_actions(
            f,
            scene,
            details_and_actions_chunks[0],
            details_and_actions_chunks[1],
        );
        paint_preview(f, scene, chunks[1]);
    }
}

fn review_trailers_line(counts: &TagTrailerCounts) -> Line<'static> {
    let resolve_color = |n: usize| -> Style {
        if n == 0 {
            Style::default().fg(Color::White)
        } else {
            Style::default().fg(Color::Green)
        }
    };

    Line::from(vec![
        Span::styled("Reviewed-by: ", Style::default().fg(Color::Cyan)),
        Span::styled(
            counts.reviewed_by.to_string(),
            resolve_color(counts.reviewed_by),
        ),
        Span::styled(" | Tested-by: ", Style::default().fg(Color::Cyan)),
        Span::styled(
            counts.tested_by.to_string(),
            resolve_color(counts.tested_by),
        ),
        Span::styled(" | Acked-by: ", Style::default().fg(Color::Cyan)),
        Span::styled(counts.acked_by.to_string(), resolve_color(counts.acked_by)),
    ])
}

fn paint_details_and_actions(
    f: &mut Frame,
    scene: &PatchsetDetailsScene,
    details_chunk: Rect,
    actions_chunk: Rect,
) {
    let mut patchset_details = vec![
        Line::from(vec![
            Span::styled(r#"  Title: "#, Style::default().fg(Color::Cyan)),
            Span::styled(scene.patch_title.clone(), Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("Author: ", Style::default().fg(Color::Cyan)),
            Span::styled(scene.author_name.clone(), Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("Version: ", Style::default().fg(Color::Cyan)),
            Span::styled(
                format!("{}", scene.version),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled("Patch count: ", Style::default().fg(Color::Cyan)),
            Span::styled(
                format!("{}", scene.patch_count),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled("Last updated: ", Style::default().fg(Color::Cyan)),
            Span::styled(
                scene.last_updated.clone(),
                Style::default().fg(Color::White),
            ),
        ]),
        review_trailers_line(&scene.tag_trailer_counts),
    ];

    if let Some(staged) = &scene.staged_to_reply {
        patchset_details.push(Line::from(vec![
            Span::styled("Staged to reply: ", Style::default().fg(Color::Cyan)),
            Span::styled(staged.clone(), Style::default().fg(Color::White)),
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

    // TODO: Create a function to produce new action lines
    let patchset_actions = vec![
        Line::from(vec![
            if scene.is_bookmarked {
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
            if scene.is_apply_staged {
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
            if scene.is_current_patch_reply_staged {
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

fn paint_preview(f: &mut Frame, scene: &PatchsetDetailsScene, chunk: Rect) {
    let patch_preview = scene.preview_entries[scene.preview_index].clone();

    let patch_preview = Paragraph::new(patch_preview)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(ratatui::widgets::BorderType::Double)
                .title(
                    Line::styled(
                        scene.preview_title.clone(),
                        Style::default().fg(Color::Green),
                    )
                    .left_aligned(),
                )
                .padding(Padding::vertical(1)),
        )
        .left_aligned()
        .scroll((scene.preview_scroll_offset as u16, scene.preview_pan as u16));

    f.render_widget(patch_preview, chunk);
}

// ---------------------------------------------------------------------------
// Navigation-bar helpers
// ---------------------------------------------------------------------------

pub fn mode_spans() -> Vec<Span<'static>> {
    vec![Span::styled(
        "Patchset Details and Actions",
        Style::default().fg(Color::Green),
    )]
}

pub fn keys_hint_span() -> Span<'static> {
    Span::styled(
        "(ESC / q) to return | (ENTER) run actions | (?) help",
        Style::default().fg(Color::Red),
    )
}
