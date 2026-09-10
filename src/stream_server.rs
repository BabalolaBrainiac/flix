use anyhow::{Context, Result};
use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::Response,
    routing::{get, head},
    Router,
};
use futures_util::{Stream, StreamExt};
use serde::Deserialize;
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::{
    io::SeekFrom,
    pin::Pin,
    task::{Context as TaskContext, Poll},
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeekExt};
use tokio::net::TcpListener;
use tokio::sync::{Mutex, OwnedSemaphorePermit, RwLock, Semaphore};
use tokio::task::JoinHandle;
use tokio_util::io::ReaderStream;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::session::{TorrentId, TorrentSession};

// 1 MiB read buffer for streaming responses.
// The tokio-util default is 4 KiB, which is too small for high-bitrate video.
const STREAM_READ_BUF: usize = 1_048_576;
const MAX_ACTIVE_STREAMS: usize = 32;
const STREAM_SLOT_WAIT: Duration = Duration::from_secs(10);

pub struct ServerHandle {
    port: u16,
    token: String,
    task: JoinHandle<()>,
    state: Arc<AppState>,
}

impl ServerHandle {
    pub async fn shutdown(&mut self) {
        self.state.cancel.cancel();
        self.task.abort();
        let _ = (&mut self.task).await;
    }

    pub async fn clear_browser_media(&self) {
        if let Some(media) = self.state.browser.write().await.take() {
            media.cancel.cancel();
        }
    }

    pub async fn browser_media(
        &self,
        id: TorrentId,
        file: usize,
        subtitle: Option<Vec<u8>>,
    ) -> (String, Option<String>) {
        self.clear_browser_media().await;
        let access_token = Uuid::new_v4().to_string();
        let mut url = reqwest::Url::parse(&format!("http://127.0.0.1:{}/b/{id}/{file}", self.port))
            .expect("local stream URL");
        url.query_pairs_mut().append_pair("t", &access_token);
        let mut subtitle_address = url.clone();
        subtitle_address.set_path("/subtitles");
        let subtitle_url = subtitle.as_ref().map(|_| subtitle_address.to_string());
        *self.state.browser.write().await = Some(BrowserMedia {
            access_token,
            id,
            file,
            subtitle: subtitle.map(axum::body::Bytes::from),
            cancel: self.state.cancel.child_token(),
        });
        (url.to_string(), subtitle_url)
    }
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        self.state.cancel.cancel();
        self.task.abort();
    }
}

pub struct AppState {
    pub session: Arc<TorrentSession>,
    pub token: String,
    request_window: Mutex<RequestWindow>,
    stream_slots: Arc<Semaphore>,
    cancel: CancellationToken,
    browser: RwLock<Option<BrowserMedia>>,
}

struct BrowserMedia {
    access_token: String,
    id: TorrentId,
    file: usize,
    subtitle: Option<axum::body::Bytes>,
    cancel: CancellationToken,
}

struct PermittedStream<S> {
    stream: S,
    _permit: OwnedSemaphorePermit,
}

impl<S> Stream for PermittedStream<S>
where
    S: Stream + Unpin,
{
    type Item = S::Item;

    fn poll_next(self: Pin<&mut Self>, context: &mut TaskContext<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.get_mut().stream).poll_next(context)
    }
}

struct RequestWindow {
    started_at: Instant,
    requests: usize,
}

impl RequestWindow {
    fn allow(&mut self) -> bool {
        if self.started_at.elapsed() >= Duration::from_secs(1) {
            self.started_at = Instant::now();
            self.requests = 0;
        }
        if self.requests >= 512 {
            return false;
        }
        self.requests += 1;
        true
    }
}

#[derive(Deserialize)]
struct StreamQuery {
    t: Option<String>,
}

pub async fn serve(session: Arc<TorrentSession>) -> Result<ServerHandle> {
    let token = Uuid::new_v4().to_string();

    let app_state = Arc::new(AppState {
        session,
        token: token.clone(),
        request_window: Mutex::new(RequestWindow {
            started_at: Instant::now(),
            requests: 0,
        }),
        stream_slots: Arc::new(Semaphore::new(MAX_ACTIVE_STREAMS)),
        cancel: CancellationToken::new(),
        browser: RwLock::new(None),
    });

    let app = Router::new()
        .route("/s/{id}/{file}", get(handler))
        .route("/s/{id}/{file}", head(handler))
        .route("/b/{id}/{file}", get(browser_handler))
        .route("/subtitles", get(subtitle_handler))
        .with_state(app_state.clone());

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .context("Failed to bind to 127.0.0.1")?;
    let port = listener.local_addr()?.port();

    let task = tokio::spawn(async move {
        if let Err(error) = axum::serve(listener, app).await {
            tracing::error!(error = %error, "stream server stopped");
        }
    });

    Ok(ServerHandle {
        port,
        token,
        task,
        state: app_state,
    })
}

pub fn stream_url(h: &ServerHandle, id: TorrentId, file: usize) -> String {
    format!(
        "http://127.0.0.1:{}/s/{}/{}?t={}",
        h.port, id, file, h.token
    )
}

fn mime_for(name: &str) -> &'static str {
    let name = name.to_ascii_lowercase();
    if name.ends_with(".mp4") || name.ends_with(".m4v") {
        "video/mp4"
    } else if name.ends_with(".webm") {
        "video/webm"
    } else if name.ends_with(".mkv") {
        "video/x-matroska"
    } else if name.ends_with(".avi") {
        "video/x-msvideo"
    } else {
        "application/octet-stream"
    }
}

// Returns Some(Ok((start, end))) for a valid range,
// Some(Err(())) for unsatisfiable,
// None for no range requested
fn parse_range(
    range_header: Option<&axum::http::HeaderValue>,
    len: u64,
) -> Option<Result<(u64, u64), ()>> {
    let header_val = range_header?.to_str().ok()?;
    if !header_val.starts_with("bytes=") {
        return None;
    }

    if len == 0 {
        return Some(Err(()));
    }

    let range = &header_val["bytes=".len()..];
    let parts: Vec<&str> = range.split('-').collect();
    if parts.len() != 2 {
        return None; // Invalid format
    }

    let start_str = parts[0].trim();
    let end_str = parts[1].trim();

    if start_str.is_empty() {
        // -500: last 500 bytes
        if let Ok(suffix_len) = end_str.parse::<u64>() {
            if suffix_len == 0 {
                return Some(Err(()));
            }
            let start = len.saturating_sub(suffix_len);
            let end = len.saturating_sub(1);
            return Some(Ok((start, end)));
        }
    } else {
        if let Ok(start) = start_str.parse::<u64>() {
            if start >= len {
                return Some(Err(()));
            }
            let mut end = len.saturating_sub(1);
            if !end_str.is_empty() {
                if let Ok(e) = end_str.parse::<u64>() {
                    end = std::cmp::min(e, len.saturating_sub(1));
                }
            }
            if start > end {
                return Some(Err(()));
            }
            return Some(Ok((start, end)));
        }
    }

    None
}

async fn handler(
    State(app): State<Arc<AppState>>,
    Path((id, file)): Path<(usize, usize)>,
    Query(query): Query<StreamQuery>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    // Verify token
    if query.t.as_deref() != Some(&app.token) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let cancel = app.cancel.clone();
    stream_response(app, id, file, headers, cancel).await
}

fn browser_origin(headers: &HeaderMap) -> Result<Option<HeaderValue>, StatusCode> {
    let Some(origin) = headers.get(header::ORIGIN) else {
        return Ok(None);
    };
    let value = origin.to_str().map_err(|_| StatusCode::FORBIDDEN)?;
    let url = reqwest::Url::parse(value).map_err(|_| StatusCode::FORBIDDEN)?;
    if url.scheme() != "http"
        || !matches!(url.host_str(), Some("127.0.0.1" | "localhost"))
        || url.origin().ascii_serialization() != value
    {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(Some(origin.clone()))
}

fn browser_headers(response: &mut Response, origin: Option<HeaderValue>) {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
        .headers_mut()
        .insert(header::VARY, HeaderValue::from_static("Origin"));
    response.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    if let Some(origin) = origin {
        response
            .headers_mut()
            .insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin);
    }
}

async fn browser_handler(
    State(app): State<Arc<AppState>>,
    Path((id, file)): Path<(usize, usize)>,
    Query(query): Query<StreamQuery>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    let origin = browser_origin(&headers)?;
    let cancel = {
        let guard = app.browser.read().await;
        let media = guard.as_ref().ok_or(StatusCode::NOT_FOUND)?;
        if query.t.as_deref() != Some(&media.access_token) || media.id != id || media.file != file {
            return Err(StatusCode::UNAUTHORIZED);
        }
        media.cancel.clone()
    };
    let mut response = stream_response(app, id, file, headers, cancel).await?;
    browser_headers(&mut response, origin);
    Ok(response)
}

async fn subtitle_handler(
    State(app): State<Arc<AppState>>,
    Query(query): Query<StreamQuery>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    let origin = browser_origin(&headers)?;
    if !app.request_window.lock().await.allow() {
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }
    let guard = app.browser.read().await;
    let media = guard.as_ref().ok_or(StatusCode::NOT_FOUND)?;
    if query.t.as_deref() != Some(&media.access_token) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let bytes = media.subtitle.clone().ok_or(StatusCode::NOT_FOUND)?;
    let mut response = Response::builder()
        .header(header::CONTENT_TYPE, "text/vtt; charset=utf-8")
        .body(Body::from(bytes))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    browser_headers(&mut response, origin);
    Ok(response)
}

async fn stream_response(
    app: Arc<AppState>,
    id: usize,
    file: usize,
    headers: HeaderMap,
    cancel: CancellationToken,
) -> Result<Response, StatusCode> {
    if !app.request_window.lock().await.allow() {
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }

    let permit = tokio::time::timeout(STREAM_SLOT_WAIT, app.stream_slots.clone().acquire_owned())
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;

    let (mut stream, len) = app
        .session
        .open_stream(id, file)
        .map_err(|_| StatusCode::NOT_FOUND)?;

    let name = app
        .session
        .files(id)
        .ok()
        .and_then(|files| files.into_iter().find(|f| f.index == file))
        .map(|f| f.name)
        .unwrap_or_else(|| "unknown.bin".to_string());

    let range = parse_range(headers.get(header::RANGE), len);

    match range {
        Some(Err(())) => Ok(Response::builder()
            .status(StatusCode::RANGE_NOT_SATISFIABLE)
            .header(header::CONTENT_RANGE, format!("bytes */{len}"))
            .body(Body::empty())
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?),
        Some(Ok((start, end))) => {
            if stream.seek(SeekFrom::Start(start)).await.is_err() {
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
            let take = stream.take(end - start + 1);
            Ok(Response::builder()
                .status(StatusCode::PARTIAL_CONTENT)
                .header(header::CONTENT_TYPE, mime_for(&name))
                .header(header::ACCEPT_RANGES, "bytes")
                .header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{len}"))
                .header(header::CONTENT_LENGTH, end - start + 1)
                .body(stream_body(take, permit, cancel))
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?)
        }
        None => {
            if len == 0 {
                Ok(Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, mime_for(&name))
                    .header(header::ACCEPT_RANGES, "bytes")
                    .header(header::CONTENT_LENGTH, 0)
                    .body(Body::empty())
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?)
            } else {
                Ok(Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, mime_for(&name))
                    .header(header::ACCEPT_RANGES, "bytes")
                    .header(header::CONTENT_LENGTH, len)
                    .body(stream_body(stream, permit, cancel))
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?)
            }
        }
    }
}

fn stream_body(
    reader: impl AsyncRead + Send + Sync + Unpin + 'static,
    permit: OwnedSemaphorePermit,
    cancel: CancellationToken,
) -> Body {
    Body::from_stream(
        PermittedStream {
            stream: ReaderStream::with_capacity(reader, STREAM_READ_BUF),
            _permit: permit,
        }
        .take_until(cancel.cancelled_owned()),
    )
}

#[cfg(test)]
mod tests {
    use super::{parse_range, RequestWindow, MAX_ACTIVE_STREAMS};
    use axum::http::HeaderValue;
    use std::time::{Duration, Instant};
    use tokio::sync::Semaphore;

    #[tokio::test]
    async fn cancellation_releases_a_pending_stream_reader() {
        let slots = std::sync::Arc::new(Semaphore::new(1));
        let permit = slots.clone().acquire_owned().await.unwrap();
        let (_writer, reader) = tokio::io::duplex(64);
        let cancel = tokio_util::sync::CancellationToken::new();
        let body = super::stream_body(reader, permit, cancel.clone());
        let task = tokio::spawn(axum::body::to_bytes(body, 1024));
        cancel.cancel();
        assert!(tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap()
            .is_empty());
        assert_eq!(slots.available_permits(), 1);
    }

    fn parse(value: &str, length: u64) -> Option<Result<(u64, u64), ()>> {
        parse_range(
            Some(&HeaderValue::from_str(value).expect("range header")),
            length,
        )
    }

    #[test]
    fn parses_range_boundaries() {
        assert_eq!(parse("bytes=0-1023", 2048), Some(Ok((0, 1023))));
        assert_eq!(parse("bytes=0-", 2048), Some(Ok((0, 2047))));
        assert_eq!(parse("bytes=-500", 2048), Some(Ok((1548, 2047))));
        assert_eq!(parse("bytes=9999-", 2048), Some(Err(())));
        assert_eq!(parse("bytes=20-10", 2048), Some(Err(())));
        assert_eq!(parse("bytes=-0", 2048), Some(Err(())));
        assert_eq!(parse("bytes=0-1", 0), Some(Err(())));
    }

    #[test]
    fn limits_requests_and_resets_the_window() {
        let mut window = RequestWindow {
            started_at: Instant::now(),
            requests: 511,
        };
        assert!(window.allow());
        assert!(!window.allow());
        window.started_at = Instant::now() - Duration::from_secs(2);
        assert!(window.allow());
    }

    #[tokio::test]
    async fn limits_active_reader_buffers() {
        let slots = std::sync::Arc::new(Semaphore::new(MAX_ACTIVE_STREAMS));
        let mut permits = Vec::new();
        for _ in 0..MAX_ACTIVE_STREAMS {
            permits.push(slots.clone().try_acquire_owned().expect("stream slot"));
        }

        assert!(slots.clone().try_acquire_owned().is_err());
        drop(permits.pop());
        assert!(slots.clone().try_acquire_owned().is_ok());
    }
}
