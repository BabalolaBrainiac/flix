use super::app::App;
use crate::playback::{self, PlaybackPreparation};
use crate::player::ipc::MpvIpc;
use crate::player::{self, PlaybackOptions, PlayerKind};
use crate::session::{TorrentFile, TorrentId};
use crate::stream_server;
use anyhow::Result;
use std::time::Duration;
use tokio::sync::mpsc;

pub(super) struct PreparedPlayback {
    id: TorrentId,
    file: TorrentFile,
    preparation: PlaybackPreparation,
}

impl App {
    pub(super) async fn launch_playback(&mut self, id: TorrentId, file: TorrentFile) -> Result<()> {
        self.stop_playback()?;
        let session = self.session.clone();
        let data_dir = self.data_dir.clone();
        let file_for_task = file.clone();
        self.set_status(format!("Preparing {}...", file.name));
        self.pending_playback = Some(tokio::spawn(async move {
            session.select_files(id, &[file_for_task.index]).await?;
            let preparation = playback::prepare(&session, &data_dir, id, &file_for_task).await;
            Ok(PreparedPlayback {
                id,
                file: file_for_task,
                preparation,
            })
        }));
        Ok(())
    }

    pub(super) async fn poll_pending_playback(&mut self) {
        let finished = self
            .pending_playback
            .as_ref()
            .is_some_and(tokio::task::JoinHandle::is_finished);
        if !finished {
            return;
        }
        let Some(task) = self.pending_playback.take() else {
            return;
        };
        match task.await {
            Ok(Ok(prepared)) => {
                if let Err(error) = self.start_prepared_playback(prepared).await {
                    self.set_status(format!("Playback failed: {error}"));
                }
            }
            Ok(Err(error)) => self.set_status(format!("Playback preparation failed: {error}")),
            Err(error) => self.set_status(format!("Playback preparation failed: {error}")),
        }
    }

    async fn start_prepared_playback(&mut self, prepared: PreparedPlayback) -> Result<()> {
        let stream_url = self.stream_url(prepared.id, prepared.file.index).await?;
        let players = player::detect();
        let Some(selected) = players.first() else {
            self.set_status(player::install::prompt_install_guidance()?);
            return Ok(());
        };
        let ipc_socket = matches!(selected.kind, PlayerKind::Mpv)
            .then(|| std::env::temp_dir().join(format!("flix-{}.sock", std::process::id())));
        let subtitle_files = prepared
            .preparation
            .subtitle
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned())
            .into_iter()
            .collect();
        let options = PlaybackOptions::new(prepared.file.name.clone(), prepared.file.length)
            .with_subtitles(subtitle_files)
            .with_ipc_socket(ipc_socket.clone());
        self.player = Some(player::launch(selected, &stream_url, &options)?);
        self.active_file_index = Some(prepared.file.index);
        if let Some(socket) = ipc_socket {
            self.start_ipc_monitor(MpvIpc::new(socket));
        }
        self.record_playback(prepared.id).await?;
        self.set_playback_status(&prepared);
        Ok(())
    }

    fn set_playback_status(&mut self, prepared: &PreparedPlayback) {
        let subtitle = if prepared.preparation.subtitle.is_some() {
            " with English subtitles"
        } else {
            ""
        };
        let notes = if prepared.preparation.notes.is_empty() {
            String::new()
        } else {
            format!(" {}", prepared.preparation.notes.join(" "))
        };
        self.set_status(format!("Playing {}{subtitle}.{notes}", prepared.file.name));
    }

    async fn stream_url(&self, id: TorrentId, file_index: usize) -> Result<String> {
        let mut handle = self.server_handle.lock().await;
        if handle.is_none() {
            *handle = Some(stream_server::serve(self.session.clone()).await?);
        }
        let server = handle
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Stream server did not start"))?;
        Ok(stream_server::stream_url(server, id, file_index))
    }

    fn start_ipc_monitor(&mut self, ipc: MpvIpc) {
        let (sender, receiver) = mpsc::channel(4);
        self.player_ipc = Some(ipc.clone());
        self.position_receiver = Some(receiver);
        self.ipc_task = Some(tokio::spawn(async move {
            loop {
                if let Ok(Some(position)) = ipc.get_playback_position().await {
                    if sender.send(position).await.is_err() {
                        return;
                    }
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }));
    }

    async fn record_playback(&mut self, id: TorrentId) -> Result<()> {
        let info_hash = self.session.info_hash(id)?;
        let mut library = self.library.lock().await;
        library.mark_played(&info_hash);
        library.save()
    }

    pub(super) fn poll_playback(&mut self) {
        if let Some(receiver) = self.position_receiver.as_mut() {
            while let Ok(position) = receiver.try_recv() {
                self.playback_position = Some(position);
            }
        }
        let exited = self
            .player
            .as_mut()
            .and_then(|player| player.has_exited().ok())
            .unwrap_or(false);
        if exited {
            let finished_file = self.active_file_index.take();
            let _ = self.stop_playback();
            let selected_next = finished_file.is_some_and(|file_index| {
                let files = self
                    .selected_torrent_id
                    .and_then(|id| self.session.files(id).ok())
                    .unwrap_or_default();
                let current = files
                    .iter()
                    .position(|file| file.index == file_index)
                    .unwrap_or(self.selected_file_idx);
                self.selected_file_idx = current;
                self.selected_torrent_id
                    .is_some_and(|id| self.select_next_file(id))
            });
            let status = if selected_next {
                "Player exited. The next episode is selected."
            } else {
                "Player exited."
            };
            self.set_status(status.to_string());
        }
    }

    pub(super) fn stop_playback(&mut self) -> Result<()> {
        if let Some(task) = self.pending_playback.take() {
            task.abort();
        }
        if let Some(task) = self.ipc_task.take() {
            task.abort();
        }
        self.position_receiver = None;
        if let Some(ipc) = self.player_ipc.take() {
            ipc.cleanup()?;
        }
        self.playback_position = None;
        self.active_file_index = None;
        if let Some(mut player) = self.player.take() {
            player.stop()?;
        }
        Ok(())
    }
}
