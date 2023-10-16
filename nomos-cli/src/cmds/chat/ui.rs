use super::{App, ChatMessage};
use ratatui::prelude::*;
use ratatui::{
    backend::Backend,
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
};

pub fn ui<B: Backend>(f: &mut Frame<B>, app: &App) {
    if app.username.is_none() {
        select_username(f, app)
    } else {
        chat(f, app)
    }
}

fn select_username<B: Backend>(f: &mut Frame<B>, app: &App) {
    assert!(app.username.is_none());
    let block = Block::default()
        .title("NomosChat")
        .borders(Borders::ALL)
        .style(Style::new().white().on_black());
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Min(5),
                Constraint::Min(1),
                Constraint::Min(5),
                Constraint::Min(0),
            ]
            .as_ref(),
        )
        .split(block.inner(f.size()));
    f.render_widget(block, f.size());

    let text = vec![
        Line::from(vec![
            Span::raw("Welcome to "),
            Span::styled("NomosChat", Style::new().green().italic()),
            "!".into(),
        ]),
        Line::from("The first almost-instant messaging protocol".red()),
    ];

    let welcome = Paragraph::new(text)
        .style(Style::new().white().on_black())
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });
    f.render_widget(welcome, chunks[0]);

    let help_text = Line::from("Press <Enter> to select your username, or <Esc> to quit.");
    let help = Paragraph::new(help_text)
        .alignment(Alignment::Center)
        .style(Style::new().white().on_black());
    f.render_widget(help, chunks[1]);

    let input = Paragraph::new(app.input.value())
        .style(Style::new().white().on_black())
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Select username")
                .border_style(Style::default().fg(Color::Yellow)),
        );
    f.render_widget(input, centered_rect(chunks[2], 50, 50));
}

fn centered_rect(r: Rect, percent_x: u16, percent_y: u16) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Percentage((100 - percent_y) / 2),
                Constraint::Percentage(percent_y),
                Constraint::Percentage((100 - percent_y) / 2),
            ]
            .as_ref(),
        )
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints(
            [
                Constraint::Percentage((100 - percent_x) / 2),
                Constraint::Percentage(percent_x),
                Constraint::Percentage((100 - percent_x) / 2),
            ]
            .as_ref(),
        )
        .split(popup_layout[1])[1]
}

fn chat<B: Backend>(f: &mut Frame<B>, app: &App) {
    let block = Block::default()
        .title("NomosChat")
        .borders(Borders::ALL)
        .style(Style::new().white().on_black());
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(5), Constraint::Min(1)].as_ref())
        .split(block.inner(f.size()));
    f.render_widget(block, f.size());

    let messages: Vec<ListItem> = app
        .messages
        .iter()
        .map(|ChatMessage { author, message }| {
            let content = Line::from(Span::raw(format!("{author}: {message}")));
            ListItem::new(content)
        })
        .collect();
    let messages =
        List::new(messages).block(Block::default().borders(Borders::ALL).title("Messages"));
    f.render_widget(messages, centered_rect(chunks[1], 90, 100));

    let style = if !app.message_in_flight {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let input = Paragraph::new(app.input.value())
        .style(Style::new().white().on_black())
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Press <Enter> to send message")
                .border_style(style),
        );

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
        .split(chunks[0]);

    let waiting_animation = std::iter::repeat(".")
        .take(app.last_updated.elapsed().as_secs() as usize % 4)
        .collect::<Vec<_>>()
        .join("");

    let status = Paragraph::new(
        app.message_status
            .as_ref()
            .map(|s| format!("{}{}", s.display(), waiting_animation))
            .unwrap_or_default(),
    )
    .block(Block::default().borders(Borders::ALL).title("Status:"));

    f.render_widget(input, centered_rect(chunks[0], 80, 50));
    f.render_widget(status, centered_rect(chunks[1], 80, 50));
}
