mod bookmarked;
mod details_actions;
mod edit_config;
mod latest;
pub mod loading_screen;
mod mail_list;
mod navigation_bar;
pub mod popup;
mod widgets;

use std::rc::Rc;

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style, Stylize},
    text::Text,
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::{infrastructure::terminal::Tui, model::Model};

use crate::viewmodels::ViewModelState;

#[derive(Copy, Debug, Clone, PartialEq, Eq, Hash)]
pub enum View {
    MailingLists,
    BookmarkedPatchsets,
    LatestPatchsets,
    PatchsetDetails,
    EditConfig,
}

impl View {
    #[allow(dead_code, unused_variables)]
    pub fn render_screen(
        &self,
        terminal: &mut Tui,
        state: &ViewModelState,
    ) -> color_eyre::Result<()> {
        terminal.draw(|frame| {
            View::clear_frame(frame);

            let main_layout = View::main_layout();
            let main_layout_chunks = View::chunk_layout(&main_layout, frame);

            View::render_title_on_chunk(frame, main_layout_chunks[0]);
            self.render_main_content(frame, state, main_layout_chunks[1]);

            let footer_layout = View::footer_layout();
            let footer_layout_chunks = View::chunk_layout(&footer_layout, frame);
            self.render_navigation_bar(frame, state, footer_layout_chunks);
        })?;

        Ok(())
    }

    fn render_navigation_bar(&self, frame: &mut Frame, state: &ViewModelState, chunk: Rc<[Rect]>) {
        match (self, state) {
            (View::MailingLists, ViewModelState::MailingLists(s)) => {
                mail_list::render_navigation_bar(frame, s, chunk)
            }
            (View::BookmarkedPatchsets, _) => todo!(),
            (View::LatestPatchsets, _) => todo!(),
            (View::PatchsetDetails, _) => todo!(),
            (View::EditConfig, _) => todo!(),
            _ => todo!(),
        };
    }

    fn render_main_content(&self, frame: &mut Frame, state: &ViewModelState, chunk: Rect) {
        match (self, state) {
            (View::MailingLists, ViewModelState::MailingLists(s)) => {
                mail_list::render_main_content(frame, s, chunk)
            }
            (View::BookmarkedPatchsets, _) => todo!(),
            (View::LatestPatchsets, _) => todo!(),
            (View::PatchsetDetails, _) => todo!(),
            (View::EditConfig, _) => todo!(),
            _ => todo!(),
        };
    }

    fn render_title_on_chunk(frame: &mut Frame, chunk: Rect) {
        let title_block = Block::default()
            .borders(Borders::ALL)
            .style(Style::default())
            .title_alignment(Alignment::Center);

        let title_content: String = "patch-hub".to_string();

        let title = Paragraph::new(Text::styled(
            title_content,
            Style::default().fg(Color::Green).bold(),
        ))
        .centered()
        .block(title_block);

        frame.render_widget(title, chunk);
    }

    fn clear_frame(frame: &mut Frame) {
        frame.render_widget(Clear, frame.area());
    }

    fn main_layout() -> Layout {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(1),
                Constraint::Length(3),
            ])
    }

    fn footer_layout() -> Layout {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(30), Constraint::Percentage(80)])
    }

    fn chunk_layout(layout: &Layout, frame: &mut Frame) -> Rc<[Rect]> {
        layout.split(frame.area())
    }
}

pub fn draw_ui(f: &mut Frame, model: &Model) {
    // Clear the whole screen for sanitizing reasons
    f.render_widget(Clear, f.area());

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(3),
        ])
        .split(f.area());

    View::render_title_on_chunk(f, chunks[0]);
    match model.current_screen {
        View::MailingLists => mail_list::render_main(f, model, chunks[1]),
        View::BookmarkedPatchsets => {
            bookmarked::render_main(f, &model.bookmarked_patchsets, chunks[1])
        }
        View::LatestPatchsets => latest::render_main(f, model, chunks[1]),
        View::PatchsetDetails => details_actions::render_main(f, model, chunks[1]),
        View::EditConfig => edit_config::render_main(f, model, chunks[1]),
    }

    navigation_bar::render(f, model, chunks[2]);

    model.popup.as_ref().inspect(|p| {
        let (x, y) = p.dimensions();
        let rect = centered_rect(x, y, f.area());
        p.render(f, rect);
    });
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
