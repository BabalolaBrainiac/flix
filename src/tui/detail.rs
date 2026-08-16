use crate::session::TorrentFile;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};

pub fn render(
    f: &mut Frame,
    area: Rect,
    torrent_name: &str,
    files: &[TorrentFile],
    selected: usize,
    loading: bool,
    playback_pos: Option<f64>,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(area);

    let header = Paragraph::new(format!(
        "Torrent: {} | Enter/p: Play | n: Play Next Episode",
        torrent_name
    ))
    .block(Block::default().borders(Borders::ALL).title("Detail"));
    f.render_widget(header, chunks[0]);

    if loading {
        let p = Paragraph::new("Loading torrent metadata from swarm... Please wait.")
            .block(Block::default().borders(Borders::ALL))
            .style(Style::default().fg(Color::Yellow));
        f.render_widget(p, chunks[1]);
    } else if files.is_empty() {
        let p = Paragraph::new("No files found or no torrent selected.")
            .block(Block::default().borders(Borders::ALL))
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(p, chunks[1]);
    } else {
        let items: Vec<ListItem> = files
            .iter()
            .enumerate()
            .map(|(i, file)| {
                let color = if file.is_video {
                    Color::Green
                } else {
                    Color::White
                };
                let size_mb = file.length as f64 / 1_048_576.0;
                let text = format!(
                    "{}: {} ({:.2} MiB){}",
                    file.index,
                    file.name,
                    size_mb,
                    if i == selected { " <" } else { "" }
                );
                ListItem::new(text).style(if i == selected {
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(color)
                })
            })
            .collect();

        let mut state = ListState::default();
        state.select(Some(selected));

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Files in Torrent"),
            )
            .highlight_symbol(">> ");

        f.render_stateful_widget(list, chunks[1], &mut state);
    }

    let footer_text = if let Some(pos) = playback_pos {
        format!("Live Player Position: {:.1}s", pos)
    } else {
        "Ready to stream. Up/Down to choose file, Enter to Launch Player".to_string()
    };
    let footer = Paragraph::new(footer_text)
        .style(Style::default().fg(Color::Cyan))
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(footer, chunks[2]);
}
