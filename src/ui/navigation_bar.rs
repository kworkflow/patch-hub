use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    text::Line,
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::app::{screens::CurrentScreen, AppViewModel};

use super::{bookmarked, details_actions, edit_config, latest, mail_list};

pub fn render(f: &mut Frame, vm: &AppViewModel<'_>, chunk: Rect) {
    let mode_footer_text = match vm.state.navigation.current_screen {
        CurrentScreen::MailingListSelection => mail_list::mode_footer_text(vm),
        CurrentScreen::BookmarkedPatchsets => bookmarked::mode_footer_text(),
        CurrentScreen::LatestPatchsets => latest::mode_footer_text(vm),
        CurrentScreen::PatchsetDetails => details_actions::mode_footer_text(),
        CurrentScreen::EditConfig => edit_config::mode_footer_text(vm),
    };
    let mode_footer = Paragraph::new(Line::from(mode_footer_text))
        .block(Block::default().borders(Borders::ALL))
        .centered();

    let current_keys_hint = {
        match vm.state.navigation.current_screen {
            CurrentScreen::MailingListSelection => mail_list::keys_hint(),
            CurrentScreen::BookmarkedPatchsets => bookmarked::keys_hint(),
            CurrentScreen::LatestPatchsets => latest::keys_hint(),
            CurrentScreen::PatchsetDetails => details_actions::keys_hint(),
            CurrentScreen::EditConfig => edit_config::keys_hint(vm),
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
