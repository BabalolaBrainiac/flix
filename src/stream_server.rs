use anyhow::{Context, Result};
use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::Response,
    routing::{get, head},
    Router,
};
use serde::Deserialize;
use std::io::SeekFrom;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio_util::io::ReaderStream;
use uuid::Uuid;

use crate::session::{TorrentId, TorrentSession};

// 1 MiB read buffer for streaming responses.
// The tokio-util default is 4 KiB, which is too small for high-bitrate video.
const STREAM_READ_BUF: usize = 1_048_576;

pub struct ServerHandle {
    port: u16,
    token: String,
    task: JoinHandle<()>,
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        self.task.abort();
    }
}

pub struct AppState {
    pub session: Arc<TorrentSession>,
    pub token: String,
    request_window: Mutex<RequestWindow>,
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
    });

    let app = Router::new()
        .route("/s/{id}/{file}", get(handler))
        .route("/s/{id}/{file}", head(handler))
        .with_state(app_state);

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .context("Failed to bind to 127.0.0.1")?;
    let port = listener.local_addr()?.port();

    let task = tokio::spawn(async move {
        if let Err(error) = axum::serve(listener, app).await {
            tracing::error!(error = %error, "stream server stopped");
        }
    });

    Ok(ServerHandle { port, token, task })
}

pub fn stream_url(h: &ServerHandle, id: TorrentId, file: usize) -> String {
    format!(
        "http://127.0.0.1:{}/s/{}/{}?t={}",
        h.port, id, file, h.token
    )
}

fn mime_for(name: &str) -> &'static str {
    if name.ends_with(".mp4") {
        "video/mp4"
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
    if !app.request_window.lock().await.allow() {
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }

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
                .body(Body::from_stream(ReaderStream::with_capacity(
                    take,
                    STREAM_READ_BUF,
                )))
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
                    .body(Body::from_stream(ReaderStream::with_capacity(
                        stream,
                        STREAM_READ_BUF,
                    )))
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_range, RequestWindow};
    use axum::http::HeaderValue;
    use std::time::{Duration, Instant};

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
}
