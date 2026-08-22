use anyhow::{Context, Result};
use directories::ProjectDirs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Config {
    pub data_dir: PathBuf,
    pub download_dir: PathBuf,
}

/// Prefix for the temporary directory that holds a playback's downloaded data.
const PLAYBACK_CACHE_PREFIX: &str = "flix-play-";

pub struct PlaybackCache {
    directory: tempfile::TempDir,
}

impl PlaybackCache {
    pub fn new() -> Result<Self> {
        let directory = tempfile::Builder::new()
            .prefix(PLAYBACK_CACHE_PREFIX)
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

/// Removes playback caches left over from a previous run.
///
/// A `PlaybackCache` removes its directory when it is dropped, but a crash or a
/// force quit skips that, so multi-gigabyte directories leak in the system
/// temp folder. This sweep runs at startup and removes each `flix-play-*`
/// directory that nothing has written to for at least an hour. The age check
/// keeps it from deleting a cache that another running instance is streaming
/// into right now.
pub fn sweep_orphaned_playback_caches() {
    const STALE_AFTER: std::time::Duration = std::time::Duration::from_secs(3600);

    let temp_dir = std::env::temp_dir();
    let Ok(entries) = std::fs::read_dir(&temp_dir) else {
        return;
    };
    let now = std::time::SystemTime::now();

    for entry in entries.flatten() {
        let name = entry.file_name();
        if !name.to_string_lossy().starts_with(PLAYBACK_CACHE_PREFIX) {
            continue;
        }
        if !entry.path().is_dir() {
            continue;
        }
        let is_stale = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .map(|age| age >= STALE_AFTER)
            // Remove a directory whose age cannot be read; it is orphaned too.
            .unwrap_or(true);
        if is_stale {
            let path = entry.path();
            if let Err(error) = std::fs::remove_dir_all(&path) {
                tracing::warn!("Could not remove orphaned playback cache {path:?}: {error}");
            } else {
                tracing::info!("Removed orphaned playback cache {path:?}");
            }
        }
    }
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
