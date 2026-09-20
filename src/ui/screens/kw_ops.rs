use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use crate::{app::view_model::KwOpsViewModel, ui::scene::KwOpsScene};

pub fn build_scene(vm: &KwOpsViewModel) -> KwOpsScene {
    KwOpsScene {
        patchset_title: vm.patchset_title.clone(),
        message_id: vm.message_id.clone(),
        kernel_tree_id: vm.kernel_tree_id.clone(),
        tree_path: vm.tree_path.clone(),
        branch: vm.branch.clone(),
        extra_args: vm.extra_args.clone(),
        branch_focused: vm.branch_focused,
        extras_focused: vm.extras_focused,
        editing: vm.editing,
        kw_binary: vm.kw_binary.clone(),
        tree_readiness: vm.tree_readiness.clone(),
        output_dir: vm.output_dir.clone(),
        job_status: vm.job_status.clone(),
        command: vm.command.clone(),
        start_label: vm.start_label.clone(),
        cancel_label: vm.cancel_label.clone(),
        restore_label: vm.restore_label.clone(),
        deploy_placeholder: vm.deploy_placeholder.clone(),
        branch_guidance: vm.branch_guidance.clone(),
        log_tail: vm.log_tail.clone(),
    }
}

pub fn paint(f: &mut Frame, scene: &KwOpsScene, chunk: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
        .split(chunk);

    paint_form(f, scene, chunks[0]);
    paint_log(f, scene, chunks[1]);
}

fn paint_form(f: &mut Frame, scene: &KwOpsScene, chunk: Rect) {
    let mut lines = vec![
        labeled("Patchset", &scene.patchset_title),
        labeled("Message-ID", &scene.message_id),
        labeled(
            "Tree",
            &format!("{}  {}", scene.kernel_tree_id, scene.tree_path),
        ),
        field_line("Branch", &scene.branch, scene.branch_focused, scene.editing),
        field_line(
            "Extra args",
            &scene.extra_args,
            scene.extras_focused,
            scene.editing,
        ),
    ];
    if let Some(guidance) = &scene.branch_guidance {
        lines.push(Line::from(Span::styled(
            guidance.clone(),
            Style::default().fg(Color::Yellow),
        )));
    }
    lines.extend([
        Line::from(""),
        labeled("kw", &scene.kw_binary),
        labeled("Tree status", &scene.tree_readiness),
        labeled("Output dir", &scene.output_dir),
        labeled("Job", &scene.job_status),
        labeled("Command", &scene.command),
        labeled("Start", &scene.start_label),
        labeled("Cancel", &scene.cancel_label),
        labeled("Restore", &scene.restore_label),
        labeled("Deploy", &scene.deploy_placeholder),
    ]);

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Kw operations "),
        )
        .wrap(Wrap { trim: true });
    f.render_widget(paragraph, chunk);
}

fn paint_log(f: &mut Frame, scene: &KwOpsScene, chunk: Rect) {
    let inner_width = chunk.width.saturating_sub(2);
    let inner_height = chunk.height.saturating_sub(2);
    let offset = log_scroll_offset(&scene.log_tail, inner_width, inner_height);
    let paragraph = Paragraph::new(scene.log_tail.clone())
        .block(Block::default().borders(Borders::ALL).title(" Build log "))
        .wrap(Wrap { trim: false })
        .scroll((offset, 0));
    f.render_widget(paragraph, chunk);
}

/// Scroll so the newest wrapped rows sit at the bottom of the pane.
///
/// Uses ratatui's wrap-aware [`Paragraph::line_count`] so the offset
/// matches what the painter actually renders (word wrap, tabs, wide
/// chars). Counted without a [`Block`] and with the inner width, so
/// border rows are not mixed into the text height.
pub(crate) fn log_scroll_offset(text: &str, inner_width: u16, inner_height: u16) -> u16 {
    if inner_height == 0 || inner_width == 0 {
        return 0;
    }
    let rows = Paragraph::new(text)
        .wrap(Wrap { trim: false })
        .line_count(inner_width);
    rows.saturating_sub(inner_height as usize) as u16
}

fn labeled(label: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label}: "), Style::default().fg(Color::Cyan)),
        Span::styled(value.to_string(), Style::default().fg(Color::White)),
    ])
}

fn field_line(label: &str, value: &str, focused: bool, editing: bool) -> Line<'static> {
    let display = if value.is_empty() { " " } else { value };
    let mut style = Style::default().fg(Color::White);
    if focused {
        style = style.add_modifier(Modifier::BOLD);
        if editing {
            style = style.fg(Color::LightYellow);
        } else {
            style = style.fg(Color::Yellow);
        }
    }
    let mut spans = vec![
        Span::styled(format!("{label}: "), Style::default().fg(Color::Cyan)),
        Span::styled(display.to_string(), style),
    ];
    if focused && editing {
        spans.push(Span::styled(" ", Style::default().bg(Color::White)));
    }
    Line::from(spans)
}

pub fn mode_spans() -> Vec<Span<'static>> {
    vec![Span::styled(
        "Kw operations",
        Style::default().fg(Color::Green),
    )]
}

pub fn keys_hint_span(editing: bool) -> Span<'static> {
    if editing {
        Span::styled(
            "(ESC) cancel edit | (ENTER) confirm",
            Style::default().fg(Color::Red),
        )
    } else {
        Span::styled(
            "(ESC / q) back | (e) edit | (b) build | (c) cancel | (r) restore | (?) help",
            Style::default().fg(Color::Red),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_log_does_not_scroll() {
        assert_eq!(0, log_scroll_offset("cc1: compiling\n", 40, 10));
    }

    #[test]
    fn tall_log_pins_the_newest_rows() {
        let text = (0..20)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(15, log_scroll_offset(&text, 40, 5));
    }

    #[test]
    fn hard_wrapped_lines_count_toward_the_offset() {
        assert_eq!(1, log_scroll_offset("abcdefghij", 4, 2));
    }

    #[test]
    fn word_wrapped_lines_pin_the_newest_rows() {
        // Width 10, "aaaaa aaaaa aaaaa": char-ceil would be 2 rows; ratatui
        // word-wraps to 3, so height 2 must scroll by 1 to keep the end visible.
        assert_eq!(1, log_scroll_offset("aaaaa aaaaa aaaaa", 10, 2));
    }

    #[test]
    fn trailing_newline_does_not_invent_an_extra_row() {
        // ratatui's line_count does not treat a trailing newline as a
        // blank wrapped row, so a one-line pane shows "a" with no scroll.
        assert_eq!(0, log_scroll_offset("a\n", 10, 1));
    }

    #[test]
    fn zero_inner_area_does_not_scroll() {
        assert_eq!(0, log_scroll_offset("line\nline\n", 0, 10));
        assert_eq!(0, log_scroll_offset("line\nline\n", 10, 0));
    }
}
