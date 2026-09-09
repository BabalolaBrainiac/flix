//! Diagnostic for the playback warm-up.
//!
//! Usage:
//!   cargo run --example warm_probe -- <info_hash> <file_index>
//!
//! The probe runs the shipped path: it adds the torrent with the file selected
//! at add time, then it calls `warm_stream`. It prints the swarm counters each
//! second, so a slow start and a dead source look different.

use anyhow::{Context, Result};
use flix::session::{Source, TorrentSession};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let info_hash = args.next().context("pass an info hash")?;
    let file_index: usize = args.next().context("pass a file index")?.parse()?;

    let cache = tempfile::Builder::new().prefix("flix-probe-").tempdir()?;
    let session = Arc::new(TorrentSession::new(cache.path()).await?);
    let magnet = flix::stremio::build_magnet(&info_hash, None)?;

    let started = Instant::now();
    println!("[{:>6.1}s] adding {}", 0.0, &info_hash[..8]);
    let (id, preselected) = session
        .add_with_file(Source::parse(&magnet)?, file_index)
        .await?;
    println!(
        "[{:>6.1}s] metadata ready, file preselected: {preselected}",
        started.elapsed().as_secs_f32()
    );

    let files = session.files(id)?;
    let file = files
        .get(file_index)
        .with_context(|| format!("file {file_index} is not in this torrent"))?;
    println!(
        "[{:>6.1}s] file {} = {} ({:.2} MiB), files in torrent: {}",
        started.elapsed().as_secs_f32(),
        file_index,
        file.name,
        file.length as f64 / 1_048_576.0,
        files.len()
    );

    let monitor_session = session.clone();
    let monitor = tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(2)).await;
            if let Some(progress) = monitor_session.progress(id) {
                println!(
                    "[{:>6.1}s] peers={} {:.2} MiB/s verified={:.2} MiB",
                    started.elapsed().as_secs_f32(),
                    progress.peers,
                    progress.download_bps as f64 / 1_048_576.0,
                    progress.done as f64 / 1_048_576.0,
                );
            }
        }
    });

    let outcome = session.warm_stream(id, file_index).await;
    monitor.abort();

    match outcome {
        Ok(report) => println!(
            "RESULT ok after {:.1}s: {report}",
            started.elapsed().as_secs_f32()
        ),
        Err(error) => println!(
            "RESULT failed after {:.1}s: {error:#}",
            started.elapsed().as_secs_f32()
        ),
    }
    Ok(())
}
