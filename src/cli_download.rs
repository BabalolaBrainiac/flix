use crate::media;
use crate::session::{Source, TorrentFile, TorrentSession};
use anyhow::{Context, Result};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

pub async fn run(
    session: Arc<TorrentSession>,
    download_dir: &Path,
    torrent: &str,
    preferred_file_index: Option<usize>,
) -> Result<()> {
    println!("Adding torrent...");
    let source = Source::parse(torrent)?;
    // Select the file at add time. A second file selection on a live torrent
    // can deadlock inside librqbit.
    let (id, preselected) = match preferred_file_index {
        Some(file_index) => session.add_with_file(source, file_index).await?,
        None => (session.add(source).await?, false),
    };
    let files = session.files(id)?;
    let file = selected_file(&files, preferred_file_index)?;
    if crate::session::needs_file_selection(preferred_file_index, preselected, file.index) {
        session.select_files(id, &[file.index]).await?;
    }
    println!("Downloading: {}", file.name);
    println!("Destination: {}", download_dir.display());

    let handle = session.handle(id).context("Torrent not found")?;
    let completion = handle.wait_until_completed();
    tokio::pin!(completion);
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    loop {
        tokio::select! {
            result = &mut completion => {
                result?;
                println!("Download completed: {}", file.name);
                return Ok(());
            }
            _ = tokio::signal::ctrl_c() => {
                println!("Download stopped. Completed data remains in {}.", download_dir.display());
                return Ok(());
            }
            _ = interval.tick() => {
                if let Some(progress) = session.progress(id) {
                    println!(
                        "Downloaded {:.1} MiB at {:.1} MiB/s from {} peers",
                        progress.done as f64 / 1_048_576.0,
                        progress.download_bps as f64 / 1_048_576.0,
                        progress.peers
                    );
                }
            }
        }
    }
}

fn selected_file(
    files: &[TorrentFile],
    preferred_file_index: Option<usize>,
) -> Result<&TorrentFile> {
    if let Some(index) = preferred_file_index {
        if let Some(file) = files
            .iter()
            .find(|file| file.index == index && file.is_video)
        {
            return Ok(file);
        }
    }
    media::select_default_video(files).context("Torrent contains no supported video file")
}
