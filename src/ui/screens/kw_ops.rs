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
    let paragraph = Paragraph::new(scene.log_tail.clone())
        .block(Block::default().borders(Borders::ALL).title(" Build log "))
        .wrap(Wrap { trim: false });
    f.render_widget(paragraph, chunk);
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
