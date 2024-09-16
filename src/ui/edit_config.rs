pub fn render(f: &mut Frame, app: &App, chunk: Rect) {
    let edit_config_state = app.edit_config_state.as_ref().unwrap();
    let mut constraints = Vec::new();

    for _ in 0..(chunk.height / 3) {
        constraints.push(Constraint::Length(3));
    }

    let config_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(chunk);

    let highlighted_entry = edit_config_state.get_highlighted_entry();
    for i in 0..edit_config_state.get_number_of_configs() {
        if i + 1 > config_chunks.len() {
            break;
        }

        let (config, value) = edit_config_state.get_config_by_index(i);
        let value = Line::from(
            if edit_config_state.is_editing() && i == highlighted_entry {
                vec![
                    Span::styled(
                        edit_config_state.get_editing_val().to_string(),
                        Style::default(),
                    ),
                    Span::styled(" ", Style::default().bg(Color::White)),
                ]
            } else {
                vec![Span::from(value)]
            },
        );

        let config_entry = Paragraph::new(value)
            .centered()
            .block(Block::default().borders(Borders::ALL).title(config))
            .style(
                if i == highlighted_entry && edit_config_state.is_editing() {
                    Style::default()
                        .fg(Color::LightYellow)
                        .add_modifier(Modifier::BOLD)
                } else if i == highlighted_entry {
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                },
            );

        f.render_widget(config_entry, config_chunks[i]);
    }
}
