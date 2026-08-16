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
