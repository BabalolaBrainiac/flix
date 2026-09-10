use anyhow::{Context, Result};
use librqbit::{create_torrent, CreateTorrentOptions};
use std::path::PathBuf;

/// Builds a `.torrent` file from a local path for the post-release QA check.
///
/// Usage: qa_torrent <input file or directory> <output .torrent path>
///
/// The output uses the same `create_torrent` call that Flix uses elsewhere,
/// so the file layout it expects under the download directory matches
/// exactly: a single input file downloads to `<download_dir>/<basename>`,
/// and an input directory downloads to `<download_dir>/<dir basename>/...`
/// with the same relative paths.
#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let input = PathBuf::from(args.next().context("Supply an input file or directory")?);
    let output = PathBuf::from(args.next().context("Supply an output .torrent path")?);

    let torrent = create_torrent(&input, CreateTorrentOptions::default())
        .await
        .context("Failed to build the QA fixture torrent")?;
    let bytes = torrent
        .as_bytes()
        .context("Failed to encode the QA fixture torrent")?;

    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).context("Failed to create the torrent output directory")?;
    }
    std::fs::write(&output, &bytes).context("Failed to write the QA fixture torrent")?;

    println!("{}", output.display());
    Ok(())
}
