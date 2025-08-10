use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    text::Line,
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::{model::Model, views::View};

use super::{bookmarked, details_actions, edit_config, latest, mail_list};

pub fn render(f: &mut Frame, model: &Model, chunk: Rect) {
    let mode_footer_text = match model.current_screen {
        View::MailingLists => mail_list::mode_footer_text(model),
        View::BookmarkedPatchsets => bookmarked::mode_footer_text(),
        View::LatestPatchsets => latest::mode_footer_text(model),
        View::PatchsetDetails => details_actions::mode_footer_text(),
        View::EditConfig => edit_config::mode_footer_text(model),
    };
    let mode_footer = Paragraph::new(Line::from(mode_footer_text))
        .block(Block::default().borders(Borders::ALL))
        .centered();

    let current_keys_hint = {
        match model.current_screen {
            View::MailingLists => mail_list::keys_hint_text(),
            View::BookmarkedPatchsets => bookmarked::keys_hint(),
            View::LatestPatchsets => latest::keys_hint(),
            View::PatchsetDetails => details_actions::keys_hint(),
            View::EditConfig => edit_config::keys_hint(model),
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
