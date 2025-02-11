use super::{bookmarked, details_actions, edit_config, latest, mail_list};
use crate::{
    app::{self, App},
    logger::LoggerActor,
};
use app::screens::CurrentScreen;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    text::Line,
    widgets::{Block, Borders, Paragraph},
    Frame,
};

pub fn render(f: &mut Frame, app: &App<impl LoggerActor>, chunk: Rect) {
    let mode_footer_text = match app.current_screen {
        CurrentScreen::MailingListSelection => mail_list::mode_footer_text(app),
        CurrentScreen::BookmarkedPatchsets => bookmarked::mode_footer_text(),
        CurrentScreen::LatestPatchsets => latest::mode_footer_text(app),
        CurrentScreen::PatchsetDetails => details_actions::mode_footer_text(),
        CurrentScreen::EditConfig => edit_config::mode_footer_text(app),
    };
    let mode_footer = Paragraph::new(Line::from(mode_footer_text))
        .block(Block::default().borders(Borders::ALL))
        .centered();

    let current_keys_hint = {
        match app.current_screen {
            CurrentScreen::MailingListSelection => mail_list::keys_hint(),
            CurrentScreen::BookmarkedPatchsets => bookmarked::keys_hint(),
            CurrentScreen::LatestPatchsets => latest::keys_hint(),
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
