use flix::library::{Entry, Library, Sort, SourceSerde};
use flix::meta::parse::parse;
use std::time::SystemTime;

#[test]
fn test_meta_name_parsing() {
    let res = parse("Big.Buck.Bunny.2008.1080p.mkv");
    assert_eq!(res.title, "Big Buck Bunny");
    assert_eq!(res.year, Some(2008));
    assert_eq!(res.season, None);
    assert_eq!(res.episode, None);

    let tv = parse("Silo.S01E03.1080p.WEB.H264.mkv");
    assert_eq!(tv.title, "Silo");
    assert_eq!(tv.season, Some(1));
    assert_eq!(tv.episode, Some(3));

    let torrent = parse("/tmp/Big.Buck.Bunny.2008.1080p.mkv.torrent");
    assert_eq!(torrent.title, "Big Buck Bunny");
    assert_eq!(torrent.year, Some(2008));

    let magnet = parse(
        "magnet:?xt=urn:btih:0123456789012345678901234567890123456789&dn=Open.Movie.2024.1080p",
    );
    assert_eq!(magnet.title, "Open Movie");
    assert_eq!(magnet.year, Some(2024));
}

#[test]
fn test_library_persistence_and_sorting() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let mut lib = Library::load(tmp_dir.path()).unwrap();

    let e1 = Entry {
        info_hash: "hash1".to_string(),
        display_name: "Zebra".to_string(),
        source: SourceSerde::Magnet("magnet:?xt=urn:btih:1".to_string()),
        added_at: SystemTime::now(),
        last_played: None,
        meta: None,
        output_name: None,
    };

    let e2 = Entry {
        info_hash: "hash2".to_string(),
        display_name: "Alpha".to_string(),
        source: SourceSerde::Magnet("magnet:?xt=urn:btih:2".to_string()),
        added_at: SystemTime::now() + std::time::Duration::from_secs(10),
        last_played: None,
        meta: None,
        output_name: None,
    };

    lib.upsert(e1);
    lib.upsert(e2);
    lib.save().unwrap();

    // Reload from disk
    let reloaded = Library::load(tmp_dir.path()).unwrap();
    let sorted_by_title = reloaded.sorted(Sort::TitleAsc);
    assert_eq!(sorted_by_title[0].display_name, "Alpha");
    assert_eq!(sorted_by_title[1].display_name, "Zebra");
}
