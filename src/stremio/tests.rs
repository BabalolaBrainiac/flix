use super::*;
use axum::{
    extract::State,
    http::{StatusCode, Uri},
    response::IntoResponse,
    Json, Router,
};
use serde_json::json;
use std::sync::{Arc, Mutex};

async fn provider(State(calls): State<Arc<Mutex<Vec<String>>>>, uri: Uri) -> impl IntoResponse {
    let path = uri.path();
    calls.lock().unwrap().push(path.to_string());
    if path.starts_with("/catalog/anime") || path.contains("tt404") {
        return (StatusCode::SERVICE_UNAVAILABLE, Json(json!({})));
    }
    let body = if path.starts_with("/catalog/series") {
        json!({"metas":[{"id":"tt123", "type":"series", "name":"Anime fixture"}]})
    } else if path.starts_with("/meta/series") {
        json!({"meta":{"genres":["Animation"], "country":"Japan", "videos":[{"id":"tt123:1:1", "season":1, "episode":1}]}})
    } else if path.starts_with("/meta/movie") {
        json!({"meta":{"genres":["Drama"], "country":"Japan"}})
    } else if path.starts_with("/stream") {
        json!({"streams":[
            {"infoHash":"0123456789012345678901234567890123456789", "title":"English Dub", "name":"1080p"},
            {"infoHash":"abcdefabcdefabcdefabcdefabcdefabcdefabcd", "title":"Dual Audio", "name":"1080p"}
        ]})
    } else {
        json!({"metas":[]})
    };
    (StatusCode::OK, Json(body))
}

struct Fixture {
    client: StremioClient,
    calls: Arc<Mutex<Vec<String>>>,
    task: tokio::task::JoinHandle<()>,
}

impl Fixture {
    async fn new() -> Self {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base =
            reqwest::Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
        let app = Router::new().fallback(provider).with_state(calls.clone());
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let client = StremioClient {
            client: reqwest::Client::new(),
            cinemeta_base: base.clone(),
            anime_base: base.clone(),
            stream_base: base,
            responses: Default::default(),
        };
        Self {
            client,
            calls,
            task,
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[tokio::test]
async fn general_catalog_anime_keeps_its_provider_and_enforces_language() {
    use crate::desktop::{commands, types::*};
    let fixture = Fixture::new().await;
    let directory = tempfile::tempdir().unwrap();
    let coordinator = crate::playback::PlaybackCoordinator::new(crate::config::Config {
        data_dir: directory.path().join("data"),
        download_dir: directory.path().join("downloads"),
    });
    let episodes = commands::handle_episodes(
        &fixture.client,
        &EpisodesCommand {
            item_id: "tt123".into(),
            is_anime: false,
        },
    )
    .await
    .unwrap();
    assert!(episodes.is_anime);
    assert_eq!(episodes.episodes[0].stream_id, "tt123:1:1");
    let streams = commands::handle_streams(
        &fixture.client,
        &coordinator,
        &StreamsCommand {
            media_type: Some("series".into()),
            stream_id: episodes.episodes[0].stream_id.clone(),
            is_anime: false,
        },
    )
    .await
    .unwrap();
    assert!(streams.is_anime);
    assert_eq!(streams.streams.len(), 1);
    assert!(
        coordinator
            .get_resolved_source(&streams.streams[0].source_id)
            .await
            .unwrap()
            .is_anime
    );
    commands::handle_episodes(
        &fixture.client,
        &EpisodesCommand {
            item_id: "tt123".into(),
            is_anime: true,
        },
    )
    .await
    .unwrap();
    let calls = fixture.calls.lock().unwrap();
    assert_eq!(
        calls
            .iter()
            .filter(|path| path.as_str() == "/meta/series/tt123.json")
            .count(),
        1
    );
    assert!(calls
        .iter()
        .any(|path| path == "/stream/series/tt123:1:1.json"));
}

#[tokio::test]
async fn anime_search_uses_metadata_when_the_anime_provider_fails() {
    let fixture = Fixture::new().await;
    let result = fixture
        .client
        .search_catalog("fixture", SearchCatalog::Anime)
        .await;
    assert!(result.notes.is_empty());
    assert_eq!(result.items.len(), 1);
    assert_eq!(result.items[0].kind, CatalogKind::Cinemeta);
    assert!(crate::desktop::types::CatalogItemSummary::from(&result.items[0]).is_anime);
    assert!(!fixture
        .client
        .content_is_anime("movie", "tt123")
        .await
        .unwrap());
}

#[tokio::test]
async fn metadata_failure_cannot_select_unverified_sources_or_enter_the_cache() {
    let fixture = Fixture::new().await;
    for _ in 0..2 {
        assert!(fixture
            .client
            .streams_for("series", "tt404:1:1", false)
            .await
            .is_err());
    }
    let calls = fixture.calls.lock().unwrap();
    assert_eq!(calls.len(), 2);
    assert!(calls.iter().all(|path| path.starts_with("/meta/")));
}
