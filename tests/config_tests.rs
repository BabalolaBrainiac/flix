use flix::config::PlaybackCache;

#[test]
fn playback_cache_is_removed_when_dropped() {
    let path = {
        let cache = PlaybackCache::new().expect("playback cache");
        let path = cache.path().to_path_buf();
        assert!(path.is_dir());
        std::fs::write(path.join("piece.bin"), b"torrent data").expect("cache fixture");
        path
    };

    assert!(!path.exists());
}

#[cfg(unix)]
#[test]
fn cleanup_keeps_an_active_cache_and_removes_an_abandoned_cache() {
    use std::fs::{File, FileTimes, OpenOptions};
    use std::time::{Duration, SystemTime};
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("flix-play-test");
    std::fs::create_dir(&path).unwrap();
    let owner = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(path.join("owner.lock"))
        .unwrap();
    owner.try_lock().unwrap();
    let old = SystemTime::now() - Duration::from_secs(7200);
    File::open(&path)
        .unwrap()
        .set_times(FileTimes::new().set_modified(old))
        .unwrap();
    flix::config::sweep_playback_caches(directory.path());
    assert!(path.exists());
    drop(owner);
    flix::config::sweep_playback_caches(directory.path());
    assert!(!path.exists());
}
