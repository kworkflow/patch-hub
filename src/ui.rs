use patch_hub::patch::Patch;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, HighlightSpacing, List, ListItem, ListState, Paragraph},
    Frame,
};

use crate::app::{self, App};
use app::screens::{bookmarked::BookmarkedPatchsetsState, CurrentScreen};

mod details_actions;
mod edit_config;
pub mod loading_screen;
mod render_patchset;

pub fn draw_ui(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(3),
        ])
        .split(f.area());

    render_title(f, chunks[0]);

    match app.current_screen {
        CurrentScreen::MailingListSelection => render_mailing_list_selection(f, app, chunks[1]),
        CurrentScreen::BookmarkedPatchsets => {
            render_bookmarked_patchsets(f, &app.bookmarked_patchsets_state, chunks[1])
        }
        CurrentScreen::LatestPatchsets => render_list(f, app, chunks[1]),
        CurrentScreen::PatchsetDetails => details_actions::render_main(f, app, chunks[1]),
        CurrentScreen::EditConfig => edit_config::render_main(f, app, chunks[1]),
    }

    render_navi_bar(f, app, chunks[2]);
}

fn render_title(f: &mut Frame, chunk: Rect) {
    let title_block = Block::default()
        .borders(Borders::ALL)
        .style(Style::default())
        .title_alignment(Alignment::Center);

    let title_content: String = "Patch Hub".to_string();

    let title = Paragraph::new(Text::styled(
        title_content,
        Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::ITALIC),
    ))
    .centered()
    .block(title_block);

    f.render_widget(title, chunk);
}

fn render_mailing_list_selection(f: &mut Frame, app: &App, chunk: Rect) {
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

fn render_bookmarked_patchsets(
    f: &mut Frame,
    bookmarked_patchsets_state: &BookmarkedPatchsetsState,
    chunk: Rect,
) {
    let patchset_index = bookmarked_patchsets_state.patchset_index;
    let mut list_items = Vec::<ListItem>::new();

    for (index, patch) in bookmarked_patchsets_state
        .bookmarked_patchsets
        .iter()
        .enumerate()
    {
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
    list_state.select(Some(patchset_index));

    f.render_stateful_widget(list, chunk, &mut list_state);
}

fn render_list(f: &mut Frame, app: &App, chunk: Rect) {
    let page_number = app.latest_patchsets_state.as_ref().unwrap().page_number();
    let patchset_index = app
        .latest_patchsets_state
        .as_ref()
        .unwrap()
        .patchset_index();
    let mut list_items = Vec::<ListItem>::new();

    let patch_feed_page: Vec<&Patch> = app
        .latest_patchsets_state
        .as_ref()
        .unwrap()
        .get_current_patch_feed_page()
        .unwrap();

    let mut index: usize = (page_number - 1) * app.config.page_size();
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
    list_state.select(Some(
        patchset_index - (page_number - 1) * app.config.page_size(),
    ));

    f.render_stateful_widget(list, chunk, &mut list_state);
}

fn render_navi_bar(f: &mut Frame, app: &App, chunk: Rect) {
    let mode_footer_text = match app.current_screen {
        CurrentScreen::MailingListSelection => {
            let mut text_area = Span::default();

            if app.mailing_list_selection_state.target_list.is_empty() {
                text_area =
                    Span::styled("type the target list", Style::default().fg(Color::DarkGray))
            } else {
                for mailing_list in &app.mailing_list_selection_state.mailing_lists {
                    if mailing_list
                        .name()
                        .eq(&app.mailing_list_selection_state.target_list)
                    {
                        text_area = Span::styled(
                            &app.mailing_list_selection_state.target_list,
                            Style::default().fg(Color::Green),
                        );
                        break;
                    } else if mailing_list
                        .name()
                        .starts_with(&app.mailing_list_selection_state.target_list)
                    {
                        text_area = Span::styled(
                            &app.mailing_list_selection_state.target_list,
                            Style::default().fg(Color::LightCyan),
                        );
                    }
                }
                if text_area.content.is_empty() {
                    text_area = Span::styled(
                        &app.mailing_list_selection_state.target_list,
                        Style::default().fg(Color::Red),
                    );
                }
            }

            vec![
                Span::styled("Target List: ", Style::default().fg(Color::Green)),
                text_area,
            ]
        }
        CurrentScreen::BookmarkedPatchsets => {
            vec![Span::styled(
                "Bookmarked Patchsets",
                Style::default().fg(Color::Green),
            )]
        }
        CurrentScreen::LatestPatchsets => {
            vec![Span::styled(
                format!(
                    "Latest Patchsets from {} (page {})",
                    &app.latest_patchsets_state.as_ref().unwrap().target_list(),
                    &app.latest_patchsets_state.as_ref().unwrap().page_number()
                ),
                Style::default().fg(Color::Green),
            )]
        }
        CurrentScreen::PatchsetDetails => details_actions::mode_footer_text(),
        CurrentScreen::EditConfig => edit_config::mode_footer_text(app),
    };
    let mode_footer = Paragraph::new(Line::from(mode_footer_text))
        .block(Block::default().borders(Borders::ALL))
        .centered();

    let current_keys_hint = {
        match app.current_screen {
            CurrentScreen::MailingListSelection => Span::styled(
                "(ESC) to quit | (ENTER) to confirm | (🡇 ) down | (🡅 ) up | (F1) to bookmarked patchsets | (F2) edit config | (F5) refresh lists",
                Style::default().fg(Color::Red),
            ),
            CurrentScreen::BookmarkedPatchsets => Span::styled(
                "(ESC) to return | (ENTER) to select | ( j / 🡇 ) down | ( k / 🡅 ) up",
                Style::default().fg(Color::Red),
            ),
            CurrentScreen::LatestPatchsets => Span::styled(
                "(ESC) to return | (ENTER) to select | ( j / 🡇 ) down | ( k / 🡅 ) up | ( h / 🡄 ) previous page | ( l / 🡆 ) next page",
                Style::default().fg(Color::Red),
            ),
            CurrentScreen::PatchsetDetails => details_actions::keys_hint(),
            CurrentScreen::EditConfig => edit_config::keys_hint(app),
        }
    };

    let keys_hint_footer = Paragraph::new(Line::from(current_keys_hint))
        .block(Block::default().borders(Borders::ALL))
        .centered();

    let footer_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(80)])
        .split(chunk);

    f.render_widget(mode_footer, footer_chunks[0]);
    f.render_widget(keys_hint_footer, footer_chunks[1]);
}

/// helper function to create a centered rect using up certain percentage of the available rect `r`
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    // Cut the given rectangle into three vertical pieces
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    // Then cut the middle vertical piece into three width-wise pieces
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1] // Return the middle chunk
}
