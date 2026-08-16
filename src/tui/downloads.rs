use crate::session::{Progress, TorrentId};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    widgets::{Block, Borders, Gauge, Paragraph},
    Frame,
};

pub struct ActiveDownloadItem {
    pub id: TorrentId,
    pub name: String,
    pub progress: Progress,
}

pub fn render(f: &mut Frame, area: Rect, items: &[ActiveDownloadItem], selected: usize) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    let header = Paragraph::new(format!(
        "Active Transfers ({}) | Up/Down to navigate, Enter to inspect files",
        items.len()
    ))
    .block(Block::default().borders(Borders::ALL).title("Downloads"));
    f.render_widget(header, chunks[0]);

    if items.is_empty() {
        let p = Paragraph::new("No active transfers. Press '2' to Add a magnet/torrent.")
            .style(Style::default().fg(Color::DarkGray))
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(p, chunks[1]);
        return;
    }

    let download_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            items
                .iter()
                .map(|_| Constraint::Length(4))
                .collect::<Vec<_>>(),
        )
        .split(chunks[1]);

    for (i, item) in items.iter().enumerate() {
        if i >= download_chunks.len() {
            break;
        }
        let chunk = download_chunks[i];
        let subchunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(2), Constraint::Length(2)])
            .split(chunk);

        let percent = if item.progress.total > 0 {
            ((item.progress.done as f64 / item.progress.total as f64) * 100.0).clamp(0.0, 100.0)
                as u16
        } else {
            0
        };

        let is_sel = i == selected;
        let dl_mbps = item.progress.download_bps as f64 / 1_048_576.0;
        let up_mbps = item.progress.upload_bps as f64 / 1_048_576.0;
        let done_mb = item.progress.done as f64 / 1_048_576.0;
        let total_mb = item.progress.total as f64 / 1_048_576.0;

        let title = format!(
            "{}[{}] {}{}",
            if is_sel { "> " } else { "" },
            item.id,
            item.name,
            if item.progress.finished {
                " (Completed)"
            } else {
                ""
            }
        );
        let gauge = Gauge::default()
            .block(
                Block::default()
                    .title(title)
                    .borders(Borders::TOP | Borders::LEFT | Borders::RIGHT),
            )
            .gauge_style(Style::default().fg(if is_sel { Color::Yellow } else { Color::Cyan }))
            .percent(percent);
        f.render_widget(gauge, subchunks[0]);

        let info = Paragraph::new(format!(
            "  Peers: {:<3} | DL: {:>6.2} MiB/s | UL: {:>6.2} MiB/s | {:>7.2} / {:>7.2} MiB ({:>3}%)",
            item.progress.peers, dl_mbps, up_mbps, done_mb, total_mb, percent
        ))
        .block(Block::default().borders(Borders::BOTTOM | Borders::LEFT | Borders::RIGHT))
        .style(if is_sel { Style::default().fg(Color::Yellow) } else { Style::default().fg(Color::White) });
        f.render_widget(info, subchunks[1]);
    }
}
