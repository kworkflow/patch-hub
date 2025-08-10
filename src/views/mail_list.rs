use std::rc::Rc;

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, HighlightSpacing, List, ListItem, ListState},
    Frame,
};

use crate::{
    model::Model, viewmodels::mailing_list_viewmodel::MailingListsState,
    views::widgets::centered_text_widget,
};

pub fn render_navigation_bar(frame: &mut Frame, state: &MailingListsState, chunks: Rc<[Rect]>) {
    let target_list_text = target_list_text(state);
    let target_list_widget = centered_text_widget(target_list_text);

    let keys_hint_text = keys_hint_text();
    let keys_hint_widget = centered_text_widget(vec![keys_hint_text]);

    frame.render_widget(target_list_widget, chunks[0]);
    frame.render_widget(keys_hint_widget, chunks[1]);
}

pub fn render_main_content(f: &mut Frame, state: &MailingListsState, chunk: Rect) {
    let highlighted_list_index = state.highlighted_list_index;
    let mut list_items = Vec::<ListItem>::new();

    for mailing_list in &state.filtered_mailing_lists {
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

fn target_list_text(state: &MailingListsState) -> Vec<Span> {
    let mut text_area = Span::default();

    if state.search_string.is_empty() {
        text_area = Span::styled("type the target list", Style::default().fg(Color::DarkGray))
    } else {
        for mailing_list in &state.mailing_lists {
            if mailing_list.name().eq(&state.search_string) {
                text_area = Span::styled(&state.search_string, Style::default().fg(Color::Green));
                break;
            } else if mailing_list.name().starts_with(&state.search_string) {
                text_area =
                    Span::styled(&state.search_string, Style::default().fg(Color::LightCyan));
            }
        }
        if text_area.content.is_empty() {
            text_area = Span::styled(&state.search_string, Style::default().fg(Color::Red));
        }
    }

    vec![
        Span::styled("Target List: ", Style::default().fg(Color::Green)),
        text_area,
    ]
}

pub fn keys_hint_text() -> Span<'static> {
    Span::styled(
        "(ESC) to quit | (ENTER) to confirm | (?) help",
        Style::default().fg(Color::Red),
    )
}
pub fn render_main(f: &mut Frame, model: &Model, chunk: Rect) {
    let highlighted_list_index = model.mailing_list_selection.highlighted_list_index;
    let mut list_items = Vec::<ListItem>::new();

    for mailing_list in &model.mailing_list_selection.possible_mailing_lists {
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

pub fn mode_footer_text(model: &Model) -> Vec<Span> {
    let mut text_area = Span::default();

    if model.mailing_list_selection.target_list.is_empty() {
        text_area = Span::styled("type the target list", Style::default().fg(Color::DarkGray))
    } else {
        for mailing_list in &model.mailing_list_selection.mailing_lists {
            if mailing_list
                .name()
                .eq(&model.mailing_list_selection.target_list)
            {
                text_area = Span::styled(
                    &model.mailing_list_selection.target_list,
                    Style::default().fg(Color::Green),
                );
                break;
            } else if mailing_list
                .name()
                .starts_with(&model.mailing_list_selection.target_list)
            {
                text_area = Span::styled(
                    &model.mailing_list_selection.target_list,
                    Style::default().fg(Color::LightCyan),
                );
            }
        }
        if text_area.content.is_empty() {
            text_area = Span::styled(
                &model.mailing_list_selection.target_list,
                Style::default().fg(Color::Red),
            );
        }
    }

    vec![
        Span::styled("Target List: ", Style::default().fg(Color::Green)),
        text_area,
    ]
}

#[allow(dead_code)]
pub fn keys_hint() -> Span<'static> {
    Span::styled(
        "(ESC) to quit | (ENTER) to confirm | (?) help",
        Style::default().fg(Color::Red),
    )
}
