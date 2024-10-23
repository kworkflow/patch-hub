use super::{details_actions, edit_config};
use crate::app::{self, App};
use app::screens::CurrentScreen;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

pub fn render_navi_bar(f: &mut Frame, app: &App, chunk: Rect) {
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
