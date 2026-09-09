use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const STALE_AFTER: Duration = Duration::from_secs(14 * 24 * 3600); // 14 days

/// Manages the on-disk page cache.
pub struct PageCache {
    root: PathBuf,
}

impl PageCache {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            root: data_dir.join("reader"),
        }
    }

    /// Returns the path for a cached page.
    pub fn page_path(
        &self,
        source: &str,
        publication: &str,
        chapter: &str,
        index: usize,
        ext: &str,
    ) -> PathBuf {
        self.root
            .join(source)
            .join(sanitize_path_component(publication))
            .join(sanitize_path_component(chapter))
            .join(format!("{index:04}.{ext}"))
    }

    /// Check if a page is cached.
    pub fn has_page(&self, path: &Path) -> bool {
        path.is_file()
            && std::fs::metadata(path)
                .map(|m| m.len() > 0)
                .unwrap_or(false)
    }

    /// Store a page atomically (write to .partial, then rename).
    pub async fn store_page(&self, path: &Path, data: &[u8]) -> Result<()> {
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .context("Failed to create cache directory")?;
        }
        let partial = path.with_extension("partial");
        let result = write_file(&partial, path, data).await;
        if result.is_err() {
            let _ = tokio::fs::remove_file(&partial).await;
        }
        result?;
        let root = self.root.join("mangadex");
        tokio::task::spawn_blocking(move || {
            crate::storage::trim_cache(&root, crate::storage::READER_CACHE_LIMIT, STALE_AFTER);
        })
        .await
        .context("Reader cache cleanup failed")?;
        Ok(())
    }

    /// Remove chapter directories that nothing has read for 14 days.
    pub fn sweep_stale(&self) {
        sweep_stale_dirs(&self.root, STALE_AFTER);
    }
}

fn sanitize_path_component(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

async fn write_file(partial: &Path, destination: &Path, bytes: &[u8]) -> Result<()> {
    use tokio::io::AsyncWriteExt;
    let mut file = tokio::fs::File::create(partial)
        .await
        .context("Failed to create temporary cache file")?;
    file.write_all(bytes)
        .await
        .context("Failed to write cache file")?;
    file.sync_all().await.context("Failed to sync cache file")?;
    tokio::fs::rename(partial, destination)
        .await
        .context("Failed to save cache file")
}

fn sweep_stale_dirs(root: &Path, stale_after: Duration) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        // Check if any file in the subtree was modified recently
        if dir_is_stale(&path, now, stale_after) {
            let _ = std::fs::remove_dir_all(&path);
        }
    }
}

fn dir_is_stale(dir: &Path, now: SystemTime, stale_after: Duration) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return true;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if !dir_is_stale(&path, now, stale_after) {
                return false;
            }
        } else if let Ok(metadata) = entry.metadata() {
            if let Ok(modified) = metadata.modified() {
                if now
                    .duration_since(modified)
                    .map(|d| d < stale_after)
                    .unwrap_or(false)
                {
                    return false;
                }
            }
        }
    }
    true
}
