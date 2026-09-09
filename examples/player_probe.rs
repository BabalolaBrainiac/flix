use anyhow::{Context, Result};
use flix::player::{self, PlaybackOptions, PlayerKind};
use std::{path::PathBuf, time::Duration};

/// Opens a local sample in VLC and checks startup, process lifetime, and Stop.
#[tokio::main]
async fn main() -> Result<()> {
    let path = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .context("Supply a local video path")?,
    );
    let metadata = std::fs::metadata(&path).context("The sample video is unavailable")?;
    let player = player::detect()
        .into_iter()
        .find(|player| player.kind == PlayerKind::Vlc)
        .context("Install VLC before this check")?;
    let options = PlaybackOptions::new("Flix playback check".into(), metadata.len());
    let mut process = player::launch(&player, &player::file_uri(&path.canonicalize()?), &options)?;
    process.wait_for_startup().await?;
    tokio::time::sleep(Duration::from_secs(3)).await;
    anyhow::ensure!(
        process.poll_exit()?.is_none(),
        "VLC closed before the sample check ended"
    );
    process.stop()?;
    anyhow::ensure!(process.has_exited()?, "VLC did not stop");
    println!("VLC startup, process lifetime, and Stop passed.");
    Ok(())
}
