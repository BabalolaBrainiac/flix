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
    owner: Option<std::fs::File>,
    directory: tempfile::TempDir,
}

impl PlaybackCache {
    pub fn new() -> Result<Self> {
        let directory = tempfile::Builder::new()
            .prefix(PLAYBACK_CACHE_PREFIX)
            .tempdir()
            .context("Failed to create the temporary playback cache")?;
        let owner = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(directory.path().join("owner.lock"))?;
        owner
            .try_lock()
            .context("Failed to lock the playback cache")?;
        Ok(Self {
            owner: Some(owner),
            directory,
        })
    }

    pub fn path(&self) -> &Path {
        self.directory.path()
    }

    pub fn close(mut self) -> Result<()> {
        self.owner.take();
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
    sweep_playback_caches(&std::env::temp_dir());
}

pub fn sweep_playback_caches(temp_dir: &Path) {
    let Ok(entries) = std::fs::read_dir(temp_dir) else {
        return;
    };
    let now = std::time::SystemTime::now();
    for entry in entries.flatten() {
        if !entry
            .file_name()
            .to_string_lossy()
            .starts_with(PLAYBACK_CACHE_PREFIX)
            || !entry.file_type().is_ok_and(|kind| kind.is_dir())
        {
            continue;
        }
        let path = entry.path();
        let age = entry
            .metadata()
            .ok()
            .and_then(|meta| meta.modified().ok())
            .and_then(|modified| now.duration_since(modified).ok())
            .unwrap_or_default();
        let marker = path.join("owner.lock");
        if marker
            .symlink_metadata()
            .is_ok_and(|meta| meta.file_type().is_symlink())
        {
            continue;
        }
        if marker.exists() {
            if age < std::time::Duration::from_secs(60) {
                continue;
            }
            let Ok(owner) = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&marker)
            else {
                continue;
            };
            if owner.try_lock().is_err() {
                continue;
            }
            drop(owner);
        } else if age < std::time::Duration::from_secs(3600) {
            continue;
        }
        if let Err(error) = std::fs::remove_dir_all(path) {
            tracing::warn!("Could not remove an unused playback cache: {error}");
        }
    }
}

/// Runs cache cleanup outside the async runtime worker pool.
///
/// Deleting a large stale cache can take seconds. Startup can continue while
/// this task removes only caches that meet the existing stale-age rule.
pub fn schedule_orphaned_playback_cache_sweep() {
    tokio::task::spawn_blocking(sweep_orphaned_playback_caches);
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
