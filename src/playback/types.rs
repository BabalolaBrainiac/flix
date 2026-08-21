use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MediaRef {
    Movie {
        catalog_id: String,
        title: String,
    },
    Episode {
        catalog_id: String,
        stream_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        imdb_id: Option<String>,
        season: u32,
        episode: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
    },
}

impl MediaRef {
    pub fn display_title(&self) -> String {
        match self {
            MediaRef::Movie { title, .. } => title.clone(),
            MediaRef::Episode {
                season,
                episode,
                title,
                ..
            } => {
                let formatted_ep = format!("S{:02}E{:02}", season, episode);
                match title {
                    Some(t) if !t.trim().is_empty() => format!("{} · {}", formatted_ep, t.trim()),
                    _ => format!("{} · Title unavailable", formatted_ep),
                }
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SafeError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

impl SafeError {
    pub fn new(code: impl Into<String>, message: impl Into<String>, retryable: bool) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            retryable,
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new("NOT_FOUND", message, false)
    }

    pub fn network_error(message: impl Into<String>) -> Self {
        Self::new("NETWORK_ERROR", message, true)
    }

    pub fn player_unavailable(message: impl Into<String>) -> Self {
        Self::new("PLAYER_UNAVAILABLE", message, false)
    }

    pub fn playback_failed(message: impl Into<String>) -> Self {
        Self::new("PLAYBACK_FAILED", message, true)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum PlaybackSnapshot {
    Idle,
    ResolvingSource {
        media: MediaRef,
    },
    LoadingTorrent {
        media: MediaRef,
        quality: String,
    },
    Buffering {
        media: MediaRef,
        quality: String,
        file_name: String,
    },
    FindingSubtitle {
        media: MediaRef,
        quality: String,
    },
    DownloadingSubtitle {
        media: MediaRef,
        quality: String,
        subtitle_name: String,
    },
    LaunchingPlayer {
        media: MediaRef,
        quality: String,
        player_name: String,
    },
    Playing {
        media: MediaRef,
        title: String,
        quality: String,
        file_length: u64,
        player_name: String,
        subtitle_ready: bool,
        has_next: bool,
    },
    Stopping,
    Failed {
        error: SafeError,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OperationAccepted {
    pub operation_id: String,
    pub status: String,
}
