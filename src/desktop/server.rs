use crate::desktop::service::DesktopService;
use crate::desktop::types::{
    DownloadsAction, DownloadsCommand, EpisodesCommand, NextEpisodeCommand, PlayCommand,
    SearchCommand, SettingsAction, SettingsCommand, StopPlaybackCommand, StreamsCommand,
};
use anyhow::{Context, Result};
use axum::{
    body::Body,
    extract::{Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use mime_guess::from_path;
use rust_embed::RustEmbed;
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::{broadcast, Mutex};
use uuid::Uuid;

#[derive(RustEmbed)]
#[folder = "web/dist/"]
struct WebAssets;

#[derive(Clone)]
pub struct ServerState {
    pub service: Arc<Mutex<DesktopService>>,
    pub token: String,
    pub port: u16,
    pub shutdown_tx: broadcast::Sender<()>,
}

pub struct DesktopServer {
    pub state: ServerState,
    pub listener: TcpListener,
    pub addr: SocketAddr,
}

impl DesktopServer {
    pub async fn bind(service: DesktopService) -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .context("Failed to bind local HTTP server to 127.0.0.1")?;
        let addr = listener.local_addr()?;
        let port = addr.port();

        // Generate 64-char cryptographically random hex bootstrap token
        let token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        let (shutdown_tx, _) = broadcast::channel(1);

        let state = ServerState {
            service: Arc::new(Mutex::new(service)),
            token,
            port,
            shutdown_tx,
        };

        Ok(Self {
            state,
            listener,
            addr,
        })
    }

    pub fn url(&self) -> String {
        format!(
            "http://127.0.0.1:{}/?token={}",
            self.state.port, self.state.token
        )
    }

    pub fn local_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.state.port)
    }

    pub async fn run(self) -> Result<()> {
        let app = create_router(self.state.clone());
        let mut shutdown_rx = self.state.shutdown_tx.subscribe();

        axum::serve(self.listener, app)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.recv().await;
            })
            .await
            .context("Desktop HTTP server encounter an error")?;

        Ok(())
    }
}

pub fn create_router(state: ServerState) -> Router {
    Router::new()
        .route("/api/health", get(handle_health))
        .route("/api/status", get(handle_status))
        .route("/api/activation", get(handle_get_activation))
        .route("/api/activation/redeem", post(handle_post_redeem_invite))
        .route("/api/activation/reset", post(handle_post_reset_activation))
        .route("/api/vlc-guidance", get(handle_get_vlc_guidance))
        .route("/api/diagnostics", get(handle_get_diagnostics))
        .route("/api/search", post(handle_search))
        .route("/api/episodes", post(handle_episodes))
        .route("/api/streams", post(handle_streams))
        .route("/api/play", post(handle_play))
        .route("/api/next", post(handle_next))
        .route("/api/stop", post(handle_stop))
        .route(
            "/api/downloads",
            get(handle_get_downloads).post(handle_post_downloads),
        )
        .route("/api/settings", get(handle_get_settings))
        .route("/api/quit", post(handle_quit))
        .fallback(handle_static_or_spa)
        .with_state(state)
}

fn validate_origin_and_token(headers: &HeaderMap, state: &ServerState) -> Result<(), StatusCode> {
    // 1. Origin / Host check: reject non-local origin headers if present
    if let Some(origin) = headers.get(header::ORIGIN) {
        if let Ok(origin_str) = origin.to_str() {
            let allowed_1 = format!("http://127.0.0.1:{}", state.port);
            let allowed_2 = format!("http://localhost:{}", state.port);
            if origin_str != allowed_1 && origin_str != allowed_2 {
                return Err(StatusCode::FORBIDDEN);
            }
        }
    }

    // 2. Token check
    let auth_header = headers
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok());
    let custom_token_header = headers.get("X-Flix-Token").and_then(|h| h.to_str().ok());

    let token_valid = if let Some(auth) = auth_header {
        if let Some(bearer) = auth.strip_prefix("Bearer ") {
            bearer.trim() == state.token
        } else {
            false
        }
    } else if let Some(custom) = custom_token_header {
        custom.trim() == state.token
    } else {
        false
    };

    if !token_valid {
        return Err(StatusCode::UNAUTHORIZED);
    }

    Ok(())
}

async fn handle_health(
    State(state): State<ServerState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, StatusCode> {
    validate_origin_and_token(&headers, &state)?;
    Ok(Json(json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION")
    })))
}

async fn handle_status(
    State(state): State<ServerState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, StatusCode> {
    validate_origin_and_token(&headers, &state)?;
    let mut service = state.service.lock().await;
    let res = service
        .status()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(res))
}

async fn handle_get_activation(
    State(state): State<ServerState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, StatusCode> {
    validate_origin_and_token(&headers, &state)?;
    let service = state.service.lock().await;
    let res = service.activation_status();
    Ok(Json(res))
}

async fn handle_post_redeem_invite(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(cmd): Json<crate::desktop::types::RedeemInviteCommand>,
) -> Result<impl IntoResponse, StatusCode> {
    validate_origin_and_token(&headers, &state)?;
    let service = state.service.lock().await;
    let res = service
        .redeem_invite(&cmd)
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok(Json(res))
}

async fn handle_post_reset_activation(
    State(state): State<ServerState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, StatusCode> {
    validate_origin_and_token(&headers, &state)?;
    let service = state.service.lock().await;
    service
        .reset_activation()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(json!({ "success": true })))
}

async fn handle_get_vlc_guidance(
    State(state): State<ServerState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, StatusCode> {
    validate_origin_and_token(&headers, &state)?;
    let service = state.service.lock().await;
    let res = service.vlc_guidance();
    Ok(Json(res))
}

async fn handle_get_diagnostics(
    State(state): State<ServerState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, StatusCode> {
    validate_origin_and_token(&headers, &state)?;
    let service = state.service.lock().await;
    let res = service
        .diagnostics()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(res))
}

async fn handle_search(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(command): Json<SearchCommand>,
) -> Result<impl IntoResponse, StatusCode> {
    validate_origin_and_token(&headers, &state)?;
    let service = state.service.lock().await;
    let res = service
        .search(&command)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(res))
}

async fn handle_episodes(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(command): Json<EpisodesCommand>,
) -> Result<impl IntoResponse, StatusCode> {
    validate_origin_and_token(&headers, &state)?;
    let service = state.service.lock().await;
    let res = service
        .episodes(&command)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(res))
}

async fn handle_streams(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(command): Json<StreamsCommand>,
) -> Result<impl IntoResponse, StatusCode> {
    validate_origin_and_token(&headers, &state)?;
    let service = state.service.lock().await;
    let res = service
        .streams(&command)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(res))
}

async fn handle_play(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(command): Json<PlayCommand>,
) -> Result<impl IntoResponse, StatusCode> {
    validate_origin_and_token(&headers, &state)?;
    let mut service = state.service.lock().await;
    let res = service
        .play(command)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(res))
}

async fn handle_next(
    State(state): State<ServerState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, StatusCode> {
    validate_origin_and_token(&headers, &state)?;
    let mut service = state.service.lock().await;
    let res = service
        .next(NextEpisodeCommand)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(res))
}

async fn handle_stop(
    State(state): State<ServerState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, StatusCode> {
    validate_origin_and_token(&headers, &state)?;
    let mut service = state.service.lock().await;
    let res = service
        .stop(StopPlaybackCommand)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(res))
}

async fn handle_get_downloads(
    State(state): State<ServerState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, StatusCode> {
    validate_origin_and_token(&headers, &state)?;
    let service = state.service.lock().await;
    let res = service
        .downloads(&DownloadsCommand {
            action: DownloadsAction::List,
        })
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(res))
}

#[derive(Deserialize)]
struct PostDownloadsPayload {
    action: String,
    magnet: Option<String>,
    file_index: Option<usize>,
}

async fn handle_post_downloads(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(payload): Json<PostDownloadsPayload>,
) -> Result<impl IntoResponse, StatusCode> {
    validate_origin_and_token(&headers, &state)?;
    let service = state.service.lock().await;
    let action = if payload.action == "add" {
        let magnet = payload.magnet.ok_or(StatusCode::BAD_REQUEST)?;
        DownloadsAction::Add {
            magnet,
            file_index: payload.file_index,
        }
    } else {
        DownloadsAction::List
    };
    let res = service
        .downloads(&DownloadsCommand { action })
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(res))
}

async fn handle_get_settings(
    State(state): State<ServerState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, StatusCode> {
    validate_origin_and_token(&headers, &state)?;
    let service = state.service.lock().await;
    let res = service
        .settings(&SettingsCommand {
            action: SettingsAction::Get,
        })
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(res))
}

async fn handle_quit(
    State(state): State<ServerState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, StatusCode> {
    validate_origin_and_token(&headers, &state)?;
    let mut service = state.service.lock().await;
    let _ = service.stop(StopPlaybackCommand).await;
    let _ = state.shutdown_tx.send(());
    Ok(Json(json!({ "success": true })))
}

async fn handle_static_or_spa(
    uri: axum::http::Uri,
    Query(_query): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');
    let asset_path = if path.is_empty() { "index.html" } else { path };

    if let Some(file) = WebAssets::get(asset_path) {
        let mime = from_path(asset_path).first_or_octet_stream();
        return Response::builder()
            .status(StatusCode::OK)
            .header(
                header::CONTENT_TYPE,
                HeaderValue::from_str(mime.as_ref())
                    .unwrap_or(HeaderValue::from_static("application/octet-stream")),
            )
            .body(Body::from(file.data))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
    }

    // SPA fallback: return index.html for unknown client routes
    if let Some(index) = WebAssets::get("index.html") {
        return Response::builder()
            .status(StatusCode::OK)
            .header(
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/html; charset=utf-8"),
            )
            .body(Body::from(index.data))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
    }

    StatusCode::NOT_FOUND.into_response()
}
