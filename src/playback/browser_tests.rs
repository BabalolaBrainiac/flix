use super::*;
use crate::desktop::{DesktopServer, DesktopService};
use reqwest::{header, StatusCode};

async fn fixture(bytes: &[u8]) -> (DesktopService, tempfile::TempDir, PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let config = Config {
        data_dir: root.path().join("data"),
        download_dir: root.path().join("downloads"),
    };
    let service = DesktopService::new(config.clone());
    let coordinator = service.coordinator();
    let cache = PlaybackCache::new().unwrap();
    let cache_path = cache.path().to_owned();
    let path = cache.path().join("sample.mp4");
    std::fs::write(&path, bytes).unwrap();
    let torrent = librqbit::create_torrent(&path, librqbit::CreateTorrentOptions::default())
        .await
        .unwrap();
    let torrent_path = root.path().join("sample.torrent");
    std::fs::write(&torrent_path, torrent.as_bytes().unwrap()).unwrap();
    let session = Arc::new(TorrentSession::new(cache.path()).await.unwrap());
    let torrent_id = session.add(Source::File(torrent_path)).await.unwrap();
    let subtitle_dir = config.data_dir.join("subtitles");
    std::fs::create_dir_all(&subtitle_dir).unwrap();
    std::fs::write(
        subtitle_dir.join(format!(
            "{}.0.test.en.srt",
            session.info_hash(torrent_id).unwrap()
        )),
        "1\n00:00:00,000 --> 00:00:10,000\nFlix subtitle check\n",
    )
    .unwrap();
    let server = stream_server::serve(session.clone()).await.unwrap();
    let mut active = ActiveSession {
        target: PlaybackTarget::Browser,
        browser_seen_at: None,
        is_anime: false,
        media: MediaRef::Movie {
            catalog_id: "test".into(),
            title: "Flix browser check".into(),
        },
        videos: session.files(torrent_id).unwrap(),
        torrent_id,
        position: 0,
        player: None,
        player_name: None,
        episode_queue: None,
        quality: "Test".into(),
        cache,
        session,
        server,
    };
    coordinator
        .launch_active_session(&mut active, Vec::new(), CancellationToken::new())
        .await
        .unwrap();
    assert!(active.player.is_none());
    *coordinator.active_session.lock().await = Some(Arc::new(Mutex::new(active)));
    (service, root, cache_path)
}

fn event(id: &str, event: BrowserEvent) -> BrowserEventCommand {
    BrowserEventCommand {
        playback_id: id.to_owned(),
        event,
        position_ms: 1200,
    }
}

#[tokio::test]
async fn anime_browser_playback_fails_before_source_access() {
    let root = tempfile::tempdir().unwrap();
    let coordinator = PlaybackCoordinator::new(Config {
        data_dir: root.path().join("data"),
        download_dir: root.path().join("downloads"),
    });
    let error = coordinator
        .run_playback_pipeline(
            MediaRef::Movie {
                catalog_id: "kitsu:1".into(),
                title: "Test anime".into(),
            },
            ResolvedSource {
                is_anime: true,
                magnet: "invalid source".into(),
                file_index: None,
                quality: "Test".into(),
                alternatives: Vec::new(),
            },
            None,
            PlaybackTarget::Browser,
            CancellationToken::new(),
        )
        .await
        .unwrap_err();
    assert_eq!(safe_error_for_pipeline(&error).code, "BROWSER_UNSUPPORTED");
    assert!(coordinator.active_session.lock().await.is_none());
}

#[tokio::test]
async fn browser_stream_checks_ranges_access_and_cleanup() {
    let bytes = vec![42u8; 128 * 1024];
    let (service, _root, cache_path) = fixture(&bytes).await;
    let coordinator = service.coordinator();
    let PlaybackSnapshot::Browser {
        playback_id,
        stream_url,
        subtitle_url,
        phase,
        ..
    } = coordinator.snapshot()
    else {
        panic!("browser state");
    };
    assert_eq!(phase, BrowserPhase::Ready);
    let client = reqwest::Client::new();
    let response = client
        .get(&stream_url)
        .header(header::RANGE, "bytes=4-9")
        .header(header::ORIGIN, "http://127.0.0.1:5184")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(
        response.headers()[header::CONTENT_RANGE],
        "bytes 4-9/131072"
    );
    assert_eq!(
        response.headers()[header::ACCESS_CONTROL_ALLOW_ORIGIN],
        "http://127.0.0.1:5184"
    );
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(response.bytes().await.unwrap().as_ref(), &bytes[4..10]);
    let head = client.head(&stream_url).send().await.unwrap();
    assert_eq!(head.headers()[header::CONTENT_LENGTH], "131072");
    assert!(head.bytes().await.unwrap().is_empty());
    let suffix = client
        .get(&stream_url)
        .header(header::RANGE, "bytes=-4")
        .send()
        .await
        .unwrap();
    assert_eq!(
        suffix.bytes().await.unwrap().as_ref(),
        &bytes[bytes.len() - 4..]
    );
    assert_eq!(
        client
            .get(&stream_url)
            .header(header::RANGE, "bytes=999999-")
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::RANGE_NOT_SATISFIABLE
    );
    assert_eq!(
        client
            .get(&stream_url)
            .header(header::ORIGIN, "https://example.org")
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    let mut missing = reqwest::Url::parse(&stream_url).unwrap();
    missing.set_query(None);
    assert_eq!(
        client.get(missing).send().await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
    let subtitle = client.get(subtitle_url.unwrap()).send().await.unwrap();
    assert_eq!(
        subtitle.headers()[header::CONTENT_TYPE],
        "text/vtt; charset=utf-8"
    );
    assert!(subtitle.text().await.unwrap().starts_with("WEBVTT"));

    coordinator
        .browser_event(event(&playback_id, BrowserEvent::Playing))
        .await
        .unwrap();
    coordinator
        .browser_event(event(&playback_id, BrowserEvent::Paused))
        .await
        .unwrap();
    assert!(coordinator
        .browser_event(event("stale", BrowserEvent::Stop))
        .await
        .is_err());
    assert!(matches!(
        coordinator.snapshot(),
        PlaybackSnapshot::Browser {
            phase: BrowserPhase::Paused,
            position_ms: 1200,
            ..
        }
    ));
    let report = serde_json::to_string(&coordinator.debug_report()).unwrap();
    assert!(!report.contains(&stream_url));
    assert!(!report.contains(&playback_id));

    {
        let active = coordinator.active_session.lock().await.clone().unwrap();
        let active = active.lock().await;
        active
            .server
            .browser_media(active.torrent_id, 0, None)
            .await;
    }
    assert_eq!(
        client.get(&stream_url).send().await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
    coordinator
        .browser_event(event(&playback_id, BrowserEvent::Stop))
        .await
        .unwrap();
    assert_eq!(coordinator.snapshot(), PlaybackSnapshot::Idle);
    assert!(coordinator.active_session.lock().await.is_none());
    assert!(!cache_path.exists());
}

#[tokio::test]
async fn browser_failure_and_end_release_the_session() {
    for terminal in [BrowserEvent::Failed, BrowserEvent::Ended] {
        let (service, _root, cache_path) = fixture(&vec![42u8; 64 * 1024]).await;
        let coordinator = service.coordinator();
        let PlaybackSnapshot::Browser { playback_id, .. } = coordinator.snapshot() else {
            panic!("browser state");
        };
        coordinator
            .browser_event(event(&playback_id, terminal))
            .await
            .unwrap();
        assert!(coordinator.active_session.lock().await.is_none());
        assert!(!cache_path.exists());
        if terminal == BrowserEvent::Failed {
            assert!(
                matches!(coordinator.snapshot(), PlaybackSnapshot::Failed { error } if error.code == "BROWSER_MEDIA_FAILED")
            );
        } else {
            assert_eq!(coordinator.snapshot(), PlaybackSnapshot::Idle);
        }
    }
}

#[tokio::test]
async fn browser_disconnect_releases_the_session() {
    let (service, _root, cache_path) = fixture(&vec![42u8; 64 * 1024]).await;
    let coordinator = service.coordinator();
    let active = coordinator.active_session.lock().await.clone().unwrap();
    active.lock().await.browser_seen_at = Some(Instant::now() - BROWSER_TIMEOUT);
    drop(active);
    coordinator.monitor_player().await;
    tokio::time::timeout(Duration::from_secs(4), async {
        while !matches!(coordinator.snapshot(), PlaybackSnapshot::Failed { .. }) {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert!(!cache_path.exists());
}

#[tokio::test]
#[ignore = "requires generated video and a browser test client"]
async fn browser_smoke_host() {
    let media_path = std::env::var("FLIX_BROWSER_TEST_MEDIA").expect("Set the generated MP4 path");
    let control_path =
        std::env::var("FLIX_BROWSER_TEST_CONTROL").expect("Set a temporary control path");
    let (service, _root, _cache_path) = fixture(&std::fs::read(media_path).unwrap()).await;
    service.coordinator().monitor_player().await;
    let server = DesktopServer::bind(service).await.unwrap();
    let mut control = std::fs::OpenOptions::new();
    control.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        control.mode(0o600);
    }
    use std::io::Write;
    control
        .open(&control_path)
        .unwrap()
        .write_all(server.url().as_bytes())
        .unwrap();
    server.run().await.unwrap();
    std::fs::remove_file(control_path).unwrap();
}
