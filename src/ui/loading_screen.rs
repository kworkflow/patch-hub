use std::fmt::Display;

use ratatui::{
    prelude::Backend,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame, Terminal,
};

use super::centered_rect;

const SPINNER: [char; 8] = [
    '\u{1F311}',
    '\u{1F312}',
    '\u{1F313}',
    '\u{1F314}',
    '\u{1F315}',
    '\u{1F316}',
    '\u{1F317}',
    '\u{1F318}',
];
static mut SPINNER_TICK: usize = 1;

/// This function renders a loading screen taking a `terminal` instance and a
/// `title`.
pub fn render<B: Backend>(mut terminal: Terminal<B>, title: impl Display) -> Terminal<B> {
    let _ = terminal.draw(|f| draw_loading_screen(f, title));
    terminal
}

/// Gets the current spinner state and updates the tick.
fn spinner() -> char {
    let char_to_ret = SPINNER[unsafe { SPINNER_TICK }];
    unsafe {
        SPINNER_TICK = (SPINNER_TICK + 1) % 8;
    }
    char_to_ret
}

/// The actual implementation of the loading screen rendering. Currently the
/// loading notification is static.
fn draw_loading_screen(f: &mut Frame, title: impl Display) {
    let loading_text = format!("{} {}", title, spinner());

    let loading_par = Paragraph::new(Line::from(Span::styled(
        loading_text,
        Style::default().fg(Color::Green),
    )))
    .block(Block::default().borders(Borders::ALL))
    .centered()
    .wrap(Wrap { trim: true });

    let loading_area = centered_rect(30, 10, f.area());
    f.render_widget(loading_par, loading_area);
}
