use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, HighlightSpacing, List, ListItem, ListState},
    Frame,
};

use crate::app::screens::bookmarked::BookmarkedPatchsets;

pub fn render_main(f: &mut Frame, bookmarked_patchsets: &BookmarkedPatchsets, chunk: Rect) {
    let patchset_index = bookmarked_patchsets.patchset_index;
    let mut list_items = Vec::<ListItem>::new();

    for (index, patch) in bookmarked_patchsets.bookmarked_patchsets.iter().enumerate() {
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

pub fn mode_footer_text() -> Vec<Span<'static>> {
    vec![Span::styled(
        "Bookmarked Patchsets",
        Style::default().fg(Color::Green),
    )]
}

pub fn keys_hint() -> Span<'static> {
    Span::styled(
        "(ESC / q) to return | (ENTER) to select | (?) help",
        Style::default().fg(Color::Red),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lore::patch::Patch;
    use ratatui::{
        backend::TestBackend,
        style::{Color, Modifier},
        Terminal,
    };

    fn make_patch(title: &str) -> Patch {
        let json = format!(
            r#"{{
                "title": "{title}",
                "author": {{ "name": "Test Author", "email": "test@test.com" }},
                "link": {{ "@href": "msg-id" }},
                "updated": "2024-01-01T00:00:00Z"
            }}"#
        );
        serde_json::from_str::<Patch>(&json).unwrap()
    }

    #[test]
    fn test_mode_footer_text() {
        let spans = mode_footer_text();
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "Bookmarked Patchsets");
        assert_eq!(spans[0].style.fg, Some(Color::Green));
    }

    #[test]
    fn test_keys_hint() {
        let span = keys_hint();
        assert!(span.content.contains("(ESC / q) to return"));
        assert_eq!(span.style.fg, Some(Color::Red));
    }

    #[test]
    fn test_render_main_empty_list_borders() {
        let backend = TestBackend::new(50, 10);
        let mut terminal = Terminal::new(backend).unwrap();

        let data = BookmarkedPatchsets {
            bookmarked_patchsets: vec![],
            patchset_index: 0,
        };

        terminal
            .draw(|f| {
                let chunk = f.area();
                render_main(f, &data, chunk);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();

        assert_eq!(buffer[(0, 0)].symbol(), "╔");
        assert_eq!(buffer[(49, 9)].symbol(), "╝");
    }

    #[test]
    fn test_render_main_populated_list_styling_robust() {
        let width = 150;
        let backend = TestBackend::new(width, 10);
        let mut terminal = Terminal::new(backend).unwrap();

        let patch1 = make_patch("UniqueNameA");
        let patch2 = make_patch("UniqueNameB");

        let data = BookmarkedPatchsets {
            bookmarked_patchsets: vec![patch1, patch2],
            patchset_index: 1,
        };

        terminal
            .draw(|f| {
                let chunk = f.area();
                render_main(f, &data, chunk);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();

        let line_0_text: String = (0..width).map(|x| buffer[(x, 1)].symbol()).collect();
        assert!(line_0_text.contains("UniqueNameA"));

        let target_char = "U";
        let mut found_x = None;

        for x in 0..width {
            if buffer[(x, 1)].symbol() == target_char {
                found_x = Some(x);
                break;
            }
        }

        let x_pos = found_x
            .expect("Não foi possível encontrar a letra 'U' na linha 1 para validar o estilo");

        let cell = &buffer[(x_pos, 1)];
        assert_eq!(
            cell.fg,
            Color::Yellow,
            "O texto deveria ser Amarelo, mas estava {:?}",
            cell.fg
        );
        assert!(!cell.modifier.contains(Modifier::REVERSED));

        assert_eq!(
            buffer[(1, 2)].symbol(),
            ">",
            "Símbolo de seleção '>' não encontrado na posição esperada"
        );

        let line_1_text: String = (0..width).map(|x| buffer[(x, 2)].symbol()).collect();
        assert!(line_1_text.contains("UniqueNameB"));

        found_x = None;
        for x in 0..width {
            if x > 2 && buffer[(x, 2)].symbol() == target_char {
                found_x = Some(x);
                break;
            }
        }
        let x_pos_sel = found_x.expect("Não foi possível encontrar a letra 'U' na linha 2");

        let sel_cell = &buffer[(x_pos_sel, 2)];
        assert_eq!(sel_cell.fg, Color::Cyan);
        assert!(sel_cell.modifier.contains(Modifier::BOLD));
        assert!(sel_cell.modifier.contains(Modifier::REVERSED));
    }

    #[test]
    fn test_render_truncation_padding() {
        let width = 200;
        let backend = TestBackend::new(width, 5);
        let mut terminal = Terminal::new(backend).unwrap();

        let patch = make_patch("Short");
        let data = BookmarkedPatchsets {
            bookmarked_patchsets: vec![patch],
            patchset_index: 0,
        };

        terminal
            .draw(|f| {
                render_main(f, &data, f.area());
            })
            .unwrap();

        let buffer = terminal.backend().buffer();

        let line_text: String = (0..width).map(|x| buffer[(x, 1)].symbol()).collect();

        assert!(line_text.contains("Short       "));
    }
}
