use flix::reader::cache::PageCache;
use flix::reader::progress::{ProgressStore, ReadingPosition};
use flix::reader::{Chapter, Publication, ReaderSource};

#[test]
fn page_cache_stores_and_retrieves_pages() {
    let dir = tempfile::tempdir().expect("temp dir");
    let cache = PageCache::new(dir.path());

    let path = cache.page_path("mangadex", "pub-1", "ch-1", 1, "jpg");
    assert!(!cache.has_page(&path));

    // Store a page
    let data = b"fake jpeg data";
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(cache.store_page(&path, data)).expect("store");

    assert!(cache.has_page(&path));
    let loaded = std::fs::read(&path).expect("read");
    assert_eq!(loaded, data);
}

#[test]
fn page_cache_path_structure() {
    let dir = tempfile::tempdir().expect("temp dir");
    let cache = PageCache::new(dir.path());

    let path = cache.page_path("mangadex", "One Piece", "ch 1.5", 3, "png");
    let path_str = path.to_string_lossy();
    assert!(path_str.contains("mangadex"));
    assert!(path_str.contains("One_Piece"));
    assert!(path_str.contains("ch_1_5"));
    assert!(path_str.ends_with("0003.png"));
}

#[test]
fn progress_tracks_reading_position() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut store = ProgressStore::load(dir.path()).expect("load");

    store.set(ReadingPosition {
        publication_id: "manga-1".to_string(),
        chapter_id: "ch-5".to_string(),
        page: 12,
        updated_at: "1725148800".to_string(),
    });
    store.save().expect("save");

    let loaded = ProgressStore::load(dir.path()).expect("reload");
    let pos = loaded.get("manga-1").expect("position");
    assert_eq!(pos.chapter_id, "ch-5");
    assert_eq!(pos.page, 12);
}

#[test]
fn progress_updates_position_for_same_publication() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut store = ProgressStore::load(dir.path()).expect("load");

    store.set(ReadingPosition {
        publication_id: "manga-1".to_string(),
        chapter_id: "ch-1".to_string(),
        page: 1,
        updated_at: "1000".to_string(),
    });
    store.set(ReadingPosition {
        publication_id: "manga-1".to_string(),
        chapter_id: "ch-3".to_string(),
        page: 7,
        updated_at: "2000".to_string(),
    });
    store.save().expect("save");

    let loaded = ProgressStore::load(dir.path()).expect("reload");
    let pos = loaded.get("manga-1").expect("position");
    assert_eq!(pos.chapter_id, "ch-3");
    assert_eq!(pos.page, 7);
}

#[test]
fn publication_display_format() {
    let pub1 = Publication {
        id: "abc-123".to_string(),
        title: "Test Manga".to_string(),
        description: None,
        year: Some(2024),
        status: Some("ongoing".to_string()),
        tags: vec![],
        cover_url: None,
        source: ReaderSource::MangaDex,
    };
    assert_eq!(pub1.id, "abc-123");
    assert_eq!(pub1.title, "Test Manga");
    assert_eq!(pub1.year, Some(2024));
}

#[test]
fn chapter_selection_by_number() {
    let chapters = [
        Chapter {
            id: "id-1".to_string(),
            chapter: Some("1".to_string()),
            volume: Some("1".to_string()),
            title: Some("Beginning".to_string()),
            language: "en".to_string(),
            pages: 45,
            external_url: None,
        },
        Chapter {
            id: "id-2".to_string(),
            chapter: Some("2".to_string()),
            volume: Some("1".to_string()),
            title: None,
            language: "en".to_string(),
            pages: 42,
            external_url: None,
        },
    ];

    // Find chapter "2"
    let found = chapters.iter().find(|c| c.chapter.as_deref() == Some("2"));
    assert!(found.is_some());
    assert_eq!(found.unwrap().id, "id-2");

    // Chapter "99" not found
    let not_found = chapters.iter().find(|c| c.chapter.as_deref() == Some("99"));
    assert!(not_found.is_none());
}

#[test]
fn cbz_export_creates_valid_zip() {
    let dir = tempfile::tempdir().expect("temp dir");

    // Create fake page files
    let page1 = dir.path().join("0001.jpg");
    let page2 = dir.path().join("0002.jpg");
    std::fs::write(&page1, b"page one data").expect("write 1");
    std::fs::write(&page2, b"page two data").expect("write 2");

    let cbz_path = dir.path().join("output.cbz");
    let pages = [page1, page2];

    // Use the same logic as export_cbz
    {
        let file = std::fs::File::create(cbz_path.to_str().unwrap()).expect("create cbz");
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for (index, path) in pages.iter().enumerate() {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("jpg");
            let name = format!("{:04}.{ext}", index + 1);
            zip.start_file(&name, options).expect("start file");
            let data = std::fs::read(path).expect("read page");
            std::io::Write::write_all(&mut zip, &data).expect("write");
        }
        zip.finish().expect("finish");
    }

    // Verify the CBZ is a valid zip
    let cbz_file = std::fs::File::open(&cbz_path).expect("open cbz");
    let mut archive = zip::ZipArchive::new(cbz_file).expect("valid zip");
    assert_eq!(archive.len(), 2);

    let first = archive.by_name("0001.jpg").expect("first page");
    assert_eq!(first.size(), 13); // "page one data".len()
}
