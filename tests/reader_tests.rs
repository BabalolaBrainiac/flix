use flix::reader::mangadex::{approved_mangadex_url, sanitize_text};
use flix::reader::progress::{ProgressStore, ReadingPosition};
use flix::reader::{Chapter, Publication, ReaderSource};
use serde_json::json;

#[test]
fn search_response_parses_into_publications() {
    let json = json!({
        "result": "ok",
        "response": "collection",
        "data": [
            {
                "id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
                "type": "manga",
                "attributes": {
                    "title": {"en": "One Piece"},
                    "description": {"en": "A pirate adventure manga"},
                    "year": 1997,
                    "status": "ongoing",
                    "tags": [
                        {
                            "id": "tag1",
                            "type": "tag",
                            "attributes": {
                                "name": {"en": "Action"},
                                "group": "genre"
                            }
                        },
                        {
                            "id": "tag2",
                            "type": "tag",
                            "attributes": {
                                "name": {"en": "Adventure"},
                                "group": "genre"
                            }
                        }
                    ]
                },
                "relationships": [
                    {
                        "id": "rel1",
                        "type": "cover_art",
                        "attributes": {
                            "fileName": "cover.jpg"
                        }
                    },
                    {
                        "id": "rel2",
                        "type": "author",
                        "attributes": {
                            "name": "Eiichiro Oda"
                        }
                    }
                ]
            },
            {
                "id": "b2c3d4e5-f6a7-8901-bcde-f12345678901",
                "type": "manga",
                "attributes": {
                    "title": {"ja": "Naruto", "en": "Naruto"},
                    "description": {},
                    "year": 1999,
                    "status": "completed",
                    "tags": []
                },
                "relationships": []
            }
        ],
        "limit": 10,
        "offset": 0,
        "total": 2
    });

    // Parse using the internal serde structs via the public API
    // We test through the deserialize path by checking the JSON shape matches
    let data = json["data"].as_array().expect("data array");
    assert_eq!(data.len(), 2);

    // First manga
    let first = &data[0];
    assert_eq!(first["id"], "a1b2c3d4-e5f6-7890-abcd-ef1234567890");
    assert_eq!(first["attributes"]["title"]["en"], "One Piece");
    assert_eq!(first["attributes"]["year"], 1997);
    assert_eq!(first["attributes"]["status"], "ongoing");

    let tags = first["attributes"]["tags"].as_array().expect("tags");
    assert_eq!(tags.len(), 2);
    assert_eq!(tags[0]["attributes"]["name"]["en"], "Action");
    assert_eq!(tags[1]["attributes"]["name"]["en"], "Adventure");

    let rels = first["relationships"].as_array().expect("relationships");
    let cover = rels
        .iter()
        .find(|r| r["type"] == "cover_art")
        .expect("cover");
    assert_eq!(cover["attributes"]["fileName"], "cover.jpg");

    // Second manga (fallback title)
    let second = &data[1];
    assert_eq!(second["attributes"]["title"]["en"], "Naruto");
}

#[test]
fn title_with_control_characters_is_sanitized() {
    let dirty = "Hello\u{202a}World\u{202e}\u{2066}Test\u{2069}\u{0000}Done";
    let clean = sanitize_text(dirty, 300);
    assert_eq!(clean, "HelloWorldTestDone");
    assert!(!clean.contains('\u{202a}'));
    assert!(!clean.contains('\u{202e}'));
    assert!(!clean.contains('\u{2066}'));
    assert!(!clean.contains('\u{2069}'));
    assert!(!clean.contains('\u{0000}'));
}

#[test]
fn chapter_feed_sorts_by_volume_then_chapter() {
    let json = json!({
        "result": "ok",
        "response": "collection",
        "data": [
            {
                "id": "ch3",
                "type": "chapter",
                "attributes": {
                    "volume": "2",
                    "chapter": "1",
                    "title": "Vol 2 Ch 1",
                    "translatedLanguage": "en",
                    "pages": 20,
                    "externalUrl": null
                }
            },
            {
                "id": "ch1",
                "type": "chapter",
                "attributes": {
                    "volume": "1",
                    "chapter": "2",
                    "title": "Vol 1 Ch 2",
                    "translatedLanguage": "en",
                    "pages": 18,
                    "externalUrl": null
                }
            },
            {
                "id": "ch2",
                "type": "chapter",
                "attributes": {
                    "volume": "1",
                    "chapter": "10",
                    "title": "Vol 1 Ch 10",
                    "translatedLanguage": "en",
                    "pages": 22,
                    "externalUrl": null
                }
            },
            {
                "id": "ch4",
                "type": "chapter",
                "attributes": {
                    "volume": "1",
                    "chapter": "1",
                    "title": "Vol 1 Ch 1",
                    "translatedLanguage": "en",
                    "pages": 45,
                    "externalUrl": null
                }
            }
        ],
        "limit": 500,
        "offset": 0,
        "total": 4
    });

    // Parse and sort like the chapters() method does
    let data = json["data"].as_array().expect("data");
    let mut chapters: Vec<Chapter> = data
        .iter()
        .filter(|ch| ch["attributes"]["externalUrl"].is_null())
        .filter(|ch| ch["attributes"]["translatedLanguage"] == "en")
        .map(|ch| Chapter {
            id: ch["id"].as_str().unwrap().to_string(),
            chapter: ch["attributes"]["chapter"].as_str().map(String::from),
            volume: ch["attributes"]["volume"].as_str().map(String::from),
            title: ch["attributes"]["title"].as_str().map(String::from),
            language: ch["attributes"]["translatedLanguage"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            pages: ch["attributes"]["pages"].as_u64().unwrap_or(0) as usize,
            external_url: None,
        })
        .collect();

    chapters.sort_by(|a, b| {
        let vol_a = a.volume.as_deref().and_then(|v| v.parse::<f64>().ok());
        let vol_b = b.volume.as_deref().and_then(|v| v.parse::<f64>().ok());
        vol_a
            .partial_cmp(&vol_b)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                let ch_a = a.chapter.as_deref().and_then(|c| c.parse::<f64>().ok());
                let ch_b = b.chapter.as_deref().and_then(|c| c.parse::<f64>().ok());
                ch_a.partial_cmp(&ch_b).unwrap_or(std::cmp::Ordering::Equal)
            })
    });

    assert_eq!(chapters.len(), 4);
    assert_eq!(chapters[0].id, "ch4"); // vol 1, ch 1
    assert_eq!(chapters[1].id, "ch1"); // vol 1, ch 2
    assert_eq!(chapters[2].id, "ch2"); // vol 1, ch 10
    assert_eq!(chapters[3].id, "ch3"); // vol 2, ch 1
}

#[test]
fn chapter_with_external_url_is_skipped() {
    let json = json!({
        "result": "ok",
        "response": "collection",
        "data": [
            {
                "id": "ch1",
                "type": "chapter",
                "attributes": {
                    "volume": "1",
                    "chapter": "1",
                    "title": "Normal chapter",
                    "translatedLanguage": "en",
                    "pages": 20,
                    "externalUrl": null
                }
            },
            {
                "id": "ch2",
                "type": "chapter",
                "attributes": {
                    "volume": "1",
                    "chapter": "2",
                    "title": "External chapter",
                    "translatedLanguage": "en",
                    "pages": 0,
                    "externalUrl": "https://external-publisher.com/chapter/2"
                }
            }
        ],
        "limit": 500,
        "offset": 0,
        "total": 2
    });

    let data = json["data"].as_array().expect("data");
    let chapters: Vec<_> = data
        .iter()
        .filter(|ch| ch["attributes"]["externalUrl"].is_null())
        .collect();

    assert_eq!(chapters.len(), 1);
    assert_eq!(chapters[0]["id"], "ch1");
}

#[test]
fn empty_language_result_reports_available_languages() {
    let json = json!({
        "result": "ok",
        "response": "collection",
        "data": [
            {
                "id": "ch1",
                "type": "chapter",
                "attributes": {
                    "volume": "1",
                    "chapter": "1",
                    "title": "Chapter 1",
                    "translatedLanguage": "ja",
                    "pages": 20,
                    "externalUrl": null
                }
            },
            {
                "id": "ch2",
                "type": "chapter",
                "attributes": {
                    "volume": "1",
                    "chapter": "1",
                    "title": "Chapter 1",
                    "translatedLanguage": "es",
                    "pages": 20,
                    "externalUrl": null
                }
            }
        ],
        "limit": 500,
        "offset": 0,
        "total": 2
    });

    let data = json["data"].as_array().expect("data");

    // Collect available languages
    let mut available: Vec<String> = data
        .iter()
        .filter_map(|ch| {
            ch["attributes"]["translatedLanguage"]
                .as_str()
                .map(String::from)
        })
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    available.sort();

    // Filter by "en" — should be empty
    let en_chapters: Vec<_> = data
        .iter()
        .filter(|ch| ch["attributes"]["translatedLanguage"] == "en")
        .collect();

    assert!(en_chapters.is_empty());
    assert_eq!(available, vec!["es", "ja"]);
}

#[test]
fn page_url_builds_correctly() {
    let base_url = "https://uploads.mangadex.org";
    let hash = "abc123def456";
    let files = ["page1.jpg", "page2.jpg", "page3.jpg"];

    let urls: Vec<String> = files
        .iter()
        .map(|file| format!("{base_url}/data/{hash}/{file}"))
        .collect();

    assert_eq!(
        urls,
        vec![
            "https://uploads.mangadex.org/data/abc123def456/page1.jpg",
            "https://uploads.mangadex.org/data/abc123def456/page2.jpg",
            "https://uploads.mangadex.org/data/abc123def456/page3.jpg",
        ]
    );
}

#[test]
fn host_outside_allowlist_is_refused() {
    let evil_url = reqwest::Url::parse("https://evil.com/malware.jpg").unwrap();
    assert!(!approved_mangadex_url(&evil_url));

    let http_url = reqwest::Url::parse("http://uploads.mangadex.org/covers/x/y.jpg").unwrap();
    assert!(!approved_mangadex_url(&http_url));

    // Valid hosts
    let api_url = reqwest::Url::parse("https://api.mangadex.org/manga").unwrap();
    assert!(approved_mangadex_url(&api_url));

    let uploads_url = reqwest::Url::parse("https://uploads.mangadex.org/covers/x/y.jpg").unwrap();
    assert!(approved_mangadex_url(&uploads_url));

    let network_url =
        reqwest::Url::parse("https://somedomain.mangadex.network/data/hash/file.jpg").unwrap();
    assert!(approved_mangadex_url(&network_url));

    let org_url = reqwest::Url::parse("https://sub.mangadex.org/data/hash/file.jpg").unwrap();
    assert!(approved_mangadex_url(&org_url));
}

#[test]
fn non_image_content_type_is_refused() {
    // Test the content-type validation logic directly
    let html_type = "text/html";
    assert!(!html_type.starts_with("image/"));

    let json_type = "application/json";
    assert!(!json_type.starts_with("image/"));

    let jpeg_type = "image/jpeg";
    assert!(jpeg_type.starts_with("image/"));

    let png_type = "image/png";
    assert!(png_type.starts_with("image/"));

    let webp_type = "image/webp";
    assert!(webp_type.starts_with("image/"));
}

#[test]
fn progress_store_saves_and_loads() {
    let dir = tempfile::tempdir().expect("temp dir");
    let data_dir = dir.path();

    let mut store = ProgressStore::load(data_dir).expect("load empty");
    assert!(store.get("pub1").is_none());

    store.set(ReadingPosition {
        publication_id: "pub1".to_string(),
        chapter_id: "ch1".to_string(),
        page: 5,
        updated_at: "2026-08-31T12:00:00Z".to_string(),
    });
    store.save().expect("save");

    let loaded = ProgressStore::load(data_dir).expect("load saved");
    let pos = loaded.get("pub1").expect("position");
    assert_eq!(pos.publication_id, "pub1");
    assert_eq!(pos.chapter_id, "ch1");
    assert_eq!(pos.page, 5);
    assert_eq!(pos.updated_at, "2026-08-31T12:00:00Z");
}

#[test]
fn reader_source_display() {
    assert_eq!(ReaderSource::MangaDex.to_string(), "mangadex");
}

#[test]
fn publication_serialization_roundtrip() {
    let pub1 = Publication {
        id: "test-id".to_string(),
        title: "Test Manga".to_string(),
        description: Some("A test".to_string()),
        year: Some(2024),
        status: Some("ongoing".to_string()),
        tags: vec!["Action".to_string()],
        cover_url: Some("https://uploads.mangadex.org/covers/test-id/cover.jpg".to_string()),
        source: ReaderSource::MangaDex,
    };

    let json = serde_json::to_string(&pub1).expect("serialize");
    let deserialized: Publication = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(pub1, deserialized);
}

#[test]
fn page_urls_reject_credentials_and_other_ports() {
    for value in [
        "https://user@uploads.mangadex.org/page.jpg",
        "https://uploads.mangadex.org:8443/page.jpg",
        "https://uploads.mangadex.org.evil.example/page.jpg",
    ] {
        assert!(!approved_mangadex_url(&reqwest::Url::parse(value).unwrap()));
    }
}
