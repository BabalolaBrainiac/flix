use crate::desktop::service::DesktopService;
use crate::desktop::types::{
    DownloadsAction, DownloadsCommand, EpisodesCommand, NextEpisodeCommand, PlayCommand,
    ReaderChaptersCommand, ReaderPagesCommand, ReaderProgressSaveCommand, ReaderSearchCommand,
    RecommendCommand, RedeemInviteCommand, SearchCommand, SettingsAction, SettingsCommand,
    StopPlaybackCommand, StreamsCommand,
};
use anyhow::{Context, Result};
use axum::{
    body::Body,
    extract::{Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    routing::{get, post},
    Json, Router,
};
use futures_util::stream::{self, Stream};
use mime_guess::from_path;
use rust_embed::RustEmbed;
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tracing;
use uuid::Uuid;

#[derive(RustEmbed)]
#[folder = "web/dist/"]
struct WebAssets;

#[derive(Clone)]
pub struct ServerState {
    pub service: Arc<DesktopService>,
    pub token: String,
    pub port: u16,
    pub shutdown_tx: broadcast::Sender<()>,
    /// How many browser tabs currently hold the `/api/events` stream open.
    /// Used to shut the app down on its own once the last tab closes - see
    /// `ClientGuard`.
    active_clients: Arc<std::sync::atomic::AtomicUsize>,
}

pub struct DesktopServer {
    pub state: ServerState,
    pub listener: TcpListener,
    pub addr: SocketAddr,
}

impl DesktopServer {
    pub async fn bind(service: DesktopService) -> Result<Self> {
        // Port 0 asks the OS for any free port.
        Self::bind_on(service, 0).await
    }

    /// Binds the local server to a chosen port. A port of 0 asks the OS for any
    /// free port. A fixed port lets a second instance run beside the first, for
    /// example the web client next to the CLI.
    pub async fn bind_on(service: DesktopService, port: u16) -> Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", port))
            .await
            .with_context(|| format!("Failed to bind local HTTP server to 127.0.0.1:{port}"))?;
        let addr = listener.local_addr()?;
        let port = addr.port();

        // Generate 64-char cryptographically random hex bootstrap token
        let token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        let (shutdown_tx, _) = broadcast::channel(1);

        let state = ServerState {
            service: Arc::new(service),
            token,
            port,
            shutdown_tx,
            active_clients: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
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
        let coordinator = self.state.service.coordinator();

        crate::desktop::progress_tracker::spawn(
            Arc::clone(&coordinator),
            self.state.service.profiles(),
            self.state.service.current_profile_id_handle(),
        );

        axum::serve(self.listener, app)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.recv().await;
                let _ = coordinator.stop().await;
            })
            .await
            .context("Desktop HTTP server encountered an error")?;

        Ok(())
    }
}

pub fn create_router(state: ServerState) -> Router {
    Router::new()
        .route("/api/health", get(handle_health))
        .route("/api/events", get(handle_events))
        .route("/api/status", get(handle_status))
        .route("/api/activation", get(handle_activation_status))
        .route("/api/activation/redeem", post(handle_redeem_invite))
        .route("/api/activation/reset", post(handle_reset_activation))
        .route("/api/vlc-guidance", get(handle_get_vlc_guidance))
        .route("/api/diagnostics", get(handle_get_diagnostics))
        .route("/api/diagnostics/export", get(handle_export_diagnostics))
        .route("/api/search", post(handle_search))
        .route("/api/recommend", post(handle_recommend))
        .route("/api/episodes", post(handle_episodes))
        .route("/api/title", post(handle_title_lookup))
        .route("/api/streams", post(handle_streams))
        .route("/api/play", post(handle_play))
        .route("/api/browser/event", post(handle_browser_event))
        .route("/api/next", post(handle_next))
        .route("/api/stop", post(handle_stop))
        .route(
            "/api/downloads",
            get(handle_get_downloads).post(handle_post_downloads),
        )
        .route("/api/settings", get(handle_get_settings))
        .route("/api/reader/search", post(handle_reader_search))
        .route("/api/reader/chapters", post(handle_reader_chapters))
        .route("/api/reader/pages", post(handle_reader_pages))
        .route("/api/reader/page", get(handle_reader_page))
        .route("/api/poster", get(handle_poster))
        .route(
            "/api/reader/progress",
            get(handle_reader_progress_get).post(handle_reader_progress_save),
        )
        .route("/api/quit", post(handle_quit))
        .route(
            "/api/profiles",
            get(handle_get_profiles).post(handle_create_profile),
        )
        .route("/api/profiles/update", post(handle_update_profile))
        .route("/api/profiles/delete", post(handle_delete_profile))
        .route("/api/profiles/active", post(handle_set_active_profile))
        .route(
            "/api/profiles/lists",
            get(handle_get_lists).post(handle_create_list),
        )
        .route("/api/profiles/lists/delete", post(handle_delete_list))
        .route("/api/profiles/lists/items", post(handle_add_list_item))
        .route(
            "/api/profiles/lists/items/delete",
            post(handle_remove_list_item),
        )
        .route(
            "/api/profiles/status",
            get(handle_get_watch_status).post(handle_set_watch_status),
        )
        .route(
            "/api/profiles/status/delete",
            post(handle_remove_watch_status),
        )
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

fn validate_query_or_header_token(
    headers: &HeaderMap,
    query_token: Option<&str>,
    state: &ServerState,
) -> Result<(), StatusCode> {
    if let Some(token) = query_token {
        if token.trim() == state.token {
            return Ok(());
        }
    }
    validate_origin_and_token(headers, state)
}

fn api_error(status: StatusCode, code: &str, message: &str, retryable: bool) -> Response {
    let body = json!({
        "error": {
            "code": code,
            "message": message,
            "retryable": retryable
        }
    });
    (status, Json(body)).into_response()
}

async fn handle_export_diagnostics(
    State(state): State<ServerState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, StatusCode> {
    validate_origin_and_token(&headers, &state)?;
    Ok((
        [
            (header::CACHE_CONTROL, "no-store"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=flix-debug-report.json",
            ),
        ],
        Json(state.service.coordinator().debug_report()),
    ))
}

async fn handle_browser_event(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(command): Json<crate::playback::BrowserEventCommand>,
) -> Result<Response, StatusCode> {
    validate_origin_and_token(&headers, &state)?;
    match state.service.coordinator().browser_event(command).await {
        Ok(()) => Ok(Json(json!({ "success": true })).into_response()),
        Err(_) => Ok(api_error(
            StatusCode::CONFLICT,
            "BROWSER_SESSION_CHANGED",
            "The playback session changed. Refresh its status.",
            false,
        )),
    }
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

/// How long this process waits after its last browser tab closes before it stops.
/// Five seconds gives enough time for browser page reloads.
const AUTO_SHUTDOWN_GRACE: std::time::Duration = std::time::Duration::from_secs(5);

/// Held for as long as one browser tab holds an events connection.
/// When the last tab drops, it schedules an auto-shutdown check.
struct ClientGuard {
    active_clients: Arc<std::sync::atomic::AtomicUsize>,
    shutdown_tx: broadcast::Sender<()>,
    coordinator: Arc<crate::playback::PlaybackCoordinator>,
}

impl Drop for ClientGuard {
    fn drop(&mut self) {
        let remaining = self
            .active_clients
            .fetch_sub(1, std::sync::atomic::Ordering::SeqCst)
            - 1;
        if remaining != 0 {
            return;
        }
        let active_clients = Arc::clone(&self.active_clients);
        let shutdown_tx = self.shutdown_tx.clone();
        let coordinator = Arc::clone(&self.coordinator);
        tokio::spawn(async move {
            tokio::time::sleep(AUTO_SHUTDOWN_GRACE).await;
            loop {
                if active_clients.load(std::sync::atomic::Ordering::SeqCst) > 0 {
                    return;
                }
                let snapshot = coordinator.snapshot();
                let external_playing = matches!(
                    snapshot,
                    crate::playback::PlaybackSnapshot::Playing { .. }
                        | crate::playback::PlaybackSnapshot::LaunchingPlayer { .. }
                );
                if !external_playing {
                    tracing::info!(
                        "No browser tab has been open for {}s - stopping gracefully.",
                        AUTO_SHUTDOWN_GRACE.as_secs()
                    );
                    let _ = coordinator.stop().await;
                    let _ = shutdown_tx.send(());
                    return;
                }
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
        });
    }
}

async fn handle_events(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, Response> {
    validate_query_or_header_token(&headers, query.get("token").map(String::as_str), &state)
        .map_err(|e| api_error(e, "UNAUTHORIZED", "Unauthorized", false))?;

    let coordinator = state.service.coordinator();
    let rx = coordinator.subscribe();
    state
        .active_clients
        .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let guard = ClientGuard {
        active_clients: Arc::clone(&state.active_clients),
        shutdown_tx: state.shutdown_tx.clone(),
        coordinator,
    };

    let initial = rx.borrow().clone();
    let shutdown = state.shutdown_tx.subscribe();
    let stream = stream::unfold(
        (rx, Some(initial), shutdown, guard),
        |(mut rx, initial, mut shutdown, guard)| async move {
            if let Some(first) = initial {
                let data = serde_json::to_string(&first).unwrap_or_default();
                let event = Event::default().data(data);
                return Some((Ok(event), (rx, None, shutdown, guard)));
            }
            let changed = tokio::select! {
                biased;
                _ = shutdown.recv() => return None,
                changed = rx.changed() => changed,
            };
            if changed.is_ok() {
                let val = rx.borrow().clone();
                let data = serde_json::to_string(&val).unwrap_or_default();
                let event = Event::default().data(data);
                Some((Ok(event), (rx, None, shutdown, guard)))
            } else {
                None
            }
        },
    );

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

async fn handle_status(
    State(state): State<ServerState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, Response> {
    validate_origin_and_token(&headers, &state)
        .map_err(|e| api_error(e, "UNAUTHORIZED", "Unauthorized", false))?;
    let res = state.service.status().await.map_err(|e| {
        tracing::error!("status error: {:#}", e);
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "STATUS_ERROR",
            "Failed to retrieve playback status",
            true,
        )
    })?;
    Ok(Json(res))
}

async fn handle_get_vlc_guidance(
    State(state): State<ServerState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, Response> {
    validate_origin_and_token(&headers, &state)
        .map_err(|e| api_error(e, "UNAUTHORIZED", "Unauthorized", false))?;
    let res = state.service.vlc_guidance();
    Ok(Json(res))
}

async fn handle_get_diagnostics(
    State(state): State<ServerState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, Response> {
    validate_origin_and_token(&headers, &state)
        .map_err(|e| api_error(e, "UNAUTHORIZED", "Unauthorized", false))?;
    let res = state.service.diagnostics().map_err(|e| {
        tracing::error!("diagnostics error: {:#}", e);
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "DIAGNOSTICS_ERROR",
            "Failed to retrieve diagnostics",
            false,
        )
    })?;
    Ok(Json(res))
}

async fn handle_search(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(command): Json<SearchCommand>,
) -> Result<impl IntoResponse, Response> {
    validate_origin_and_token(&headers, &state)
        .map_err(|e| api_error(e, "UNAUTHORIZED", "Unauthorized", false))?;
    let res = state.service.search(&command).await.map_err(|e| {
        tracing::error!("search error: {:#}", e);
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "SEARCH_ERROR",
            "Failed to perform search query",
            true,
        )
    })?;
    Ok(Json(res))
}

async fn handle_recommend(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(command): Json<RecommendCommand>,
) -> Result<impl IntoResponse, Response> {
    validate_origin_and_token(&headers, &state)
        .map_err(|e| api_error(e, "UNAUTHORIZED", "Unauthorized", false))?;
    let res = state.service.recommend(&command).await.map_err(|e| {
        tracing::error!("recommend error: {:#}", e);
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "RECOMMEND_ERROR",
            "Failed to get recommendations",
            true,
        )
    })?;
    Ok(Json(res))
}

async fn handle_episodes(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(command): Json<EpisodesCommand>,
) -> Result<impl IntoResponse, Response> {
    validate_origin_and_token(&headers, &state)
        .map_err(|e| api_error(e, "UNAUTHORIZED", "Unauthorized", false))?;
    let res = state.service.episodes(&command).await.map_err(|e| {
        tracing::error!("episodes error: {:#}", e);
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "EPISODES_ERROR",
            "Failed to retrieve episode list",
            true,
        )
    })?;
    Ok(Json(res))
}

async fn handle_title_lookup(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(command): Json<crate::desktop::types::TitleLookupCommand>,
) -> Result<impl IntoResponse, Response> {
    validate_origin_and_token(&headers, &state)
        .map_err(|e| api_error(e, "UNAUTHORIZED", "Unauthorized", false))?;
    let res = state.service.title_lookup(&command).await.map_err(|e| {
        tracing::warn!("title lookup error: {:#}", e);
        api_error(
            StatusCode::NOT_FOUND,
            "TITLE_LOOKUP_ERROR",
            "Failed to look up title",
            true,
        )
    })?;
    Ok(Json(res))
}

async fn handle_streams(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(command): Json<StreamsCommand>,
) -> Result<impl IntoResponse, Response> {
    validate_origin_and_token(&headers, &state)
        .map_err(|e| api_error(e, "UNAUTHORIZED", "Unauthorized", false))?;
    let res = state.service.streams(&command).await.map_err(|e| {
        tracing::error!("streams error: {:#}", e);
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "STREAMS_ERROR",
            "Failed to retrieve stream sources",
            true,
        )
    })?;
    Ok(Json(res))
}

async fn handle_play(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(command): Json<PlayCommand>,
) -> Result<impl IntoResponse, Response> {
    validate_origin_and_token(&headers, &state)
        .map_err(|e| api_error(e, "UNAUTHORIZED", "Unauthorized", false))?;
    let res = state.service.play(command).await.map_err(|e| {
        tracing::error!("play error: {:#}", e);
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "PLAY_ERROR",
            "Failed to start playback",
            true,
        )
    })?;
    Ok((StatusCode::ACCEPTED, Json(res)))
}

async fn handle_next(
    State(state): State<ServerState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, Response> {
    validate_origin_and_token(&headers, &state)
        .map_err(|e| api_error(e, "UNAUTHORIZED", "Unauthorized", false))?;
    let res = state.service.next(NextEpisodeCommand).await.map_err(|e| {
        tracing::error!("next error: {:#}", e);
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "NEXT_EPISODE_ERROR",
            "Failed to advance to next episode",
            false,
        )
    })?;
    Ok(Json(res))
}

async fn handle_stop(
    State(state): State<ServerState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, Response> {
    validate_origin_and_token(&headers, &state)
        .map_err(|e| api_error(e, "UNAUTHORIZED", "Unauthorized", false))?;
    let res = state.service.stop(StopPlaybackCommand).await.map_err(|e| {
        tracing::error!("stop error: {:#}", e);
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "STOP_ERROR",
            "Failed to stop playback",
            false,
        )
    })?;
    Ok(Json(res))
}

async fn handle_get_downloads(
    State(state): State<ServerState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, Response> {
    validate_origin_and_token(&headers, &state)
        .map_err(|e| api_error(e, "UNAUTHORIZED", "Unauthorized", false))?;
    let res = state
        .service
        .downloads(&DownloadsCommand {
            action: DownloadsAction::List,
        })
        .await
        .map_err(|e| {
            tracing::error!("downloads error: {:#}", e);
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "DOWNLOADS_ERROR",
                "Failed to list downloads",
                true,
            )
        })?;
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
) -> Result<impl IntoResponse, Response> {
    validate_origin_and_token(&headers, &state)
        .map_err(|e| api_error(e, "UNAUTHORIZED", "Unauthorized", false))?;
    let action = if payload.action == "add" {
        let magnet = payload.magnet.ok_or_else(|| {
            api_error(
                StatusCode::BAD_REQUEST,
                "INVALID_PARAMETER",
                "Missing magnet parameter",
                false,
            )
        })?;
        DownloadsAction::Add {
            magnet,
            file_index: payload.file_index,
        }
    } else {
        DownloadsAction::List
    };
    let res = state
        .service
        .downloads(&DownloadsCommand { action })
        .await
        .map_err(|e| {
            tracing::error!("downloads error: {:#}", e);
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "DOWNLOADS_ERROR",
                "Failed to process download action",
                false,
            )
        })?;
    Ok(Json(res))
}

async fn handle_get_settings(
    State(state): State<ServerState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, Response> {
    validate_origin_and_token(&headers, &state)
        .map_err(|e| api_error(e, "UNAUTHORIZED", "Unauthorized", false))?;
    let res = state
        .service
        .settings(&SettingsCommand {
            action: SettingsAction::Get,
        })
        .map_err(|e| {
            tracing::error!("settings error: {:#}", e);
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "SETTINGS_ERROR",
                "Failed to retrieve settings",
                false,
            )
        })?;
    Ok(Json(res))
}

async fn handle_quit(
    State(state): State<ServerState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, Response> {
    validate_origin_and_token(&headers, &state)
        .map_err(|e| api_error(e, "UNAUTHORIZED", "Unauthorized", false))?;
    let _ = state.service.stop(StopPlaybackCommand).await;
    let _ = state.shutdown_tx.send(());
    Ok(Json(json!({ "success": true })))
}

async fn handle_activation_status(
    State(state): State<ServerState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, Response> {
    validate_origin_and_token(&headers, &state)
        .map_err(|e| api_error(e, "UNAUTHORIZED", "Unauthorized", false))?;
    Ok(Json(state.service.activation_status()))
}

async fn handle_redeem_invite(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(command): Json<RedeemInviteCommand>,
) -> Result<impl IntoResponse, Response> {
    validate_origin_and_token(&headers, &state)
        .map_err(|e| api_error(e, "UNAUTHORIZED", "Unauthorized", false))?;
    let res = state.service.redeem_invite(&command).await.map_err(|e| {
        tracing::error!("activation error: {:#}", e);
        api_error(
            StatusCode::BAD_GATEWAY,
            "ACTIVATION_ERROR",
            "Device activation failed",
            true,
        )
    })?;
    Ok(Json(res))
}

async fn handle_reset_activation(
    State(state): State<ServerState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, Response> {
    validate_origin_and_token(&headers, &state)
        .map_err(|e| api_error(e, "UNAUTHORIZED", "Unauthorized", false))?;
    state.service.reset_activation().map_err(|e| {
        tracing::error!("activation reset error: {:#}", e);
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "ACTIVATION_RESET_ERROR",
            "Device activation reset failed",
            false,
        )
    })?;
    Ok(Json(json!({ "success": true })))
}

async fn handle_reader_search(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(command): Json<ReaderSearchCommand>,
) -> Result<impl IntoResponse, Response> {
    validate_origin_and_token(&headers, &state)
        .map_err(|e| api_error(e, "UNAUTHORIZED", "Unauthorized", false))?;
    let res = state.service.reader_search(&command).await.map_err(|e| {
        tracing::error!("reader search error: {:#}", e);
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "READER_SEARCH_ERROR",
            "Failed to search manga",
            true,
        )
    })?;
    Ok(Json(res))
}

async fn handle_reader_chapters(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(command): Json<ReaderChaptersCommand>,
) -> Result<impl IntoResponse, Response> {
    validate_origin_and_token(&headers, &state)
        .map_err(|e| api_error(e, "UNAUTHORIZED", "Unauthorized", false))?;
    let res = state.service.reader_chapters(&command).await.map_err(|e| {
        tracing::error!("reader chapters error: {:#}", e);
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "READER_CHAPTERS_ERROR",
            "Failed to retrieve chapters",
            true,
        )
    })?;
    Ok(Json(res))
}

async fn handle_reader_pages(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(command): Json<ReaderPagesCommand>,
) -> Result<impl IntoResponse, Response> {
    validate_origin_and_token(&headers, &state)
        .map_err(|e| api_error(e, "UNAUTHORIZED", "Unauthorized", false))?;
    let res = state.service.reader_pages(&command).await.map_err(|e| {
        tracing::error!("reader pages error: {:#}", e);
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "READER_PAGES_ERROR",
            "Failed to retrieve pages",
            true,
        )
    })?;
    Ok(Json(res))
}

async fn handle_reader_page(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
) -> Result<impl IntoResponse, Response> {
    let chapter_id = params.get("chapter_id").cloned().unwrap_or_default();
    let manga_id = params.get("manga_id").cloned().unwrap_or_default();
    let index: usize = params
        .get("index")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    // An <img> tag cannot send a header, so a page URL carries a signature
    // for this one page. The master token is never accepted in the query.
    let signed = params
        .get("exp")
        .and_then(|value| value.parse::<u64>().ok())
        .zip(params.get("sig"))
        .is_some_and(|(expires_at, signature)| {
            state
                .service
                .verify_reader_page(&chapter_id, &manga_id, index, expires_at, signature)
        });
    if !signed {
        validate_origin_and_token(&headers, &state)
            .map_err(|e| api_error(e, "UNAUTHORIZED", "Unauthorized", false))?;
    }

    let (data, mime) = state
        .service
        .reader_page_download(&chapter_id, &manga_id, index)
        .await
        .map_err(|e| {
            tracing::error!("reader page error: {:#}", e);
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "READER_PAGE_ERROR",
                "Failed to load page",
                true,
            )
        })?;

    Ok((
        [
            (header::CONTENT_TYPE, mime),
            (header::CACHE_CONTROL, "private, max-age=600".to_string()),
        ],
        data,
    ))
}

async fn handle_poster(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
) -> Result<impl IntoResponse, Response> {
    let upstream = params.get("u").cloned().unwrap_or_default();
    let signed = params
        .get("exp")
        .and_then(|value| value.parse::<u64>().ok())
        .zip(params.get("sig"))
        .is_some_and(|(expires_at, signature)| {
            state
                .service
                .verify_poster(&upstream, expires_at, signature)
        });
    if !signed {
        validate_origin_and_token(&headers, &state)
            .map_err(|e| api_error(e, "UNAUTHORIZED", "Unauthorized", false))?;
    }

    let (data, mime) = state.service.poster(&upstream).await.map_err(|e| {
        tracing::warn!("poster error: {:#}", e);
        api_error(
            StatusCode::BAD_GATEWAY,
            "POSTER_ERROR",
            "Failed to load poster",
            true,
        )
    })?;
    Ok((
        [
            (header::CONTENT_TYPE, mime.to_string()),
            (header::CACHE_CONTROL, "private, max-age=86400".to_string()),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_string()),
        ],
        data,
    ))
}

async fn handle_reader_progress_get(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
) -> Result<impl IntoResponse, Response> {
    validate_origin_and_token(&headers, &state)
        .map_err(|e| api_error(e, "UNAUTHORIZED", "Unauthorized", false))?;

    let publication_id = params.get("publication_id").cloned().unwrap_or_default();
    let res = state
        .service
        .reader_progress_get(&publication_id)
        .map_err(|e| {
            tracing::error!("reader progress get error: {:#}", e);
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "READER_PROGRESS_ERROR",
                "Failed to get reading progress",
                false,
            )
        })?;
    Ok(Json(res))
}

async fn handle_reader_progress_save(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(command): Json<ReaderProgressSaveCommand>,
) -> Result<impl IntoResponse, Response> {
    validate_origin_and_token(&headers, &state)
        .map_err(|e| api_error(e, "UNAUTHORIZED", "Unauthorized", false))?;
    state.service.reader_progress_save(&command).map_err(|e| {
        tracing::error!("reader progress save error: {:#}", e);
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "READER_PROGRESS_ERROR",
            "Failed to save reading progress",
            false,
        )
    })?;
    Ok(Json(json!({ "success": true })))
}

// ---- Profiles, lists, and watch status ----

async fn handle_get_profiles(
    State(state): State<ServerState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, Response> {
    validate_origin_and_token(&headers, &state)
        .map_err(|e| api_error(e, "UNAUTHORIZED", "Unauthorized", false))?;
    let res = state.service.list_profiles().await.map_err(|e| {
        tracing::error!("list profiles error: {:#}", e);
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "PROFILES_ERROR",
            "Failed to list profiles",
            true,
        )
    })?;
    Ok(Json(res))
}

async fn handle_create_profile(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(command): Json<crate::desktop::types::CreateProfileCommand>,
) -> Result<impl IntoResponse, Response> {
    validate_origin_and_token(&headers, &state)
        .map_err(|e| api_error(e, "UNAUTHORIZED", "Unauthorized", false))?;
    let res = state.service.create_profile(command).await.map_err(|e| {
        api_error(
            StatusCode::BAD_REQUEST,
            "CREATE_PROFILE_ERROR",
            &e.to_string(),
            false,
        )
    })?;
    Ok(Json(res))
}

async fn handle_update_profile(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(command): Json<crate::desktop::types::UpdateProfileCommand>,
) -> Result<impl IntoResponse, Response> {
    validate_origin_and_token(&headers, &state)
        .map_err(|e| api_error(e, "UNAUTHORIZED", "Unauthorized", false))?;
    state.service.update_profile(command).await.map_err(|e| {
        api_error(
            StatusCode::BAD_REQUEST,
            "UPDATE_PROFILE_ERROR",
            &e.to_string(),
            false,
        )
    })?;
    Ok(Json(json!({ "success": true })))
}

async fn handle_delete_profile(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(command): Json<crate::desktop::types::ProfileIdCommand>,
) -> Result<impl IntoResponse, Response> {
    validate_origin_and_token(&headers, &state)
        .map_err(|e| api_error(e, "UNAUTHORIZED", "Unauthorized", false))?;
    state.service.delete_profile(command).await.map_err(|e| {
        api_error(
            StatusCode::BAD_REQUEST,
            "DELETE_PROFILE_ERROR",
            &e.to_string(),
            false,
        )
    })?;
    Ok(Json(json!({ "success": true })))
}

// Sets the session's active profile. Refused with 409 while playback is not
// idle - see DesktopService::set_active_profile for why.
async fn handle_set_active_profile(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(command): Json<crate::desktop::types::ProfileIdCommand>,
) -> Result<impl IntoResponse, Response> {
    validate_origin_and_token(&headers, &state)
        .map_err(|e| api_error(e, "UNAUTHORIZED", "Unauthorized", false))?;
    state
        .service
        .set_active_profile(command.profile_id)
        .await
        .map_err(|e| {
            api_error(
                StatusCode::CONFLICT,
                "ACTIVE_PROFILE_ERROR",
                &e.to_string(),
                false,
            )
        })?;
    Ok(Json(json!({ "success": true })))
}

async fn handle_get_lists(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
) -> Result<impl IntoResponse, Response> {
    validate_origin_and_token(&headers, &state)
        .map_err(|e| api_error(e, "UNAUTHORIZED", "Unauthorized", false))?;
    let profile_id = params.get("profile_id").cloned().unwrap_or_default();
    if profile_id.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "MISSING_PROFILE_ID",
            "profile_id is required",
            false,
        ));
    }
    let res = state.service.list_lists(profile_id).await.map_err(|e| {
        tracing::error!("list lists error: {:#}", e);
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "LISTS_ERROR",
            "Failed to list lists",
            true,
        )
    })?;
    Ok(Json(res))
}

async fn handle_create_list(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(command): Json<crate::desktop::types::CreateListCommand>,
) -> Result<impl IntoResponse, Response> {
    validate_origin_and_token(&headers, &state)
        .map_err(|e| api_error(e, "UNAUTHORIZED", "Unauthorized", false))?;
    state.service.create_list(command).await.map_err(|e| {
        api_error(
            StatusCode::BAD_REQUEST,
            "CREATE_LIST_ERROR",
            &e.to_string(),
            false,
        )
    })?;
    Ok(Json(json!({ "success": true })))
}

async fn handle_delete_list(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(command): Json<crate::desktop::types::ListIdCommand>,
) -> Result<impl IntoResponse, Response> {
    validate_origin_and_token(&headers, &state)
        .map_err(|e| api_error(e, "UNAUTHORIZED", "Unauthorized", false))?;
    state.service.delete_list(command).await.map_err(|e| {
        api_error(
            StatusCode::BAD_REQUEST,
            "DELETE_LIST_ERROR",
            &e.to_string(),
            false,
        )
    })?;
    Ok(Json(json!({ "success": true })))
}

async fn handle_add_list_item(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(command): Json<crate::desktop::types::AddListItemCommand>,
) -> Result<impl IntoResponse, Response> {
    validate_origin_and_token(&headers, &state)
        .map_err(|e| api_error(e, "UNAUTHORIZED", "Unauthorized", false))?;
    state.service.add_list_item(command).await.map_err(|e| {
        api_error(
            StatusCode::BAD_REQUEST,
            "ADD_LIST_ITEM_ERROR",
            &e.to_string(),
            false,
        )
    })?;
    Ok(Json(json!({ "success": true })))
}

async fn handle_remove_list_item(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(command): Json<crate::desktop::types::RemoveListItemCommand>,
) -> Result<impl IntoResponse, Response> {
    validate_origin_and_token(&headers, &state)
        .map_err(|e| api_error(e, "UNAUTHORIZED", "Unauthorized", false))?;
    state.service.remove_list_item(command).await.map_err(|e| {
        api_error(
            StatusCode::BAD_REQUEST,
            "REMOVE_LIST_ITEM_ERROR",
            &e.to_string(),
            false,
        )
    })?;
    Ok(Json(json!({ "success": true })))
}

async fn handle_get_watch_status(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
) -> Result<impl IntoResponse, Response> {
    validate_origin_and_token(&headers, &state)
        .map_err(|e| api_error(e, "UNAUTHORIZED", "Unauthorized", false))?;
    let profile_id = params.get("profile_id").cloned().unwrap_or_default();
    if profile_id.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "MISSING_PROFILE_ID",
            "profile_id is required",
            false,
        ));
    }
    let status = params.get("status").cloned();
    let res = state
        .service
        .list_watch_status(profile_id, status)
        .await
        .map_err(|e| {
            api_error(
                StatusCode::BAD_REQUEST,
                "WATCH_STATUS_ERROR",
                &e.to_string(),
                false,
            )
        })?;
    Ok(Json(res))
}

async fn handle_set_watch_status(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(command): Json<crate::desktop::types::SetWatchStatusCommand>,
) -> Result<impl IntoResponse, Response> {
    validate_origin_and_token(&headers, &state)
        .map_err(|e| api_error(e, "UNAUTHORIZED", "Unauthorized", false))?;
    state.service.set_watch_status(command).await.map_err(|e| {
        api_error(
            StatusCode::BAD_REQUEST,
            "SET_WATCH_STATUS_ERROR",
            &e.to_string(),
            false,
        )
    })?;
    Ok(Json(json!({ "success": true })))
}

async fn handle_remove_watch_status(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(command): Json<crate::desktop::types::RemoveWatchStatusCommand>,
) -> Result<impl IntoResponse, Response> {
    validate_origin_and_token(&headers, &state)
        .map_err(|e| api_error(e, "UNAUTHORIZED", "Unauthorized", false))?;
    state
        .service
        .remove_watch_status(command)
        .await
        .map_err(|e| {
            api_error(
                StatusCode::BAD_REQUEST,
                "REMOVE_WATCH_STATUS_ERROR",
                &e.to_string(),
                false,
            )
        })?;
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
