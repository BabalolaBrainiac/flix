use crate::library::{Entry, Sort};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};

pub fn render(f: &mut Frame, area: Rect, entries: &[&Entry], selected: usize, sort: &Sort) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    let sort_name = match sort {
        Sort::AddedDesc => "Recently Added",
        Sort::TitleAsc => "Title (A-Z)",
        Sort::RatingDesc => "Rating (High to Low)",
    };

    let header = Paragraph::new(format!(
        "Library [Sort: {} (press 's' to cycle)] | Press Enter to Open",
        sort_name
    ))
    .block(Block::default().borders(Borders::ALL).title("Home"));
    f.render_widget(header, chunks[0]);

    if entries.is_empty() {
        let p = Paragraph::new("No torrents in library yet. Press '2' to go to Add tab.")
            .style(Style::default().fg(Color::DarkGray))
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(p, chunks[1]);
        return;
    }

    let items: Vec<ListItem> = entries
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let tmdb = e
                .meta
                .as_ref()
                .and_then(|m| m.tmdb_score)
                .map(|s| format!("{:.1}", s))
                .unwrap_or_else(|| "-".to_string());
            let lb = e
                .meta
                .as_ref()
                .and_then(|m| m.letterboxd_score)
                .map(|s| format!("{:.1}", s))
                .unwrap_or_else(|| "-".to_string());
            let year = e
                .meta
                .as_ref()
                .and_then(|m| m.year)
                .map(|y| format!(" ({})", y))
                .unwrap_or_default();

            let title_line = format!(
                "{}  {}{}",
                e.display_name,
                year,
                if i == selected { " <" } else { "" }
            );
            let scores = format!("TMDB: {} | Letterboxd: {}", tmdb, lb);
            let text = format!("{:<50} | {}", title_line, scores);

            ListItem::new(text).style(if i == selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            })
        })
        .collect();

    let mut state = ListState::default();
    state.select(Some(selected));

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Saved Media"))
        .highlight_symbol(">> ");

    f.render_stateful_widget(list, chunks[1], &mut state);
}
