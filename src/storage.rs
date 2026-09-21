use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

pub const READER_CACHE_LIMIT: u64 = 128 * 1024 * 1024;
pub const SUBTITLE_CACHE_LIMIT: u64 = 32 * 1024 * 1024;
pub const POSTER_CACHE_LIMIT: u64 = 64 * 1024 * 1024;
pub const CACHE_AGE: Duration = Duration::from_secs(7 * 24 * 3600);

struct CachedFile {
    path: PathBuf,
    bytes: u64,
    modified: SystemTime,
}

pub fn trim_media_caches(data_dir: &Path) {
    trim_cache(
        &data_dir.join("reader").join("mangadex"),
        READER_CACHE_LIMIT,
        CACHE_AGE,
    );
    trim_cache(&data_dir.join("subtitles"), SUBTITLE_CACHE_LIMIT, CACHE_AGE);
    trim_cache(&data_dir.join("posters"), POSTER_CACHE_LIMIT, CACHE_AGE);
}

pub fn trim_cache(root: &Path, limit: u64, max_age: Duration) {
    let mut files = Vec::new();
    collect_files(root, &mut files, 0);
    files.sort_by_key(|file| file.modified);
    let mut total: u64 = files.iter().map(|file| file.bytes).sum();
    let now = SystemTime::now();
    for file in files {
        let age = now.duration_since(file.modified).unwrap_or_default();
        if (total > limit || age > max_age) && std::fs::remove_file(file.path).is_ok() {
            total = total.saturating_sub(file.bytes);
        }
    }
    remove_empty_directories(root, 0);
}

fn collect_files(root: &Path, files: &mut Vec<CachedFile>, depth: usize) {
    if depth > 5
        || root
            .symlink_metadata()
            .is_ok_and(|meta| meta.file_type().is_symlink())
    {
        return;
    }
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            collect_files(&entry.path(), files, depth + 1);
        } else if kind.is_file() {
            let Ok(meta) = entry.metadata() else { continue };
            files.push(CachedFile {
                path: entry.path(),
                bytes: meta.len(),
                modified: meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            });
        }
    }
}

fn remove_empty_directories(root: &Path, depth: usize) {
    if depth > 5
        || root
            .symlink_metadata()
            .is_ok_and(|meta| meta.file_type().is_symlink())
    {
        return;
    }
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            remove_empty_directories(&entry.path(), depth + 1);
            let _ = std::fs::remove_dir(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_cache_bytes_without_removing_reading_progress() {
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("reader/mangadex/chapter");
        std::fs::create_dir_all(&cache).unwrap();
        std::fs::write(cache.join("1.jpg"), [0; 8]).unwrap();
        std::fs::write(cache.join("2.jpg"), [0; 8]).unwrap();
        let progress = dir.path().join("reader/progress.json");
        std::fs::write(&progress, b"{}").unwrap();
        trim_cache(&dir.path().join("reader/mangadex"), 8, CACHE_AGE);
        assert_eq!(std::fs::read_dir(cache).unwrap().count(), 1);
        assert!(progress.exists());
    }

    #[cfg(unix)]
    #[test]
    fn does_not_follow_a_link_outside_the_cache() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let file = outside.path().join("keep.jpg");
        std::fs::write(&file, b"keep").unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join("link")).unwrap();
        trim_cache(dir.path(), 0, Duration::ZERO);
        assert!(file.exists());
    }
}
