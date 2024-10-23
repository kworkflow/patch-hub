use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, HighlightSpacing, List, ListItem, ListState},
    Frame,
};

use crate::app::App;

pub fn render_mailing_list_selection(f: &mut Frame, app: &App, chunk: Rect) {
    let highlighted_list_index = app.mailing_list_selection_state.highlighted_list_index;
    let mut list_items = Vec::<ListItem>::new();

    for mailing_list in &app.mailing_list_selection_state.possible_mailing_lists {
        list_items.push(ListItem::new(
            Line::from(vec![
                Span::styled(
                    mailing_list.name().to_string(),
                    Style::default().fg(Color::Magenta),
                ),
                Span::styled(
                    format!(" - {}", mailing_list.description()),
                    Style::default().fg(Color::White),
                ),
            ])
            .centered(),
        ))
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
    list_state.select(Some(highlighted_list_index));

    f.render_stateful_widget(list, chunk, &mut list_state);
}
