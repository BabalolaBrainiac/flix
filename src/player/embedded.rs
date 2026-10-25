use super::{Player, PlayerExit, PlayerKind, LARGE_FILE_THRESHOLD};
use anyhow::{anyhow, Result};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

pub const EMBEDDED_MPV_PATH: &str = "embedded://libmpv";

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct EmbeddedLaunchRequest {
    pub url: String,
    pub title: String,
    pub file_length: u64,
    pub demuxer_max_bytes: u64,
    pub cache_secs: Option<u32>,
    pub audio_languages: Vec<String>,
    pub subtitle_languages: Vec<String>,
    pub audio_track: Option<usize>,
    pub subtitle_track: Option<usize>,
    pub subtitle_files: Vec<String>,
    pub start_at: Option<f64>,
}

impl EmbeddedLaunchRequest {
    pub fn from_options(url: &str, opts: &super::PlaybackOptions) -> Self {
        let is_large = opts.file_length >= LARGE_FILE_THRESHOLD;
        let demuxer_max_bytes = if is_large {
            512 * 1024 * 1024
        } else {
            256 * 1024 * 1024
        };
        let cache_secs = is_large.then_some(30);
        let audio_track = opts.selected_tracks.as_ref().map(|tracks| tracks.audio + 1);
        let subtitle_track = opts
            .selected_tracks
            .as_ref()
            .and_then(|tracks| tracks.subtitle.map(|sub| sub + 1));

        Self {
            url: url.to_string(),
            title: opts.title.clone(),
            file_length: opts.file_length,
            demuxer_max_bytes,
            cache_secs,
            audio_languages: opts.audio_languages.clone(),
            subtitle_languages: opts.subtitle_languages.clone(),
            audio_track,
            subtitle_track,
            subtitle_files: opts.subtitle_files.clone(),
            start_at: opts.start_at,
        }
    }
}

pub trait EmbeddedPlayerHost: Send + Sync {
    fn launch(&self, request: EmbeddedLaunchRequest) -> Result<()>;
    fn stop(&self) -> Result<()>;
}

struct RegisteredHost {
    host: Arc<dyn EmbeddedPlayerHost>,
    pending_exit: Mutex<Option<PlayerExit>>,
    started_at: Mutex<Option<Instant>>,
}

static HOST_CELL: OnceLock<Mutex<Option<Arc<RegisteredHost>>>> = OnceLock::new();

fn host_slot() -> &'static Mutex<Option<Arc<RegisteredHost>>> {
    HOST_CELL.get_or_init(|| Mutex::new(None))
}

/// Registers an in-process media player host for platforms such as iOS.
pub fn register_host(host: Arc<dyn EmbeddedPlayerHost>) {
    *host_slot().lock().unwrap() = Some(Arc::new(RegisteredHost {
        host,
        pending_exit: Mutex::new(None),
        started_at: Mutex::new(None),
    }));
}

/// Clears the registered in-process media player host.
pub fn clear_host() {
    *host_slot().lock().unwrap() = None;
}

/// Returns the embedded player descriptor when a host is registered.
pub fn detected_player() -> Option<Player> {
    let has_host = host_slot().lock().unwrap().is_some();
    has_host.then(|| Player {
        kind: PlayerKind::Mpv,
        path: PathBuf::from(EMBEDDED_MPV_PATH),
        version: Some("MPVKit".to_string()),
    })
}

pub fn is_embedded_player(player: &Player) -> bool {
    player.path.to_str() == Some(EMBEDDED_MPV_PATH)
}

/// Records that the embedded player closed.
pub fn report_exit(success: bool, exit_code: Option<i32>) {
    let Some(registered) = host_slot().lock().unwrap().clone() else {
        return;
    };
    let runtime_ms = registered
        .started_at
        .lock()
        .unwrap()
        .map(|started| started.elapsed().as_millis() as u64)
        .unwrap_or(0);
    *registered.pending_exit.lock().unwrap() = Some(PlayerExit {
        success,
        exit_code,
        signal: None,
        runtime_ms,
    });
}

#[derive(Clone)]
pub struct EmbeddedSession {
    registered: Arc<RegisteredHost>,
}

impl EmbeddedSession {
    pub fn launch(url: &str, opts: &super::PlaybackOptions) -> Result<Self> {
        let registered = host_slot()
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| anyhow!("No embedded media player host is registered"))?;
        *registered.pending_exit.lock().unwrap() = None;
        *registered.started_at.lock().unwrap() = Some(Instant::now());
        let request = EmbeddedLaunchRequest::from_options(url, opts);
        registered.host.launch(request)?;
        Ok(Self { registered })
    }

    pub fn poll_exit(&self) -> Option<PlayerExit> {
        self.registered.pending_exit.lock().unwrap().take()
    }

    pub fn has_exited(&self) -> bool {
        self.registered.pending_exit.lock().unwrap().is_some()
    }

    pub fn stop(&self) -> Result<()> {
        *self.registered.pending_exit.lock().unwrap() = None;
        self.registered.host.stop()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::playback::anime::SelectedTracks;
    use crate::player::PlaybackOptions;

    #[test]
    fn builds_launch_request_with_verified_tracks_and_4k_cache() {
        let opts = PlaybackOptions::new("Episode 1".to_string(), LARGE_FILE_THRESHOLD)
            .with_selected_tracks(Some(SelectedTracks {
                audio: 1,
                subtitle: Some(2),
            }))
            .with_subtitles(vec!["/tmp/en.srt".to_string()]);
        let req = EmbeddedLaunchRequest::from_options("http://127.0.0.1:9000/s/0/0?t=abc", &opts);
        assert_eq!(req.title, "Episode 1");
        assert_eq!(req.demuxer_max_bytes, 512 * 1024 * 1024);
        assert_eq!(req.cache_secs, Some(30));
        assert_eq!(req.audio_track, Some(2));
        assert_eq!(req.subtitle_track, Some(3));
        assert_eq!(req.subtitle_files, vec!["/tmp/en.srt".to_string()]);
    }
}
