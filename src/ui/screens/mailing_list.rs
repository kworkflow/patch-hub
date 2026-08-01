use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, HighlightSpacing, List, ListItem, ListState},
    Frame,
};

use crate::{
    app::view_model::{MailingListSelectionViewModel, TargetListStatus},
    ui::scene::MailingListScene,
};
pub fn build_scene(vm: &MailingListSelectionViewModel) -> MailingListScene {
    MailingListScene {
        entries: vm.entries.clone(),
        highlighted_index: vm.highlighted_index,
    }
}

pub fn paint(f: &mut Frame, scene: &MailingListScene, chunk: Rect) {
    let mut list_items = Vec::<ListItem>::new();

    for entry in &scene.entries {
        list_items.push(ListItem::new(
            Line::from(vec![
                Span::styled(entry.name.clone(), Style::default().fg(Color::Magenta)),
                Span::styled(
                    format!(" - {}", entry.description),
                    Style::default().fg(Color::White),
                ),
            ])
            .centered(),
        ));
    }

    let list_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .style(Style::default());

    let list = List::new(list_items)
        .block(list_block)
        .highlight_style(
            Style::default()
                .add_modifier(Modifier::BOLD)
                .add_modifier(Modifier::REVERSED)
                .fg(Color::Cyan),
        )
        .highlight_symbol(">")
        .highlight_spacing(HighlightSpacing::Always);

    let mut list_state = ListState::default();
    list_state.select(Some(scene.highlighted_index));

    f.render_stateful_widget(list, chunk, &mut list_state);
}

pub fn mode_spans(vm: &MailingListSelectionViewModel) -> Vec<Span<'static>> {
    let text_area = match vm.target_list_status {
        TargetListStatus::Empty => {
            Span::styled("type the target list", Style::default().fg(Color::DarkGray))
        }
        TargetListStatus::ExactMatch => {
            Span::styled(vm.target_list.clone(), Style::default().fg(Color::Green))
        }
        TargetListStatus::PrefixMatch => Span::styled(
            vm.target_list.clone(),
            Style::default().fg(Color::LightCyan),
        ),
        TargetListStatus::NoMatch => {
            Span::styled(vm.target_list.clone(), Style::default().fg(Color::Red))
        }
    };

    vec![
        Span::styled("Target List: ", Style::default().fg(Color::Green)),
        text_area,
    ]
}

pub fn keys_hint_span() -> Span<'static> {
    Span::styled(
        "(ESC) to quit | (ENTER) to confirm | (?) help",
        Style::default().fg(Color::Red),
    )
}
