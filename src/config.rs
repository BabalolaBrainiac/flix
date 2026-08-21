use anyhow::{Context, Result};
use directories::ProjectDirs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Config {
    pub data_dir: PathBuf,
    pub download_dir: PathBuf,
}

pub struct PlaybackCache {
    directory: tempfile::TempDir,
}

impl PlaybackCache {
    pub fn new() -> Result<Self> {
        let directory = tempfile::Builder::new()
            .prefix("flix-play-")
            .tempdir()
            .context("Failed to create the temporary playback cache")?;
        Ok(Self { directory })
    }

    pub fn path(&self) -> &Path {
        self.directory.path()
    }

    pub fn close(self) -> Result<()> {
        self.directory
            .close()
            .context("Failed to remove the temporary playback cache")
    }
}

/// Returns the per-user data directory that Flix uses for state.
pub fn data_dir() -> Result<PathBuf> {
    let proj_dirs =
        ProjectDirs::from("", "", "flix").context("Could not determine project directories")?;
    Ok(proj_dirs.data_dir().to_path_buf())
}

impl Config {
    pub fn load() -> Result<Self> {
        let data_dir = data_dir()?;
        let download_dir = data_dir.join("downloads");

        std::fs::create_dir_all(&download_dir).context("Failed to create download directory")?;

        Ok(Self {
            data_dir,
            download_dir,
        })
    }
}
