use super::app::{App, Tab};
use crate::library::{Entry, SourceSerde};
use crate::session::{Source, TorrentFile};
use std::time::SystemTime;

impl App {
    pub(super) fn start_add(&mut self) {
        if self.pending_add.is_some() {
            self.set_status("A torrent add is already active.".to_string());
            return;
        }
        let input = self.input_buffer.trim().to_string();
        let source = match Source::parse(&input) {
            Ok(source) => source,
            Err(error) => {
                self.set_status(format!("Invalid source: {error}"));
                return;
            }
        };
        let session = self.session.clone();
        let library = self.library.clone();
        self.set_status("Adding torrent and fetching metadata...".to_string());
        self.pending_add = Some(tokio::spawn(async move {
            let id = session.add(source.clone()).await?;
            let files = session.files(id)?;
            session.select_files(id, &[]).await?;
            let display_name = display_name(&files, &input);
            let entry = Entry {
                info_hash: session.info_hash(id)?,
                display_name,
                source: SourceSerde::from(&source),
                added_at: SystemTime::now(),
                last_played: None,
                meta: Some(crate::meta::lookup(&input).await),
                output_name: crate::library::output_name_from_files(&files),
            };
            let mut stored = library.lock().await;
            stored.upsert(entry);
            stored.save()?;
            Ok(id)
        }));
    }

    pub(super) async fn poll_pending_add(&mut self) {
        let finished = self
            .pending_add
            .as_ref()
            .is_some_and(tokio::task::JoinHandle::is_finished);
        if !finished {
            return;
        }
        let Some(task) = self.pending_add.take() else {
            return;
        };
        let result = task.await;
        match result {
            Ok(Ok(id)) => {
                self.selected_torrent_id = Some(id);
                self.select_default_file(id);
                self.input_buffer.clear();
                self.active_tab = Tab::Detail;
                self.set_status("Torrent added.".to_string());
            }
            Ok(Err(error)) => self.set_status(format!("Torrent add failed: {error}")),
            Err(error) => self.set_status(format!("Torrent add task failed: {error}")),
        }
    }
}

fn display_name(files: &[TorrentFile], fallback: &str) -> String {
    crate::media::select_default_video(files)
        .map(|file| file.name.clone())
        .unwrap_or_else(|| fallback.to_string())
}
