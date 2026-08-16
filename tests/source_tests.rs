use flix::session::Source;

#[test]
fn classifies_supported_torrent_sources() {
    assert!(matches!(
        Source::parse("magnet:?xt=urn:btih:0123456789012345678901234567890123456789")
            .expect("valid magnet"),
        Source::Magnet(_)
    ));
    assert!(matches!(
        Source::parse("http://127.0.0.1:8899/file.torrent").expect("valid HTTP URL"),
        Source::Url(_)
    ));
    assert!(matches!(
        Source::parse("https://example.com/file.torrent").expect("valid HTTPS URL"),
        Source::Url(_)
    ));
    assert!(matches!(
        Source::parse("/tmp/file.torrent").expect("valid file path"),
        Source::File(path) if path == std::path::Path::new("/tmp/file.torrent")
    ));
}

#[test]
fn rejects_empty_and_unsupported_sources() {
    assert!(Source::parse("  ").is_err());
    assert!(Source::parse("ftp://example.com/file.torrent").is_err());
    assert!(Source::parse("not-a-torrent.txt").is_err());
}
