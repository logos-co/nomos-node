use super::{App, ChatMessage};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};

pub fn ui(f: &mut Frame, app: &App) {
    if app.username.is_none() {
        select_username(f, app)
    } else {
        chat(f, app)
    }
}

fn select_username(f: &mut Frame, app: &App) {
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

fn chat(f: &mut Frame, app: &App) {
    let block = Block::default()
        .title("NomosChat")
        .borders(Borders::ALL)
        .style(Style::new().white().on_black());
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(5), Constraint::Min(1), Constraint::Max(10)].as_ref())
        .split(block.inner(f.size()));
    f.render_widget(block, f.size());

    let messages_rect = centered_rect(chunks[1], 90, 100);
    render_messages(f, app, messages_rect);

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
        .split(chunks[0]);

    render_input(f, app, chunks[0]);
    render_status(f, app, chunks[1]);
}

fn render_messages(f: &mut Frame, app: &App, rect: Rect) {
    let messages: Vec<ListItem> = app
        .messages
        .iter()
        .map(|ChatMessage { author, message }| {
            let content = if author == app.username.as_ref().unwrap() {
                // pad to make it appear aligned on the right
                let pad = " ".repeat((rect.width as usize).saturating_sub(message.len()));
                Line::from(vec![Span::raw(pad), Span::raw(message)])
            } else {
                Line::from(vec![
                    Span::styled(format!("{author}: "), Style::new().fg(Color::Yellow).bold()),
                    Span::raw(message),
                ])
                .alignment(Alignment::Left)
            };
            ListItem::new(content)
        })
        .collect();
    let messages =
        List::new(messages).block(Block::default().borders(Borders::ALL).title("Messages"));
    f.render_widget(messages, rect);
}

fn render_input(f: &mut Frame, app: &App, rect: Rect) {
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
    f.render_widget(input, rect);
}
fn render_status(f: &mut Frame, app: &App, rect: Rect) {
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
    f.render_widget(status, rect);
}
