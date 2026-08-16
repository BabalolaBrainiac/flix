use flix::config::Config;
use flix::desktop::{
    handle_downloads, handle_search, handle_settings, CatalogItemSummary, DesktopService,
    DownloadsAction, DownloadsCommand, EpisodeQueueSeed, EpisodeSummary, EpisodesCommand,
    PlaybackState, SearchCommand, SettingsAction, SettingsCommand, StopPlaybackCommand,
    StreamsCommand,
};
use flix::stremio::{CatalogItem, CatalogKind, Episode, StremioClient};
use tempfile::TempDir;

#[test]
fn serializes_and_deserializes_desktop_commands() {
    let search = SearchCommand {
        query: "Loki".to_string(),
        anime_only: false,
    };
    let json = serde_json::to_string(&search).unwrap();
    let parsed: SearchCommand = serde_json::from_str(&json).unwrap();
    assert_eq!(search, parsed);

    let episodes = EpisodesCommand {
        item_id: "tt10161886".to_string(),
        is_anime: false,
    };
    let json = serde_json::to_string(&episodes).unwrap();
    let parsed: EpisodesCommand = serde_json::from_str(&json).unwrap();
    assert_eq!(episodes, parsed);

    let streams = StreamsCommand {
        media_type: "series".to_string(),
        stream_id: "tt10161886:1:1".to_string(),
    };
    let json = serde_json::to_string(&streams).unwrap();
    let parsed: StreamsCommand = serde_json::from_str(&json).unwrap();
    assert_eq!(streams, parsed);
}

#[tokio::test]
async fn rejects_empty_search_query() {
    let client = StremioClient::from_env().unwrap();
    let command = SearchCommand {
        query: "   ".to_string(),
        anime_only: false,
    };
    let result = handle_search(&client, &command).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("cannot be empty"));
}

#[test]
fn handles_settings_and_player_detection() {
    let temp = TempDir::new().unwrap();
    let config = Config {
        download_dir: temp.path().join("downloads"),
        data_dir: temp.path().join("data"),
    };
    let command = SettingsCommand {
        action: SettingsAction::Get,
    };
    let response = handle_settings(&config, &command).unwrap();
    assert_eq!(
        response.download_dir,
        config.download_dir.display().to_string()
    );
    assert_eq!(response.data_dir, config.data_dir.display().to_string());
}

#[tokio::test]
async fn handles_downloads_listing_and_library_queries() {
    let temp = TempDir::new().unwrap();
    let data_dir = temp.path().join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    let config = Config {
        download_dir: temp.path().join("downloads"),
        data_dir,
    };

    let list_command = DownloadsCommand {
        action: DownloadsAction::List,
    };
    let response = handle_downloads(&config, &list_command).await.unwrap();
    assert!(response.entries.is_empty());
}

#[tokio::test]
async fn desktop_service_initializes_with_idle_state_and_cleans_up() {
    let temp = TempDir::new().unwrap();
    let config = Config {
        download_dir: temp.path().join("downloads"),
        data_dir: temp.path().join("data"),
    };
    let mut service = DesktopService::new(config);
    let status = service.status().await.unwrap();
    assert_eq!(status.state, PlaybackState::Idle);

    let stop_res = service.stop(StopPlaybackCommand).await.unwrap();
    assert!(stop_res.success);
}

#[test]
fn converts_catalog_items_and_episodes_to_desktop_summaries() {
    let item = CatalogItem {
        id: "tt12345".to_string(),
        media_type: "series".to_string(),
        name: "Test Title".to_string(),
        release_info: Some("2024".to_string()),
        kind: CatalogKind::Cinemeta,
    };
    let summary = CatalogItemSummary::from(&item);
    assert_eq!(summary.id, "tt12345");
    assert_eq!(summary.name, "Test Title");
    assert!(!summary.is_anime);

    let ep = Episode {
        id: "tt12345:1:1".to_string(),
        title: Some("Pilot".to_string()),
        season: 1,
        episode: 1,
    };
    let ep_summary = EpisodeSummary::from(&ep);
    assert_eq!(ep_summary.id, "tt12345:1:1");
    assert_eq!(ep_summary.season, 1);
    assert_eq!(ep_summary.episode, 1);
}

#[test]
fn reconstructs_episode_queue_seed() {
    let seed = EpisodeQueueSeed {
        catalog_item: CatalogItemSummary {
            id: "tt12345".to_string(),
            name: "Test Show".to_string(),
            media_type: "series".to_string(),
            release_info: Some("2024".to_string()),
            is_anime: false,
        },
        episodes: vec![
            EpisodeSummary {
                id: "tt12345:1:1".to_string(),
                title: Some("Pilot".to_string()),
                season: 1,
                episode: 1,
            },
            EpisodeSummary {
                id: "tt12345:1:2".to_string(),
                title: Some("Episode 2".to_string()),
                season: 1,
                episode: 2,
            },
        ],
        current_episode_id: "tt12345:1:1".to_string(),
    };

    assert_eq!(seed.episodes.len(), 2);
    assert_eq!(seed.current_episode_id, "tt12345:1:1");
}
