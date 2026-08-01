use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, HighlightSpacing, List, ListItem, ListState},
    Frame,
};

use crate::{app::view_model::BookmarkedViewModel, ui::scene::BookmarkedScene};

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

pub fn build_scene(vm: &BookmarkedViewModel) -> BookmarkedScene {
    BookmarkedScene {
        rows: vm.rows.clone(),
        selected_index: vm.selected_index,
    }
}

// ---------------------------------------------------------------------------
// Painter
// ---------------------------------------------------------------------------

pub fn paint(f: &mut Frame, scene: &BookmarkedScene, chunk: Rect) {
    let mut list_items = Vec::<ListItem>::new();

    for row in &scene.rows {
        let patch_title = format!("{:width$}", row.title, width = 70);
        let patch_title = format!("{:.width$}", patch_title, width = 70);
        let patch_author = format!("{:width$}", row.author_name, width = 30);
        let patch_author = format!("{:.width$}", patch_author, width = 30);
        list_items.push(ListItem::new(
            Line::from(Span::styled(
                format!(
                    "{:03}. V{:02} | #{:02} | {} | {}",
                    row.absolute_index, row.version, row.total_in_series, patch_title, patch_author
                ),
                Style::default().fg(Color::Yellow),
            ))
            .centered(),
        ));
    }

    let list_block = Block::default()
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Double)
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
    list_state.select(Some(scene.selected_index));

    f.render_stateful_widget(list, chunk, &mut list_state);
}

// ---------------------------------------------------------------------------
// Navigation-bar helpers
// ---------------------------------------------------------------------------

pub fn mode_spans() -> Vec<Span<'static>> {
    vec![Span::styled(
        "Bookmarked Patchsets",
        Style::default().fg(Color::Green),
    )]
}

pub fn keys_hint_span() -> Span<'static> {
    Span::styled(
        "(ESC / q) to return | (ENTER) to select | (?) help",
        Style::default().fg(Color::Red),
    )
}
