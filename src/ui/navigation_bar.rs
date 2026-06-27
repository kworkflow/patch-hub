use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    text::Line,
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::app::view_model::{AppViewModel, ScreenViewModel};

use super::{bookmarked, details_actions, edit_config, latest, mail_list};

pub fn render(f: &mut Frame, vm: &AppViewModel, chunk: Rect) {
    let mode_footer_text = match &vm.screen {
        ScreenViewModel::MailingListSelection(mls_vm) => mail_list::mode_footer_text(mls_vm),
        ScreenViewModel::Bookmarked(_) => bookmarked::mode_footer_text(),
        ScreenViewModel::Latest(l_vm) => latest::mode_footer_text(l_vm),
        ScreenViewModel::PatchsetDetails(_) => details_actions::mode_footer_text(),
        ScreenViewModel::EditConfig(ec_vm) => edit_config::mode_footer_text(ec_vm),
    };
    let mode_footer = Paragraph::new(Line::from(mode_footer_text))
        .block(Block::default().borders(Borders::ALL))
        .centered();

    let current_keys_hint = match &vm.screen {
        ScreenViewModel::MailingListSelection(_) => mail_list::keys_hint(),
        ScreenViewModel::Bookmarked(_) => bookmarked::keys_hint(),
        ScreenViewModel::Latest(_) => latest::keys_hint(),
        ScreenViewModel::PatchsetDetails(_) => details_actions::keys_hint(),
        ScreenViewModel::EditConfig(ec_vm) => edit_config::keys_hint(ec_vm),
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
