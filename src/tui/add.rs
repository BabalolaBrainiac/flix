use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

pub fn render(f: &mut Frame, area: Rect, input: &str, status: Option<&str>) {
    let chunks = Layout::default()
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(0),
        ])
        .split(area);

    let block = Block::default()
        .title("Add Torrent (Type or Paste Magnet URL / File Path)")
        .borders(Borders::ALL);

    let p = Paragraph::new(if input.is_empty() {
        "Paste magnet:?xt=... or /path/to.torrent here"
    } else {
        input
    })
    .block(block)
    .style(if input.is_empty() {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    });

    f.render_widget(p, chunks[0]);

    let status_text = status
        .unwrap_or("Press Enter to Add | Esc to Clear / Return to Home | Tab to Switch Screen");
    let status_p = Paragraph::new(status_text)
        .style(Style::default().fg(if status.is_some() {
            Color::Cyan
        } else {
            Color::Gray
        }))
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(status_p, chunks[1]);
}
