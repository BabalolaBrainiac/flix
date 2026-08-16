use anyhow::{Context, Result};
use directories::ProjectDirs;
use std::path::{Path, PathBuf};

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

impl Config {
    pub fn load() -> Result<Self> {
        let proj_dirs =
            ProjectDirs::from("", "", "flix").context("Could not determine project directories")?;

        let data_dir = proj_dirs.data_dir().to_path_buf();
        let download_dir = data_dir.join("downloads");

        std::fs::create_dir_all(&download_dir).context("Failed to create download directory")?;

        Ok(Self {
            data_dir,
            download_dir,
        })
    }
}
