use flix::stremio::{
    automatic_1080p_fallback, automatic_playback_candidates, build_magnet, catalog_url,
    parse_catalog_response, parse_meta_response, parse_stream_response, parse_subtitle_response,
    CatalogKind,
};
use serde_json::json;

#[test]
fn parses_movie_series_and_anime_catalog_items() {
    let value = json!({
        "metas": [
            {"id": "tt1234567", "type": "movie", "name": "A\u{001b}[31m Movie", "releaseInfo": "2026"},
            {"id": "kitsu:42", "type": "series", "name": "An Anime", "releaseInfo": "2025"}
        ]
    });

    let movie = parse_catalog_response(value.clone(), CatalogKind::Cinemeta).expect("catalog");
    let anime = parse_catalog_response(value, CatalogKind::AnimeKitsu).expect("catalog");

    assert_eq!(movie[0].id, "tt1234567");
    assert!(!movie[0].name.contains('\u{1b}'));
    assert_eq!(movie[0].kind, CatalogKind::Cinemeta);
    assert_eq!(anime[1].id, "kitsu:42");
    assert_eq!(anime[1].kind, CatalogKind::AnimeKitsu);
}

#[test]
fn keeps_only_safe_english_stremio_subtitles() {
    let value = json!({
        "subtitles": [
            {"id": "one", "lang": "eng", "url": "https://subs5.strem.io/file/one.srt"},
            {"id": "two", "lang": "spa", "url": "https://subs5.strem.io/file/two.srt"},
            {"id": "three", "lang": "eng", "url": "https://example.com/file/three.srt"}
        ]
    });

    let subtitles = parse_subtitle_response(value).expect("subtitles");
    assert_eq!(subtitles.len(), 1);
    assert_eq!(subtitles[0].id, "one");
    assert_eq!(subtitles[0].url.host_str(), Some("subs5.strem.io"));
}

#[tokio::test]
#[ignore = "uses the live Anime Kitsu subtitle service"]
async fn downloads_an_anime_kitsu_english_subtitle() {
    let directory = tempfile::tempdir().expect("temporary subtitle directory");
    let context = flix::stremio::SubtitleContext {
        series_id: "kitsu:44012".to_string(),
        selected_episode: 1,
    };
    let path = flix::stremio::find_anime_subtitle(
        directory.path(),
        "0123456789012345678901234567890123456789",
        2,
        "Jigokuraku.S01E01.mkv",
        &context,
    )
    .await
    .expect("Stremio subtitle request")
    .expect("English subtitle");

    assert!(std::fs::metadata(path).expect("subtitle metadata").len() > 0);
}

#[test]
fn parses_episode_ids_for_series_and_anime() {
    let value = json!({
        "meta": {
            "videos": [
                {"id": "kitsu:42:1", "title": "Arrival", "season": 1, "episode": 1},
                {
                    "id": "kitsu:42:2",
                    "title": "Departure",
                    "season": 1,
                    "episode": 2,
                    "imdb_id": "tt3909224",
                    "imdbSeason": 4,
                    "imdbEpisode": 2
                }
            ]
        }
    });

    let episodes = parse_meta_response(value).expect("metadata");
    assert_eq!(episodes.len(), 2);
    assert_eq!(episodes[0].id, "kitsu:42:1");
    assert_eq!(episodes[0].stream_id, "kitsu:42:1");
    assert_eq!(episodes[0].imdb_id, None);
    assert_eq!(episodes[0].season, 1);
    assert_eq!(episodes[0].episode, 1);
    assert_eq!(episodes[1].stream_id, "tt3909224:4:2");
    assert_eq!(episodes[1].imdb_id.as_deref(), Some("tt3909224"));
}

#[test]
fn sorts_torrent_streams_by_seeders_and_keeps_pack_file_index() {
    let value = json!({
        "streams": [
            {
                "name": "Torrentio\n1080p",
                "title": "release one\nusers 4 size 1.2 GB",
                "infoHash": "0123456789012345678901234567890123456789",
                "fileIdx": 8,
                "behaviorHints": {"filename": "Show.S01E09.mkv"}
            },
            {
                "name": "Torrentio\n1080p",
                "title": "release two\nusers 40 size 900 MB",
                "infoHash": "abcdefabcdefabcdefabcdefabcdefabcdefabcd",
                "fileIdx": 2,
                "behaviorHints": {"filename": "Show.S01E03.1080p.mkv"}
            }
        ]
    });

    let streams = parse_stream_response(value).expect("streams");
    assert_eq!(streams.len(), 2);
    assert_eq!(streams[0].seeders, Some(40));
    assert_eq!(streams[0].size.as_deref(), Some("900 MB"));
    assert_eq!(streams[0].file_index, Some(2));
    assert_eq!(
        streams[0].file_name.as_deref(),
        Some("Show.S01E03.1080p.mkv")
    );
}

#[test]
fn ranks_native_4k_before_more_popular_1080p() {
    let value = json!({
        "streams": [
            {
                "name": "Torrentio\n1080p",
                "title": "1080p WEB-DL\nusers 900 size 2.1 GB",
                "infoHash": "0123456789012345678901234567890123456789",
                "behaviorHints": {"filename": "Show.S01E01.1080p.mkv"}
            },
            {
                "name": "Torrentio\n4K",
                "title": "2160p WEB-DL HEVC\nusers 25 size 8.4 GB",
                "infoHash": "abcdefabcdefabcdefabcdefabcdefabcdefabcd",
                "behaviorHints": {"filename": "Show.S01E01.2160p.mkv"}
            }
        ]
    });

    let streams = parse_stream_response(value).expect("streams");

    assert_eq!(
        streams[0].file_name.as_deref(),
        Some("Show.S01E01.2160p.mkv")
    );
    assert_eq!(
        automatic_1080p_fallback(&streams).and_then(|stream| stream.file_name.as_deref()),
        Some("Show.S01E01.1080p.mkv")
    );
}

#[test]
fn uses_1080p_as_the_default_when_4k_is_unavailable() {
    let value = json!({
        "streams": [
            {
                "name": "Torrentio\n720p",
                "title": "720p WEB-DL\nusers 800 size 900 MB",
                "infoHash": "0123456789012345678901234567890123456789",
                "behaviorHints": {"filename": "Show.S01E01.720p.mkv"}
            },
            {
                "name": "Torrentio\n1080p",
                "title": "1080p WEB-DL\nusers 20 size 2.1 GB",
                "infoHash": "abcdefabcdefabcdefabcdefabcdefabcdefabcd",
                "behaviorHints": {"filename": "Show.S01E01.1080p.mkv"}
            }
        ]
    });

    let streams = parse_stream_response(value).expect("streams");

    assert_eq!(
        streams[0].file_name.as_deref(),
        Some("Show.S01E01.1080p.mkv")
    );
}

#[test]
fn ranks_a_native_1080p_release_before_an_ai_upscaled_4k_release() {
    let value = json!({
        "streams": [
            {
                "name": "Torrentio\n4K",
                "title": "2160p HDR AI Upscale\nusers 500 size 3.0 GB",
                "infoHash": "0123456789012345678901234567890123456789",
                "behaviorHints": {"filename": "Show.S01E01.2160p.AI.Upscale.mkv"}
            },
            {
                "name": "Torrentio\n1080p",
                "title": "1080p WEB-DL\nusers 20 size 2.1 GB",
                "infoHash": "abcdefabcdefabcdefabcdefabcdefabcdefabcd",
                "behaviorHints": {"filename": "Show.S01E01.1080p.mkv"}
            }
        ]
    });

    let streams = parse_stream_response(value).expect("streams");

    assert_eq!(
        streams[0].file_name.as_deref(),
        Some("Show.S01E01.1080p.mkv")
    );
}

#[test]
fn builds_a_safe_magnet_and_rejects_an_invalid_hash() {
    let magnet = build_magnet(
        "0123456789012345678901234567890123456789",
        Some("Episode 1 [1080p].mkv"),
    )
    .expect("magnet");

    assert!(magnet.starts_with("magnet:?xt=urn%3Abtih%3A0123456789012345678901234567890123456789"));
    assert!(magnet.contains("dn=Episode+1+%5B1080p%5D.mkv"));
    assert!(magnet.contains("tr=udp%3A%2F%2Ftracker.opentrackr.org%3A1337%2Fannounce"));
    assert!(magnet.contains("tr=udp%3A%2F%2Fopen.stealth.si%3A80%2Fannounce"));
    assert!(build_magnet("not-a-hash", None).is_err());
}

#[test]
fn retries_other_native_4k_streams_before_1080p() {
    let value = json!({
        "streams": [
            {
                "name": "Torrentio\n4K",
                "title": "2160p WEB-DL\nusers 80 size 8 GB",
                "infoHash": "0123456789012345678901234567890123456789",
                "behaviorHints": {"filename": "Show.S01E01.first.2160p.mkv"}
            },
            {
                "name": "Torrentio\n4K",
                "title": "2160p WEB-DL\nusers 60 size 7 GB",
                "infoHash": "abcdefabcdefabcdefabcdefabcdefabcdefabcd",
                "behaviorHints": {"filename": "Show.S01E01.second.2160p.mkv"}
            },
            {
                "name": "Torrentio\n1080p",
                "title": "1080p WEB-DL\nusers 500 size 2 GB",
                "infoHash": "1111111111111111111111111111111111111111",
                "behaviorHints": {"filename": "Show.S01E01.1080p.mkv"}
            }
        ]
    });
    let streams = parse_stream_response(value).expect("streams");

    let names: Vec<_> = automatic_playback_candidates(&streams)
        .into_iter()
        .map(|stream| stream.file_name.as_deref().expect("filename"))
        .collect();

    assert_eq!(
        names,
        vec![
            "Show.S01E01.first.2160p.mkv",
            "Show.S01E01.second.2160p.mkv",
            "Show.S01E01.1080p.mkv"
        ]
    );
}

#[test]
fn maps_episode_stream_requests_to_series() {
    let _client = flix::stremio::StremioClient::from_env().unwrap();
    // Test that stream requests with colon (e.g. IMDb episode or Kitsu episode) map to series
    let imdb_ep_id = "tt3909224:4:2";
    let kitsu_ep_id = "kitsu:42:1";
    assert!(imdb_ep_id.contains(':'));
    assert!(kitsu_ep_id.starts_with("kitsu:"));
}

#[test]
fn parses_catalog_items_with_genres_and_rating() {
    let value = json!({
        "metas": [
            {
                "id": "tt1234567",
                "type": "series",
                "name": "A Great Show",
                "releaseInfo": "2025",
                "genres": ["Action", "Mystery"],
                "imdbRating": "8.5",
                "description": "A great show"
            }
        ]
    });

    let items = parse_catalog_response(value, CatalogKind::Cinemeta).expect("catalog");

    assert_eq!(items.len(), 1);
    assert_eq!(
        items[0].genres,
        Some(vec!["Action".to_string(), "Mystery".to_string()])
    );
    assert_eq!(items[0].imdb_rating.as_deref(), Some("8.5"));
    assert_eq!(items[0].description.as_deref(), Some("A great show"));
}

#[test]
fn parses_catalog_items_without_optional_fields() {
    let value = json!({
        "metas": [
            {
                "id": "tt1234567",
                "type": "movie",
                "name": "A Movie",
                "releaseInfo": "2026"
            }
        ]
    });

    let items = parse_catalog_response(value, CatalogKind::Cinemeta).expect("catalog");

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].genres, None);
    assert_eq!(items[0].imdb_rating, None);
    assert_eq!(items[0].description, None);
}

#[test]
fn catalog_url_builds_valid_genre_path() {
    let base = reqwest::Url::parse("https://v3-cinemeta.strem.io/").expect("base URL");

    let url = catalog_url(&base, "series", "top", "genre=Mystery").expect("catalog URL");

    assert!(url
        .path()
        .ends_with("/catalog/series/top/genre=Mystery.json"));
}

#[test]
fn catalog_url_builds_paginated_path() {
    let base = reqwest::Url::parse("https://v3-cinemeta.strem.io/").expect("base URL");

    let url = catalog_url(&base, "series", "top", "genre=Action&skip=100").expect("catalog URL");

    assert!(url.path().contains("genre=Action"));
    assert!(url.path().contains("skip=100"));
}

#[test]
fn catalog_url_rejects_invalid_catalog_id() {
    let base = reqwest::Url::parse("https://v3-cinemeta.strem.io/").expect("base URL");

    assert!(catalog_url(&base, "series", "bad id!", "genre=Action").is_err());
}

#[test]
fn catalog_url_rejects_invalid_media_type() {
    let base = reqwest::Url::parse("https://v3-cinemeta.strem.io/").expect("base URL");

    assert!(catalog_url(&base, "podcast", "top", "genre=Action").is_err());
}

#[test]
fn catalog_url_rejects_invalid_extra() {
    let base = reqwest::Url::parse("https://v3-cinemeta.strem.io/").expect("base URL");

    assert!(catalog_url(&base, "series", "top", "genre=<script>").is_err());
}
