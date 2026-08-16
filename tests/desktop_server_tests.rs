use flix::config::Config;
use flix::desktop::{DesktopServer, DesktopService};
use reqwest::header::{AUTHORIZATION, ORIGIN};
use reqwest::StatusCode;
use tempfile::TempDir;

#[tokio::test]
async fn rejects_api_requests_without_or_with_invalid_token() {
    let temp = TempDir::new().unwrap();
    let config = Config {
        download_dir: temp.path().join("downloads"),
        data_dir: temp.path().join("data"),
    };
    let service = DesktopService::new(config);
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
    let service = DesktopService::new(config);
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
async fn quit_endpoint_signals_graceful_shutdown() {
    let temp = TempDir::new().unwrap();
    let config = Config {
        download_dir: temp.path().join("downloads"),
        data_dir: temp.path().join("data"),
    };
    let service = DesktopService::new(config);
    let server = DesktopServer::bind(service).await.unwrap();
    let port = server.state.port;
    let token = server.state.token.clone();

    let server_handle = tokio::spawn(async move {
        let _ = server.run().await;
    });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let client = reqwest::Client::new();
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
