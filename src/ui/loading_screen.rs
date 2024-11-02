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

const LOADING_AREA_EXTRA_FACTOR_WIDTH: f32 = 1.3;
const LOADING_AREA_EXTRA_LINES: u16 = 2;

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
    let frame_area = f.area();
    let loading_text = format!("{} {}", title, spinner());

    let (width_pct, height_pct) =
        calculate_loading_percentages(loading_text.len(), frame_area.width, frame_area.height);

    let loading_area = centered_rect(width_pct, height_pct, frame_area);

    let loading_par = Paragraph::new(Line::from(Span::styled(
        loading_text,
        Style::default().fg(Color::Green),
    )))
    .block(Block::default().borders(Borders::ALL))
    .centered()
    .wrap(Wrap { trim: true });

    f.render_widget(loading_par, loading_area);
}

fn calculate_loading_percentages(
    loading_text_len: usize,
    frame_area_width: u16,
    frame_area_height: u16,
) -> (u16, u16) {
    let min_width = (loading_text_len as f32 * LOADING_AREA_EXTRA_FACTOR_WIDTH).ceil() as u16;
    let min_height = {
        let lines = (loading_text_len as u16).div_ceil(frame_area_width).max(1);
        lines + LOADING_AREA_EXTRA_LINES
    };

    let width_pct = (100 * min_width).div_ceil(frame_area_width).min(100);
    let height_pct = (100 * min_height).div_ceil(frame_area_height).min(100);

    (width_pct, height_pct)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_loading_percentages() {
        // Test case 1: standard case with reasonable frame size
        let (width_pct, height_pct) = calculate_loading_percentages(10, 40, 20);
        assert_eq!(width_pct, 33); // (10 * 1.3 = 13 / 40 * 100) = 32.5, rounded up to 33
        assert_eq!(height_pct, 15); // min_height: 1 line + 2 extra = 3, (3 / 20 * 100) = 15

        // Test case 2: text len exceeds frame width, forcing width_pct to cap at 100%
        let (width_pct, height_pct) = calculate_loading_percentages(80, 40, 20);
        assert_eq!(width_pct, 100); // min_width: 104 exceeds frame width, capped at 100%
        assert_eq!(height_pct, 20); // min_height: 2 lines + 2 extra = 4, (4 / 20 * 100) = 20

        // Test case 3: Very small loading text within large frame area
        let (width_pct, height_pct) = calculate_loading_percentages(5, 50, 30);
        assert_eq!(width_pct, 14); // (5 * 1.3 = 7 / 50 * 100) = 14
        assert_eq!(height_pct, 10); // min_height: 1 line + 2 extra = 3, (3 / 30 * 100) = 10

        // Test case 4: small frame height, causing height_pct to cap at 100%
        let (width_pct, height_pct) = calculate_loading_percentages(100, 40, 4);
        assert_eq!(width_pct, 100); // min_width: 130 exceeds frame width, capped at 100%
        assert_eq!(height_pct, 100); // min_height: 3 lines + 2 extra = 5, capped at 100%

        // Test case 5: small frame width, but big height
        let (width_pct, height_pct) = calculate_loading_percentages(100, 10, 100);
        assert_eq!(width_pct, 100); // min_width: 130 exceeds frame width, capped at 100%
        assert_eq!(height_pct, 12); // min_height: 10 lines + 2 extra = 12
    }
}
