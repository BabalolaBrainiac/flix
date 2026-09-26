use flix::config::Config;
use flix::desktop::{DesktopServer, DesktopService};
use reqwest::header::{AUTHORIZATION, ORIGIN};
use reqwest::StatusCode;
use tempfile::TempDir;

#[tokio::test]
async fn browser_events_require_auth_and_reject_stale_sessions() {
    let root = TempDir::new().unwrap();
    let server = DesktopServer::bind(
        DesktopService::new(Config {
            download_dir: root.path().join("downloads"),
            data_dir: root.path().join("data"),
        })
        .unwrap(),
    )
    .await
    .unwrap();
    let access_token = server.state.token.clone();
    let url = format!("{}/api/browser/event", server.local_url());
    let shutdown = server.state.shutdown_tx.clone();
    let task = tokio::spawn(server.run());
    let client = reqwest::Client::new();
    let body = serde_json::json!({"playback_id": "stale", "event": "stop"});
    assert_eq!(
        client.post(&url).json(&body).send().await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        client
            .post(&url)
            .header("X-Flix-Token", &access_token)
            .header(ORIGIN, "https://example.org")
            .json(&body)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        client
            .post(&url)
            .header("X-Flix-Token", &access_token)
            .json(&body)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );
    shutdown.send(()).unwrap();
    task.await.unwrap().unwrap();
}

#[tokio::test]
async fn rejects_api_requests_without_or_with_invalid_token() {
    let temp = TempDir::new().unwrap();
    let config = Config {
        download_dir: temp.path().join("downloads"),
        data_dir: temp.path().join("data"),
    };
    let service = DesktopService::new(config).unwrap();
    let server = DesktopServer::bind(service).await.unwrap();
    let port = server.state.port;
    let token = server.state.token.clone();

    tokio::spawn(async move {
        let _ = server.run().await;
    });

    // Wait a brief moment for server to accept connections
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let client = reqwest::Client::new();
    let health_url = format!("http://127.0.0.1:{}/api/health", port);

    // 1. Missing token
    let res_no_token = client.get(&health_url).send().await.unwrap();
    assert_eq!(res_no_token.status(), StatusCode::UNAUTHORIZED);

    // 2. Invalid token
    let res_invalid = client
        .get(&health_url)
        .header(AUTHORIZATION, "Bearer invalid-token")
        .send()
        .await
        .unwrap();
    assert_eq!(res_invalid.status(), StatusCode::UNAUTHORIZED);

    // 3. Valid Authorization: Bearer
    let res_bearer = client
        .get(&health_url)
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res_bearer.status(), StatusCode::OK);
    let body: serde_json::Value = res_bearer.json().await.unwrap();
    assert_eq!(body["status"], "ok");

    // 4. Valid X-Flix-Token
    let res_custom = client
        .get(&health_url)
        .header("X-Flix-Token", &token)
        .send()
        .await
        .unwrap();
    assert_eq!(res_custom.status(), StatusCode::OK);

    // 5. Foreign origin rejected
    let res_foreign = client
        .get(&health_url)
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .header(ORIGIN, "http://malicious-site.com")
        .send()
        .await
        .unwrap();
    assert_eq!(res_foreign.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn serves_embedded_static_assets_and_spa_fallback() {
    let temp = TempDir::new().unwrap();
    let config = Config {
        download_dir: temp.path().join("downloads"),
        data_dir: temp.path().join("data"),
    };
    let service = DesktopService::new(config).unwrap();
    let server = DesktopServer::bind(service).await.unwrap();
    let port = server.state.port;

    tokio::spawn(async move {
        let _ = server.run().await;
    });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let client = reqwest::Client::new();
    let root_url = format!("http://127.0.0.1:{}", port);

    let res = client.get(&root_url).send().await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = res.text().await.unwrap();
    assert!(body.contains("Flix") || body.contains("app"));

    // SPA client route fallback
    let spa_url = format!("http://127.0.0.1:{}/search/detail/123", port);
    let spa_res = client.get(&spa_url).send().await.unwrap();
    assert_eq!(spa_res.status(), StatusCode::OK);
    let spa_body = spa_res.text().await.unwrap();
    assert!(spa_body.contains("<!DOCTYPE html>"));
}

#[tokio::test]
async fn recommend_endpoint_requires_auth() {
    let temp = TempDir::new().unwrap();
    let config = Config {
        download_dir: temp.path().join("downloads"),
        data_dir: temp.path().join("data"),
    };
    let service = DesktopService::new(config).unwrap();
    let server = DesktopServer::bind(service).await.unwrap();
    let port = server.state.port;
    let token = server.state.token.clone();

    tokio::spawn(async move {
        let _ = server.run().await;
    });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{}/api/recommend", port);

    // 1. No token → 401
    let res = client
        .post(&url)
        .json(&serde_json::json!({"keywords": "action", "limit": 5}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    // 2. Valid token → 200 or 500 (500 is fine if no network)
    let res = client
        .post(&url)
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&serde_json::json!({"keywords": "action", "limit": 5}))
        .send()
        .await
        .unwrap();
    let status = res.status();
    assert!(
        status == StatusCode::OK || status == StatusCode::INTERNAL_SERVER_ERROR,
        "Expected 200 or 500, got {status}",
    );
}

#[tokio::test]
async fn quit_endpoint_signals_graceful_shutdown() {
    let temp = TempDir::new().unwrap();
    let config = Config {
        download_dir: temp.path().join("downloads"),
        data_dir: temp.path().join("data"),
    };
    let service = DesktopService::new(config).unwrap();
    let server = DesktopServer::bind(service).await.unwrap();
    let port = server.state.port;
    let token = server.state.token.clone();

    let server_handle = tokio::spawn(async move {
        let _ = server.run().await;
    });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let client = reqwest::Client::new();
    let events = client
        .get(format!("http://127.0.0.1:{port}/api/events"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(events.status(), StatusCode::OK);
    let quit_url = format!("http://127.0.0.1:{}/api/quit", port);

    let res = client
        .post(&quit_url)
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);

    // Server should shut down gracefully
    let join_res = tokio::time::timeout(std::time::Duration::from_secs(2), server_handle).await;
    assert!(join_res.is_ok());
}

#[tokio::test]
async fn closing_the_last_browser_tab_stops_the_server_on_its_own() {
    let temp = TempDir::new().unwrap();
    let config = Config {
        download_dir: temp.path().join("downloads"),
        data_dir: temp.path().join("data"),
    };
    let service = DesktopService::new(config).unwrap();
    let server = DesktopServer::bind(service).await.unwrap();
    let port = server.state.port;
    let access = server.state.token.clone();

    let server_handle = tokio::spawn(async move {
        let _ = server.run().await;
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let client = reqwest::Client::new();
    let health_url = format!("http://127.0.0.1:{port}/api/health");

    // Open the events stream a browser tab holds open, read its first
    // message to confirm the connection is live, then drop it - the same
    // signal the server gets when a tab closes.
    let events = client
        .get(format!("http://127.0.0.1:{port}/api/events"))
        .bearer_auth(&access)
        .send()
        .await
        .unwrap();
    assert_eq!(events.status(), StatusCode::OK);
    let mut stream = events.bytes_stream();
    let _ = futures_util::StreamExt::next(&mut stream).await;
    drop(stream);

    assert_eq!(
        client
            .get(&health_url)
            .bearer_auth(&access)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK,
        "the server must not stop before its grace period has passed"
    );

    let join_res = tokio::time::timeout(std::time::Duration::from_secs(10), server_handle).await;
    assert!(
        join_res.is_ok(),
        "the server must stop on its own once its last tab has been closed for a while"
    );
    assert!(client.get(&health_url).send().await.is_err());
}

struct MockUpdateAdapter {
    status_result: std::sync::Mutex<Option<anyhow::Result<flix::desktop::updater::UpdateStatus>>>,
    install_result:
        std::sync::Mutex<Option<anyhow::Result<flix::desktop::updater::UpdateInstallResult>>>,
}

impl MockUpdateAdapter {
    fn new(
        status: Option<anyhow::Result<flix::desktop::updater::UpdateStatus>>,
        install: Option<anyhow::Result<flix::desktop::updater::UpdateInstallResult>>,
    ) -> Self {
        Self {
            status_result: std::sync::Mutex::new(status),
            install_result: std::sync::Mutex::new(install),
        }
    }
}

impl flix::desktop::updater::UpdateAdapter for MockUpdateAdapter {
    fn check<'a>(
        &'a self,
    ) -> futures_util::future::BoxFuture<'a, anyhow::Result<flix::desktop::updater::UpdateStatus>>
    {
        Box::pin(async move {
            self.status_result
                .lock()
                .unwrap()
                .take()
                .unwrap_or_else(|| Err(anyhow::anyhow!("Mock check failure")))
        })
    }

    fn download_and_launch<'a>(
        &'a self,
    ) -> futures_util::future::BoxFuture<
        'a,
        anyhow::Result<flix::desktop::updater::UpdateInstallResult>,
    > {
        Box::pin(async move {
            self.install_result
                .lock()
                .unwrap()
                .take()
                .unwrap_or_else(|| Err(anyhow::anyhow!("Mock install failure")))
        })
    }
}

#[tokio::test]
async fn update_endpoints_require_authentication() {
    let temp = TempDir::new().unwrap();
    let config = Config {
        download_dir: temp.path().join("downloads"),
        data_dir: temp.path().join("data"),
    };
    let service = DesktopService::new(config).unwrap();
    let server = DesktopServer::bind(service).await.unwrap();
    let port = server.state.port;

    let server_handle = tokio::spawn(async move {
        let _ = server.run().await;
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let client = reqwest::Client::new();
    let update_url = format!("http://127.0.0.1:{port}/api/update");

    let get_res = client.get(&update_url).send().await.unwrap();
    assert_eq!(get_res.status(), StatusCode::UNAUTHORIZED);

    let post_res = client.post(&update_url).send().await.unwrap();
    assert_eq!(post_res.status(), StatusCode::UNAUTHORIZED);

    server_handle.abort();
}

#[tokio::test]
async fn update_check_handles_success_and_failure() {
    let temp = TempDir::new().unwrap();
    let config = Config {
        download_dir: temp.path().join("downloads"),
        data_dir: temp.path().join("data"),
    };

    let mock_adapter = std::sync::Arc::new(MockUpdateAdapter::new(
        Some(Ok(flix::desktop::updater::UpdateStatus {
            current_version: "0.4.0".to_string(),
            latest_version: "0.4.1".to_string(),
            available: true,
            release_url: "https://github.com/BabalolaBrainiac/flix/releases/tag/v0.4.1".to_string(),
        })),
        None,
    ));

    let updater = flix::desktop::updater::Updater::new(&config.data_dir)
        .unwrap()
        .with_adapter(mock_adapter.clone());
    let service = DesktopService::new(config).unwrap().with_updater(updater);
    let server = DesktopServer::bind(service).await.unwrap();
    let port = server.state.port;
    let token = server.state.token.clone();

    let server_handle = tokio::spawn(async move {
        let _ = server.run().await;
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let client = reqwest::Client::new();
    let update_url = format!("http://127.0.0.1:{port}/api/update");

    // Success case
    let success_res = client
        .get(&update_url)
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(success_res.status(), StatusCode::OK);
    let status_json: serde_json::Value = success_res.json().await.unwrap();
    assert_eq!(status_json["available"], true);
    assert_eq!(status_json["latest_version"], "0.4.1");

    // Failure case (adapter now empty, defaults to Err)
    let failure_res = client
        .get(&update_url)
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(failure_res.status(), StatusCode::BAD_GATEWAY);
    let err_json: serde_json::Value = failure_res.json().await.unwrap();
    assert_eq!(err_json["error"]["code"], "UPDATE_CHECK_ERROR");

    server_handle.abort();
}

#[tokio::test]
async fn update_install_handles_success_and_failure() {
    let temp = TempDir::new().unwrap();
    let config = Config {
        download_dir: temp.path().join("downloads"),
        data_dir: temp.path().join("data"),
    };

    let mock_adapter = std::sync::Arc::new(MockUpdateAdapter::new(
        None,
        Some(Err(anyhow::anyhow!("Verification failure"))),
    ));

    let updater = flix::desktop::updater::Updater::new(&config.data_dir)
        .unwrap()
        .with_adapter(mock_adapter.clone());
    let service = DesktopService::new(config).unwrap().with_updater(updater);
    let server = DesktopServer::bind(service).await.unwrap();
    let port = server.state.port;
    let token = server.state.token.clone();

    let server_handle = tokio::spawn(async move {
        let _ = server.run().await;
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let client = reqwest::Client::new();
    let update_url = format!("http://127.0.0.1:{port}/api/update");

    // 1. Failure case
    let failure_res = client
        .post(&update_url)
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(failure_res.status(), StatusCode::BAD_GATEWAY);
    let err_json: serde_json::Value = failure_res.json().await.unwrap();
    assert_eq!(err_json["error"]["code"], "UPDATE_INSTALL_ERROR");

    // Server must stay running after install failure
    let health_res = client
        .get(format!("http://127.0.0.1:{port}/api/health"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(health_res.status(), StatusCode::OK);

    // 2. Success case with requires_manual_finish = true (macOS style: no auto shutdown)
    *mock_adapter.install_result.lock().unwrap() =
        Some(Ok(flix::desktop::updater::UpdateInstallResult {
            version: "0.4.1".to_string(),
            package_path: "/tmp/Flix-macOS-universal.dmg".to_string(),
            requires_manual_finish: true,
        }));

    let success_res = client
        .post(&update_url)
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(success_res.status(), StatusCode::OK);
    let install_json: serde_json::Value = success_res.json().await.unwrap();
    assert_eq!(install_json["requires_manual_finish"], true);

    // Give time to confirm server does NOT shut down
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let health_res2 = client
        .get(format!("http://127.0.0.1:{port}/api/health"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(health_res2.status(), StatusCode::OK);

    // 3. Success case with requires_manual_finish = false (Windows style: graceful shutdown follows)
    *mock_adapter.install_result.lock().unwrap() =
        Some(Ok(flix::desktop::updater::UpdateInstallResult {
            version: "0.4.1".to_string(),
            package_path: "C:\\Updates\\Flix-Windows-x64-Setup.exe".to_string(),
            requires_manual_finish: false,
        }));

    let shutdown_res = client
        .post(&update_url)
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(shutdown_res.status(), StatusCode::OK);

    // Server should shut down after ~1 second
    let join_res = tokio::time::timeout(std::time::Duration::from_secs(3), server_handle).await;
    assert!(
        join_res.is_ok(),
        "server must shut down following helper launch"
    );
}
