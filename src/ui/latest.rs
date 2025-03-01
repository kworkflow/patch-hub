use crate::app::App;
use patch_hub::lore::patch::Patch;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, HighlightSpacing, List, ListItem, ListState},
    Frame,
};

pub fn render_main(f: &mut Frame, app: &App, chunk: Rect, page_size: usize) {
    let page_number = app.latest_patchsets.as_ref().unwrap().page_number();
    let patchset_index = app.latest_patchsets.as_ref().unwrap().patchset_index();
    let mut list_items = Vec::<ListItem>::new();

    let patch_feed_page: Vec<&Patch> = app
        .latest_patchsets
        .as_ref()
        .unwrap()
        .get_current_patch_feed_page()
        .unwrap();

    let mut index: usize = (page_number - 1) * page_size;
    for patch in patch_feed_page {
        let patch_title = format!("{:width$}", patch.title(), width = 70);
        let patch_title = format!("{:.width$}", patch_title, width = 70);
        let patch_author = format!("{:width$}", patch.author().name, width = 30);
        let patch_author = format!("{:.width$}", patch_author, width = 30);
        list_items.push(ListItem::new(
            Line::from(Span::styled(
                format!(
                    "{:03}. V{:02} | #{:02} | {} | {}",
                    index,
                    patch.version(),
                    patch.total_in_series(),
                    patch_title,
                    patch_author
                ),
                Style::default().fg(Color::Yellow),
            ))
            .centered(),
        ));
        index += 1;
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
    list_state.select(Some(patchset_index - (page_number - 1) * page_size));

    f.render_stateful_widget(list, chunk, &mut list_state);
}

pub fn mode_footer_text(app: &App) -> Vec<Span> {
    vec![Span::styled(
        format!(
            "Latest Patchsets from {} (page {})",
            &app.latest_patchsets.as_ref().unwrap().target_list(),
            &app.latest_patchsets.as_ref().unwrap().page_number()
        ),
        Style::default().fg(Color::Green),
    )]
}

pub fn keys_hint() -> Span<'static> {
    Span::styled(
        "(ESC / q) to return | (ENTER) to select | ( h / 🡄 ) previous page | ( l / 🡆 ) next page | (?) help",
        Style::default().fg(Color::Red),
    )
}
