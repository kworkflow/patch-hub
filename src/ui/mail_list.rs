use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, HighlightSpacing, List, ListItem, ListState},
    Frame,
};

use crate::app::App;

pub fn render_main(f: &mut Frame, app: &App, chunk: Rect) {
    let highlighted_list_index = app.mailing_list_selection.highlighted_list_index;
    let mut list_items = Vec::<ListItem>::new();

    for mailing_list in &app.mailing_list_selection.possible_mailing_lists {
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

pub fn mode_footer_text(app: &App) -> Vec<Span<'_>> {
    let text_area = if app.mailing_list_selection.target_list.is_empty() {
        Span::styled("type the target list", Style::default().fg(Color::DarkGray))
    } else {
        let exact_match = app.mailing_list_selection.mailing_lists.iter().any(|ml| {
            ml.name()
                .eq_ignore_ascii_case(&app.mailing_list_selection.target_list)
        });

        if exact_match {
            Span::styled(
                &app.mailing_list_selection.target_list,
                Style::default().fg(Color::Green),
            )
        } else if !app.mailing_list_selection.possible_mailing_lists.is_empty() {
            Span::styled(
                &app.mailing_list_selection.target_list,
                Style::default().fg(Color::LightCyan),
            )
        } else {
            Span::styled(
                &app.mailing_list_selection.target_list,
                Style::default().fg(Color::Red),
            )
        }
    };

    vec![
        Span::styled("Target List: ", Style::default().fg(Color::Green)),
        text_area,
    ]
}

pub fn keys_hint() -> Span<'static> {
    Span::styled(
        "(ESC) to quit | (ENTER) to confirm | (?) help",
        Style::default().fg(Color::Red),
    )
}
