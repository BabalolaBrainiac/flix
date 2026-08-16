use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use ratatui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Tabs},
    Terminal,
};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;

use super::downloads::ActiveDownloadItem;
use super::input::{apply_add_key, AddInputAction};
use super::playback::PreparedPlayback;
use crate::library::{Entry, Library, Sort};
use crate::player::ipc::MpvIpc;
use crate::player::ManagedPlayer;
use crate::session::{Progress, Source, TorrentId, TorrentSession};
use crate::stream_server::ServerHandle;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Home,
    Add,
    Detail,
    Downloads,
}

pub struct App {
    pub session: Arc<TorrentSession>,
    pub library: Arc<Mutex<Library>>,
    pub server_handle: Arc<Mutex<Option<ServerHandle>>>,
    pub active_tab: Tab,
    pub input_buffer: String,
    pub selected_torrent_id: Option<TorrentId>,
    pub selected_file_idx: usize,
    pub selected_download_idx: usize,
    pub selected_home_idx: usize,
    pub home_sort: Sort,
    pub status_message: Option<(String, Instant)>,
    pub should_quit: bool,
    pub player_ipc: Option<MpvIpc>,
    pub playback_position: Option<f64>,
    pub player: Option<ManagedPlayer>,
    pub ipc_task: Option<JoinHandle<()>>,
    pub position_receiver: Option<mpsc::Receiver<f64>>,
    pub pending_add: Option<JoinHandle<Result<TorrentId>>>,
    pub(super) pending_playback: Option<JoinHandle<Result<PreparedPlayback>>>,
    pub(super) data_dir: PathBuf,
    pub(super) active_file_index: Option<usize>,
}

impl App {
    pub fn new(
        session: Arc<TorrentSession>,
        library: Arc<Mutex<Library>>,
        data_dir: PathBuf,
    ) -> Self {
        Self {
            session,
            library,
            server_handle: Arc::new(Mutex::new(None)),
            active_tab: Tab::Home,
            input_buffer: String::new(),
            selected_torrent_id: None,
            selected_file_idx: 0,
            selected_download_idx: 0,
            selected_home_idx: 0,
            home_sort: Sort::AddedDesc,
            status_message: None,
            should_quit: false,
            player_ipc: None,
            playback_position: None,
            player: None,
            ipc_task: None,
            position_receiver: None,
            pending_add: None,
            pending_playback: None,
            data_dir,
            active_file_index: None,
        }
    }

    pub async fn run<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> Result<()>
    where
        B::Error: std::fmt::Display,
    {
        while !self.should_quit {
            self.poll_pending_add().await;
            self.poll_pending_playback().await;
            self.poll_playback();

            // Clear expired status messages (after 4s)
            if let Some((_, created_at)) = &self.status_message {
                if created_at.elapsed() > Duration::from_secs(4) {
                    self.status_message = None;
                }
            }

            // Draw UI
            self.draw_ui(terminal).await?;

            // Process keyboard input
            if event::poll(Duration::from_millis(50))? {
                if let Event::Key(key) = event::read()? {
                    self.handle_key(key.code, key.modifiers).await?;
                }
            }
        }
        self.stop_playback()?;
        Ok(())
    }

    async fn draw_ui<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> Result<()>
    where
        B::Error: std::fmt::Display,
    {
        // Snapshot active downloads
        let torrent_ids = self.session.list();
        let mut active_items = Vec::new();
        for id in torrent_ids {
            let files = self.session.files(id).unwrap_or_default();
            let name = files
                .first()
                .map(|f| f.name.clone())
                .unwrap_or_else(|| format!("Torrent #{}", id));
            let progress = self.session.progress(id).unwrap_or(Progress {
                total: 0,
                done: 0,
                download_bps: 0,
                upload_bps: 0,
                peers: 0,
                finished: false,
            });
            active_items.push(ActiveDownloadItem { id, name, progress });
        }

        // Snapshot library items
        let lib_guard = self.library.lock().await;
        let sorted_entries: Vec<Entry> = lib_guard
            .sorted(self.home_sort)
            .into_iter()
            .cloned()
            .collect();
        drop(lib_guard);

        // Snapshot files for selected torrent
        let (selected_name, files_list) = if let Some(tid) = self.selected_torrent_id {
            let files = self.session.files(tid).unwrap_or_default();
            let name = files
                .first()
                .map(|f| f.name.clone())
                .unwrap_or_else(|| format!("Torrent #{}", tid));
            (name, files)
        } else {
            ("None".to_string(), Vec::new())
        };

        let active_tab = self.active_tab;
        let input_buf = self.input_buffer.clone();
        let status_text = self.status_message.as_ref().map(|(s, _)| s.as_str());
        let sel_home = self.selected_home_idx;
        let sel_file = self.selected_file_idx;
        let sel_dl = self.selected_download_idx;
        let playback_pos = self.playback_position;
        let sort = self.home_sort;

        terminal
            .draw(|f| {
                let size = f.area();
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Length(3), Constraint::Min(0)])
                    .split(size);

                let titles = vec!["1: Home", "2: Add", "3: Detail", "4: Downloads"];
                let tab_index = match active_tab {
                    Tab::Home => 0,
                    Tab::Add => 1,
                    Tab::Detail => 2,
                    Tab::Downloads => 3,
                };

                let tabs = Tabs::new(titles)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title("Flix - Terminal Torrent Streamer"),
                    )
                    .select(tab_index)
                    .style(Style::default().fg(Color::Cyan))
                    .highlight_style(
                        Style::default()
                            .add_modifier(Modifier::BOLD)
                            .fg(Color::Yellow),
                    );

                f.render_widget(tabs, chunks[0]);

                match active_tab {
                    Tab::Home => {
                        let refs: Vec<&Entry> = sorted_entries.iter().collect();
                        super::home::render(f, chunks[1], &refs, sel_home, &sort);
                    }
                    Tab::Add => {
                        super::add::render(f, chunks[1], &input_buf, status_text);
                    }
                    Tab::Detail => {
                        super::detail::render(
                            f,
                            chunks[1],
                            &selected_name,
                            &files_list,
                            sel_file,
                            false,
                            playback_pos,
                        );
                    }
                    Tab::Downloads => {
                        super::downloads::render(f, chunks[1], &active_items, sel_dl);
                    }
                }
            })
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        Ok(())
    }

    async fn handle_key(&mut self, code: KeyCode, _modifiers: KeyModifiers) -> Result<()> {
        if self.active_tab == Tab::Add {
            match apply_add_key(&mut self.input_buffer, code) {
                AddInputAction::Cancel => {
                    self.input_buffer.clear();
                    self.active_tab = Tab::Home;
                }
                AddInputAction::Switch => {
                    self.active_tab = Tab::Downloads;
                }
                AddInputAction::Submit => self.start_add(),
                AddInputAction::Changed | AddInputAction::None => {}
            }
            return Ok(());
        }

        // Global keys outside Add tab
        match code {
            KeyCode::Char('q') | KeyCode::Char('Q') => {
                self.should_quit = true;
            }
            KeyCode::Char('1') => self.active_tab = Tab::Home,
            KeyCode::Char('2') => self.active_tab = Tab::Add,
            KeyCode::Char('3') => self.active_tab = Tab::Detail,
            KeyCode::Char('4') => self.active_tab = Tab::Downloads,
            KeyCode::Tab => {
                self.active_tab = match self.active_tab {
                    Tab::Home => Tab::Add,
                    Tab::Add => Tab::Detail,
                    Tab::Detail => Tab::Downloads,
                    Tab::Downloads => Tab::Home,
                };
            }
            KeyCode::BackTab => {
                self.active_tab = match self.active_tab {
                    Tab::Home => Tab::Downloads,
                    Tab::Add => Tab::Home,
                    Tab::Detail => Tab::Add,
                    Tab::Downloads => Tab::Detail,
                };
            }
            // Tab-specific navigation
            _ => match self.active_tab {
                Tab::Home => {
                    let count = {
                        let lib = self.library.lock().await;
                        lib.sorted(self.home_sort).len()
                    };
                    match code {
                        KeyCode::Up if self.selected_home_idx > 0 => {
                            self.selected_home_idx -= 1;
                        }
                        KeyCode::Down if count > 0 && self.selected_home_idx + 1 < count => {
                            self.selected_home_idx += 1;
                        }
                        KeyCode::Char('s') => {
                            self.home_sort = match self.home_sort {
                                Sort::AddedDesc => Sort::TitleAsc,
                                Sort::TitleAsc => Sort::RatingDesc,
                                Sort::RatingDesc => Sort::AddedDesc,
                            };
                        }
                        KeyCode::Enter => {
                            // Resume / select from library
                            let entry_opt = {
                                let lib = self.library.lock().await;
                                let entries = lib.sorted(self.home_sort);
                                entries.get(self.selected_home_idx).cloned().cloned()
                            };

                            if let Some(entry) = entry_opt {
                                let src = Source::from(&entry.source);
                                self.set_status("Loading torrent from library...".to_string());
                                match self.session.add(src).await {
                                    Ok(id) => {
                                        self.session.select_files(id, &[]).await?;
                                        self.selected_torrent_id = Some(id);
                                        self.select_default_file(id);
                                        self.active_tab = Tab::Detail;
                                    }
                                    Err(e) => {
                                        self.set_status(format!("Error: {}", e));
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
                Tab::Downloads => {
                    let torrent_ids = self.session.list();
                    let count = torrent_ids.len();
                    match code {
                        KeyCode::Up if self.selected_download_idx > 0 => {
                            self.selected_download_idx -= 1;
                        }
                        KeyCode::Down if count > 0 && self.selected_download_idx + 1 < count => {
                            self.selected_download_idx += 1;
                        }
                        KeyCode::Enter => {
                            if let Some(&tid) = torrent_ids.get(self.selected_download_idx) {
                                self.selected_torrent_id = Some(tid);
                                self.active_tab = Tab::Detail;
                                self.select_default_file(tid);
                            }
                        }
                        _ => {}
                    }
                }
                Tab::Detail => {
                    let files = if let Some(tid) = self.selected_torrent_id {
                        self.session.files(tid).unwrap_or_default()
                    } else {
                        Vec::new()
                    };
                    let count = files.len();
                    match code {
                        KeyCode::Up if self.selected_file_idx > 0 => {
                            self.selected_file_idx -= 1;
                        }
                        KeyCode::Down if count > 0 && self.selected_file_idx + 1 < count => {
                            self.selected_file_idx += 1;
                        }
                        KeyCode::Enter | KeyCode::Char('p') => {
                            if let Some(tid) = self.selected_torrent_id {
                                if let Some(file) = files.get(self.selected_file_idx) {
                                    if file.is_video {
                                        self.launch_playback(tid, file.clone()).await?;
                                    } else {
                                        self.set_status(
                                            "Select a supported video file.".to_string(),
                                        );
                                    }
                                }
                            }
                        }
                        KeyCode::Char('n') => {
                            if let Some(tid) = self.selected_torrent_id {
                                if self.select_next_file(tid) {
                                    let files = self.session.files(tid).unwrap_or_default();
                                    if let Some(file) = files.get(self.selected_file_idx) {
                                        self.launch_playback(tid, file.clone()).await?;
                                    }
                                } else {
                                    self.set_status("There is no next episode.".to_string());
                                }
                            }
                        }
                        _ => {}
                    }
                }
                Tab::Add => {}
            },
        }

        Ok(())
    }

    pub(super) fn set_status(&mut self, msg: String) {
        self.status_message = Some((msg, Instant::now()));
    }

    pub(super) fn select_default_file(&mut self, id: TorrentId) {
        let files = self.session.files(id).unwrap_or_default();
        self.selected_file_idx = crate::media::select_default_video(&files)
            .and_then(|selected| files.iter().position(|file| file.index == selected.index))
            .unwrap_or(0);
    }

    pub(super) fn select_next_file(&mut self, id: TorrentId) -> bool {
        let files = self.session.files(id).unwrap_or_default();
        let Some(next) = crate::media::next_video_position(&files, self.selected_file_idx) else {
            return false;
        };
        self.selected_file_idx = next;
        true
    }
}

impl Drop for App {
    fn drop(&mut self) {
        if let Some(task) = self.pending_add.take() {
            task.abort();
        }
        if let Some(task) = self.ipc_task.take() {
            task.abort();
        }
        if let Some(task) = self.pending_playback.take() {
            task.abort();
        }
    }
}
