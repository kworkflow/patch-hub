use std::cmp::Ordering;

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, ToSpan},
    widgets::{Block, Borders, Padding, Paragraph, Wrap},
    Frame,
};

use crate::app::{screens::details_actions::PatchsetAction, App};

/// Get list of indexes representing the patches that were replied w/ the
/// "Reviewed-by:" tag, or that are staged to.
///
/// # Arguments
///
/// * `app`: Immutable reference to `patch-hub` model
///
/// # Returns
///
/// An owned `Vec<Span>` that is an ordered comma-separated list of the indexes.
/// Staged indexes are appended w/ `*` and colored `Color::DarkGrey`. If there
/// are no patches that were neither replied or staged w/ the tag, the return
/// string will be empty.  
fn reviewed_by_list(app: &App) -> Vec<Span> {
    let patchset_details_and_actions = app.patchset_details_and_actions_state.as_ref().unwrap();

    let number_offset = if patchset_details_and_actions.has_cover_letter {
        0
    } else {
        1
    };

    let mut replied_inds: Vec<usize> = Vec::new();
    if let Some(already_reviewed_by) = app.reviewed_patchsets.get(
        &patchset_details_and_actions
            .representative_patch
            .message_id()
            .href,
    ) {
        replied_inds = already_reviewed_by
            .iter()
            .cloned()
            .map(|i| i + number_offset)
            .collect();
        replied_inds.sort_unstable();
    }

    let mut staged_inds: Vec<usize> = Vec::new();
    if let Some(true) = patchset_details_and_actions
        .patchset_actions
        .get(&PatchsetAction::ReplyWithReviewedBy)
    {
        staged_inds = patchset_details_and_actions
            .patches_to_reply
            .iter()
            .enumerate()
            .filter_map(|(i, &val)| if val { Some(i + number_offset) } else { None })
            .collect();
        staged_inds.sort_unstable();
    }

    let reviewed_by_inds = merge_reviewed_by_inds(replied_inds, staged_inds);

    let mut reviewed_by_list = Vec::new();
    // let mut reviewed_by_list = String::new();

    for (index, is_replied) in reviewed_by_inds {
        let mut entry = index.to_string();
        let mut entry_color = Style::default().fg(Color::White);
        if !is_replied {
            entry.push('*');
            entry_color = entry_color.fg(Color::DarkGray);
        }
        reviewed_by_list.push(Span::styled(entry, entry_color));
        reviewed_by_list.push(Span::styled(
            ", ".to_string(),
            Style::default().fg(Color::White),
        ));
    }
    reviewed_by_list.pop();

    reviewed_by_list
}

fn render_details_and_actions(f: &mut Frame, app: &App, details_chunk: Rect, actions_chunk: Rect) {
    let patchset_details_and_actions = app.patchset_details_and_actions_state.as_ref().unwrap();

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
    ];
    let mut reviewed_by = reviewed_by_list(app);
    if !reviewed_by.is_empty() {
        let mut reviewed_by_line = vec![
            Span::styled("Reviewed-by: ", Style::default().fg(Color::Cyan)),
            '('.to_span(),
        ];
        reviewed_by_line.append(&mut reviewed_by);
        reviewed_by_line.push(')'.to_span());
        patchset_details.push(Line::from(reviewed_by_line));
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
            Span::styled("eviewed-by ", Style::default().fg(Color::Cyan)),
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
    let patchset_details_and_actions = app.patchset_details_and_actions_state.as_ref().unwrap();

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
    let patchset_details_and_actions = app.patchset_details_and_actions_state.as_ref().unwrap();

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
        "(ESC) to return | (ENTER) run actions | (jkhl / 🡇 🡅 🡄 🡆 ) | (n) next patch | (p) previous patch | (f) fullscreen",
        Style::default().fg(Color::Red),
    )
}

/// Receives two `usize` slices representing the patch indexes tagged w/
/// "Reviewed-by", preceding those that were replied over those that are only
/// staged.
///
/// # Arguments
///
/// * `replied_inds`: Ordered slice of patch indexes that were replied
/// * `staged_inds`: Ordered slice of patch indexes that staged to be replied
///
/// # Returns
///
/// This function always returns a `Vec<(usize, bool)>` (empty or not). The
/// tuples `(index, is_replied)` encode the precedence, in which `is_replied ==
/// true` means that patch of number `index` was replied w/ the tag.
fn merge_reviewed_by_inds(
    mut replied_inds: Vec<usize>,
    mut staged_inds: Vec<usize>,
) -> Vec<(usize, bool)> {
    let mut reviewed_by_inds = Vec::new();
    let mut i = 0;
    let mut j = 0;

    // Don't assume the caller ordered the vectors beforehand
    replied_inds.sort();
    staged_inds.sort();

    while (i != replied_inds.len()) && (j != staged_inds.len()) {
        match replied_inds[i].cmp(&staged_inds[j]) {
            Ordering::Less => {
                reviewed_by_inds.push((replied_inds[i], true));
                i += 1;
            }
            Ordering::Equal => {
                reviewed_by_inds.push((replied_inds[i], true));
                i += 1;
                j += 1;
            }
            Ordering::Greater => {
                reviewed_by_inds.push((staged_inds[j], false));
                j += 1;
            }
        }
    }

    let mut remaining_inds: Vec<(usize, bool)>;
    if i != replied_inds.len() {
        remaining_inds = replied_inds[i..]
            .iter()
            .map(|&index| (index, true))
            .collect();
    } else {
        remaining_inds = staged_inds[j..]
            .iter()
            .map(|&index| (index, false))
            .collect();
    }
    reviewed_by_inds.append(&mut remaining_inds);

    reviewed_by_inds
}

#[cfg(test)]
mod tests {
    use super::merge_reviewed_by_inds;

    #[test]
    fn test_merge_reviewed_by_inds() {
        let replied_inds = vec![];
        let staged_inds = vec![];
        let reviewed_by_inds = merge_reviewed_by_inds(replied_inds, staged_inds);
        assert_eq!(
            reviewed_by_inds.len(),
            0,
            "No replied neither staged patches should result in empty reviewed-by"
        );

        let replied_inds = vec![0, 1, 2, 3, 4];
        let staged_inds = vec![];
        let reviewed_by_inds = merge_reviewed_by_inds(replied_inds.clone(), staged_inds);
        assert_eq!(
            reviewed_by_inds.len(),
            5,
            "No staged patches should result in reviewed-by w/ same size as replied"
        );
        assert_eq!(
            reviewed_by_inds
                .clone()
                .into_iter()
                .map(|(i, _)| i)
                .collect::<Vec<usize>>(),
            replied_inds,
            "No staged patches should result in reviewed-by equal to replied"
        );
        assert!(
            reviewed_by_inds.iter().all(|(_, b)| *b),
            "No staged patches should result in reviewed-by equal to replied"
        );

        let replied_inds = vec![];
        let staged_inds = vec![1, 3, 10];
        let reviewed_by_inds = merge_reviewed_by_inds(replied_inds, staged_inds.clone());
        assert_eq!(
            reviewed_by_inds.len(),
            3,
            "No staged patches should result in reviewed-by w/ same size as staged"
        );
        assert_eq!(
            reviewed_by_inds
                .clone()
                .into_iter()
                .map(|(i, _)| i)
                .collect::<Vec<usize>>(),
            staged_inds,
            "No staged patches should result in reviewed-by equal to staged"
        );
        assert!(
            reviewed_by_inds.iter().all(|(_, b)| !*b),
            "No staged patches should result in reviewed-by equal to staged"
        );

        let replied_inds = vec![0, 1, 2, 3, 4];
        let staged_inds = vec![1, 3, 10];
        let expected_reviewed_by_inds = vec![
            (0, true),
            (1, true),
            (2, true),
            (3, true),
            (4, true),
            (10, false),
        ];
        let reviewed_by_inds = merge_reviewed_by_inds(replied_inds, staged_inds);
        assert_eq!(
            reviewed_by_inds.len(),
            6,
            "No staged patches should result in reviewed-by w/ same size as staged"
        );
        assert_eq!(reviewed_by_inds, expected_reviewed_by_inds,);
    }
}
