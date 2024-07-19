use lore_peek::patch::Patch;
use ratatui::{
    layout::{
        Alignment, Constraint, Direction, Layout, Rect
    },
    style::{
        Color, Modifier, Style
    },
    text::{Line, Span, Text}, widgets::{
        Block, Borders, HighlightSpacing, List, ListItem, ListState, Padding, Paragraph, Wrap
    },
    Frame
};

use crate::app::{App, BookmarkedPatchsetsState, CurrentScreen, PatchsetAction, PAGE_SIZE};

pub fn draw_ui(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(3),
        ])
        .split(f.size());

    render_title(f, chunks[0]);

    match app.current_screen {
        CurrentScreen::MailingListSelection => {
            f.render_widget(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(ratatui::widgets::BorderType::Double),
                chunks[1]
            )
        },
        CurrentScreen::BookmarkedPatchsets => render_bookmarked_patchsets(f, &app.bookmarked_patchsets_state, chunks[1]),
        CurrentScreen::LatestPatchsets => render_list(f, app, chunks[1]),
        CurrentScreen::PatchsetDetails => render_patchset_details_and_actions(f, app, chunks[1]),
    }

    render_navi_bar(f, app, chunks[2]);
}

fn render_title(f: &mut Frame, chunk: Rect) {
    let title_block = Block::default()
        .borders(Borders::ALL)
        .style(Style::default())
        .title_alignment(Alignment::Center);

    let title_content: String = "Lore Peek".to_string();

    let title = Paragraph::new(Text::styled(
            title_content, Style::default().fg(Color::Green).add_modifier(Modifier::ITALIC),
        ))
        .centered()
        .block(title_block);

    f.render_widget(title, chunk);
}

fn render_bookmarked_patchsets(f: &mut Frame, bookmarked_patchsets_state: &BookmarkedPatchsetsState, chunk: Rect) {
    let patchset_index = bookmarked_patchsets_state.patchset_index;
    let mut index: u32 = 0;
    let mut list_items = Vec::<ListItem>::new();

    for patch in &bookmarked_patchsets_state.bookmarked_patchsets {
        let patch_title = format!("{:width$}", patch.get_title(), width = 70);
        let patch_title = format!("{:.width$}", patch_title, width = 70);
        let patch_author = format!("{:width$}", patch.get_author().name, width = 30);
        let patch_author = format!("{:.width$}", patch_author, width = 30);
        list_items.push(ListItem::new(Line::from(Span::styled(
            format!(
                "{:03}. V{:02} | #{:02} | {} | {}",
                index, patch.get_version(), patch.get_total_in_series(), patch_title, patch_author
            ),
            Style::default().fg(Color::Yellow),
        )).centered()));
        index += 1;
    }

    let list_block = Block::default()
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Double)
        .style(Style::default());
    
    let list = List::new(list_items).block(list_block)
        .highlight_style(
            Style::default()
                .add_modifier(Modifier::BOLD)
                .add_modifier(Modifier::REVERSED)
                .fg(Color::Cyan),
        )
        .highlight_symbol(">")
        .highlight_spacing(HighlightSpacing::Always);

    let mut list_state = ListState::default();
    list_state.select(Some(patchset_index as usize));

    f.render_stateful_widget(list, chunk, &mut list_state);
}

fn render_list(f: &mut Frame, app: &App, chunk: Rect) {
    let page_number = app.latest_patchsets_state.as_ref().unwrap().get_page_number();
    let patchset_index = app.latest_patchsets_state.as_ref().unwrap().get_patchset_index();
    let mut list_items = Vec::<ListItem>::new();

    let patch_feed_page: Vec<&Patch> = app.latest_patchsets_state
        .as_ref()
        .unwrap()
        .get_current_patch_feed_page()
        .unwrap();
    
    let mut index: u32 = (page_number - 1) * PAGE_SIZE;
    for patch in patch_feed_page {
        let patch_title = format!("{:width$}", patch.get_title(), width = 70);
        let patch_title = format!("{:.width$}", patch_title, width = 70);
        let patch_author = format!("{:width$}", patch.get_author().name, width = 30);
        let patch_author = format!("{:.width$}", patch_author, width = 30);
        list_items.push(ListItem::new(Line::from(Span::styled(
            format!(
                "{:03}. V{:02} | #{:02} | {} | {}",
                index, patch.get_version(), patch.get_total_in_series(), patch_title, patch_author
            ),
            Style::default().fg(Color::Yellow),
        )).centered()));
        index += 1;
    }

    let list_block = Block::default()
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Double)
        .style(Style::default());
    
    let list = List::new(list_items).block(list_block)
        .highlight_style(
            Style::default()
                .add_modifier(Modifier::BOLD)
                .add_modifier(Modifier::REVERSED)
                .fg(Color::Cyan),
        )
        .highlight_symbol(">")
        .highlight_spacing(HighlightSpacing::Always);

    let mut list_state = ListState::default();
    list_state.select(Some((
        patchset_index - (page_number - 1) * PAGE_SIZE
    ).try_into().unwrap()));

    f.render_stateful_widget(list, chunk, &mut list_state);
}

fn render_patchset_details_and_actions(f: &mut Frame, app: &App, chunk: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(30),
            Constraint::Percentage(70),
        ])
        .split(chunk);

    let details_and_actions_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(50),
            Constraint::Percentage(50),
        ])
        .split(chunks[0]);

    let patchset_details = &app.patchset_details_and_actions_state.as_ref().unwrap().representative_patch;
    let patchset_details = vec![
        Line::from(
            vec![
                Span::styled(r#"  Title: "#, Style::default().fg(Color::Cyan)),
                Span::styled(format!("{}", patchset_details.get_title()), Style::default().fg(Color::White)),
            ]
        ),
        Line::from(
            vec![
                Span::styled("Author: ", Style::default().fg(Color::Cyan)),
                Span::styled(format!("{}", patchset_details.get_author().name), Style::default().fg(Color::White)),
            ]
        ),
        Line::from(
            vec![
                Span::styled("Version: ", Style::default().fg(Color::Cyan)),
                Span::styled(format!("{}", patchset_details.get_version()), Style::default().fg(Color::White)),
            ]
        ),
        Line::from(
            vec![
                Span::styled("Patch count: ", Style::default().fg(Color::Cyan)),
                Span::styled(format!("{}", patchset_details.get_total_in_series()), Style::default().fg(Color::White)),
            ]
        ),
        Line::from(
            vec![
                Span::styled("Last updated: ", Style::default().fg(Color::Cyan)),
                Span::styled(format!("{}", patchset_details.get_updated()), Style::default().fg(Color::White)),
            ]
        ),
    ];

    let patchset_details = Paragraph::new(patchset_details)
        .block(
            Block::default()
            .borders(Borders::ALL).border_type(ratatui::widgets::BorderType::Double)
            .title(Line::styled(" Details ", Style::default().fg(Color::Green)).left_aligned())
            .padding(Padding::vertical(1))
        )
        .left_aligned()
        .wrap(Wrap { trim: true });

    f.render_widget(patchset_details, details_and_actions_chunks[0]);

    let patchset_actions = &app.patchset_details_and_actions_state.as_ref().unwrap().patchset_actions;
    let patchset_actions = vec![
        Line::from(
            vec![
                if *patchset_actions.get(&PatchsetAction::Bookmark).unwrap() {
                    Span::styled("[x] ", Style::default().fg(Color::Green))
                } else {
                    Span::styled("[ ] ", Style::default().fg(Color::Cyan))
                },
                Span::styled("b", Style::default().fg(Color::Cyan).add_modifier(Modifier::UNDERLINED).add_modifier(Modifier::BOLD)),
                Span::styled("ookmark", Style::default().fg(Color::Cyan)),
            ]
        )
    ];
    let patchset_actions = Paragraph::new(patchset_actions)
        .block(
            Block::default()
            .borders(Borders::ALL).border_type(ratatui::widgets::BorderType::Double)
            .title(Line::styled(" Actions ", Style::default().fg(Color::Green)).left_aligned())
            .padding(Padding::vertical(1))
        )
        .centered();

    f.render_widget(patchset_actions, details_and_actions_chunks[1]);

    let preview_index = app.patchset_details_and_actions_state
        .as_ref().unwrap().preview_index;
    let preview_offset = app.patchset_details_and_actions_state
        .as_ref().unwrap().preview_scroll_offset;
    let patch_preview = app.patchset_details_and_actions_state
        .as_ref()
        .unwrap()
        .patches[preview_index as usize]
        .replace('\t', "        ");
    let patch_preview = Paragraph::new(Text::from(format!("{patch_preview}")))
        .block(
            Block::default()
            .borders(Borders::ALL).border_type(ratatui::widgets::BorderType::Double)
            .title(Line::styled(" Preview ", Style::default().fg(Color::Green)).left_aligned())
            .padding(Padding::vertical(1))
        )
        .left_aligned()
        .scroll((preview_offset as u16, 0));

    f.render_widget(patch_preview, chunks[1]);
}

fn render_navi_bar(f: &mut Frame, app: &App, chunk: Rect) {
    let mode_footer_text: Vec<Span>;
    match app.current_screen {
        CurrentScreen::MailingListSelection => {
            mode_footer_text = vec![
                Span::styled("Target List: ", Style::default().fg(Color::Green)),
                if app.target_list.is_empty() {
                    Span::styled("type the target list", Style::default().fg(Color::DarkGray))
                } else {
                    Span::styled(&app.target_list, Style::default().fg(Color::LightCyan))
                }
            ]
        },
        CurrentScreen::BookmarkedPatchsets => {
            mode_footer_text = vec![
                Span::styled("Bookmarked Patchsets", Style::default().fg(Color::Green)),
            ]
        },
        CurrentScreen::LatestPatchsets => {
            mode_footer_text = vec![
                Span::styled(
                    format!("Latest Patchsets from {} (page {})", &app.target_list, &app.latest_patchsets_state.as_ref().unwrap().get_page_number()),
                    Style::default().fg(Color::Green)
                )
            ]
        },
        CurrentScreen::PatchsetDetails => {
            mode_footer_text = vec![
                Span::styled("Patchset Details and Actions", Style::default().fg(Color::Green)),
            ]
        },
    }
    let mode_footer = Paragraph::new(Line::from(mode_footer_text))
        .block(Block::default().borders(Borders::ALL)).centered();

    let current_keys_hint = {
        match app.current_screen {
            CurrentScreen::MailingListSelection => Span::styled(
                "(ESC) to quit | (ENTER) to confirm | (TAB) to bookmarked patchsets",
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
            CurrentScreen::PatchsetDetails => Span::styled(
                "(ESC) to return | (ENTER) run actions | ( j / 🡇 ) down | ( k / 🡅 ) up | (n) next patch | (p) previous patch",
                Style::default().fg(Color::Red),
            ),
        }
    };

    let keys_hint_footer = Paragraph::new(Line::from(current_keys_hint))
        .block(Block::default().borders(Borders::ALL)).centered();

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
