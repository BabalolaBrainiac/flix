use flix::desktop::{CatalogItemSummary, EpisodeQueueSeed, EpisodeSummary, PlaySource};
use flix::episode_queue::EpisodeQueue;

use flix::stremio::{
    automatic_playback_candidates, parse_stream_response, stream_quality_label, CatalogItem,
    CatalogKind, Episode,
};
use serde_json::json;

#[test]
fn ranks_native_4k_as_recommended_desktop_stream_with_fallbacks() {
    let raw = json!({
        "streams": [
            {
                "name": "1080p Stream",
                "title": "Show.S01E01.1080p.WEB-DL.x264\n👤 500 💾 2.0 GB",
                "infoHash": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            },
            {
                "name": "4K Stream 1",
                "title": "Show.S01E01.2160p.UHD.HDR.x265\n👤 50 💾 8.0 GB",
                "infoHash": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
            },
            {
                "name": "4K Stream 2",
                "title": "Show.S01E01.2160p.DV.HDR10.Remux\n👤 25 💾 15.0 GB",
                "infoHash": "cccccccccccccccccccccccccccccccccccccccc"
            },
            {
                "name": "4K Stream 3",
                "title": "Show.S01E01.4K.HEVC\n👤 10 💾 7.5 GB",
                "infoHash": "dddddddddddddddddddddddddddddddddddddddd"
            },
            {
                "name": "4K AI Upscale",
                "title": "Show.S01E01.4K.Upscaled.AI\n👤 100 💾 6.0 GB",
                "infoHash": "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"
            }
        ]
    });

    let streams = parse_stream_response(raw).unwrap();
    let candidates = automatic_playback_candidates(&streams);

    // 1. Recommended stream must be native 4K
    assert_eq!(stream_quality_label(candidates[0]), "4K");
    assert_eq!(
        candidates[0].info_hash,
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
    );

    // 2. Next candidates are up to 2 other native 4K streams
    assert_eq!(stream_quality_label(candidates[1]), "4K");
    assert_eq!(
        candidates[1].info_hash,
        "cccccccccccccccccccccccccccccccccccccccc"
    );

    assert_eq!(stream_quality_label(candidates[2]), "4K");
    assert_eq!(
        candidates[2].info_hash,
        "dddddddddddddddddddddddddddddddddddddddd"
    );

    // 3. Fallback candidate is 1080p
    assert_eq!(stream_quality_label(candidates[3]), "1080p");
    assert_eq!(
        candidates[3].info_hash,
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
}

#[test]
fn desktop_episode_queue_seed_advances_across_season_boundaries() {
    let seed = EpisodeQueueSeed {
        catalog_item: CatalogItemSummary {
            id: "tt10161886".to_string(),
            name: "Loki".to_string(),
            media_type: "series".to_string(),
            release_info: Some("2021".to_string()),
            is_anime: false,
            poster: None,
        },
        episodes: vec![
            EpisodeSummary {
                id: "tt10161886:1:6".to_string(),
                stream_id: "tt10161886:1:6".to_string(),
                imdb_id: Some("tt10161886".to_string()),
                title: Some("For All Time. Always.".to_string()),
                season: 1,
                episode: 6,
            },
            EpisodeSummary {
                id: "tt10161886:2:1".to_string(),
                stream_id: "tt10161886:2:1".to_string(),
                imdb_id: Some("tt10161886".to_string()),
                title: Some("Ouroboros".to_string()),
                season: 2,
                episode: 1,
            },
        ],
        current_episode_id: "tt10161886:1:6".to_string(),
    };

    let item = CatalogItem {
        id: seed.catalog_item.id.clone(),
        media_type: seed.catalog_item.media_type.clone(),
        name: seed.catalog_item.name.clone(),
        release_info: seed.catalog_item.release_info.clone(),
        poster: seed.catalog_item.poster.clone(),
        kind: CatalogKind::Cinemeta,
    };
    let episodes: Vec<Episode> = seed
        .episodes
        .iter()
        .map(|ep| Episode {
            id: ep.id.clone(),
            stream_id: ep.stream_id.clone(),
            imdb_id: ep.imdb_id.clone(),
            title: ep.title.clone(),
            season: ep.season,
            episode: ep.episode,
        })
        .collect();

    let mut queue = EpisodeQueue::new(item, episodes, &seed.current_episode_id).unwrap();
    assert_eq!(queue.current().season, 1);
    assert_eq!(queue.current().episode, 6);
    assert!(queue.has_next());

    // Advance across season boundary
    let advanced = queue.advance();
    assert!(advanced);
    assert_eq!(queue.current().season, 2);
    assert_eq!(queue.current().episode, 1);
    assert!(!queue.has_next());
}

#[test]
fn serializes_play_command_with_queue_and_alternatives() {
    let command = flix::desktop::PlayCommand {
        source_id: Some("src-123".to_string()),
        magnet: Some("magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567".to_string()),
        file_index: Some(0),
        media_ref: None,
        queue_seed: Some(EpisodeQueueSeed {
            catalog_item: CatalogItemSummary {
                id: "tt10161886".to_string(),
                name: "Loki".to_string(),
                media_type: "series".to_string(),
                release_info: Some("2021".to_string()),
                is_anime: false,
                poster: None,
            },
            episodes: vec![EpisodeSummary {
                id: "tt10161886:1:1".to_string(),
                stream_id: "tt10161886:1:1".to_string(),
                imdb_id: Some("tt10161886".to_string()),
                title: Some("Glorious Purpose".to_string()),
                season: 1,
                episode: 1,
            }],
            current_episode_id: "tt10161886:1:1".to_string(),
        }),
        alternatives: vec![PlaySource {
            torrent: "magnet:?xt=urn:btih:fedcba9876543210fedcba9876543210fedcba98".to_string(),
            file_index: Some(0),
            quality: "1080p".to_string(),
        }],
    };

    let json = serde_json::to_string(&command).unwrap();
    let parsed: flix::desktop::PlayCommand = serde_json::from_str(&json).unwrap();
    assert_eq!(command, parsed);
}
