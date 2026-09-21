//! Writes watch-status progress from the outside, without adding any code to
//! the playback path itself.
//!
//! `PlaybackCoordinator` already broadcasts every state change on a
//! `tokio::sync::watch` channel (`coordinator.subscribe()`), the same one the
//! `/api/events` SSE handler reads. This tracker is one more subscriber of
//! that channel: it never touches the coordinator, never blocks a sender,
//! and every database write runs inside `spawn_blocking` so a slow disk
//! write can never stall the async runtime the player and torrent I/O also
//! run on. See the profiles/lists/watch-status implementation plan's
//! Architecture decisions for the full rationale.
//!
//! Scope: this records that a title is `in_progress` when playback reaches
//! `Playing` or `Browser`, and updates the resume position for the `Browser`
//! target (the only snapshot variant that carries a live position; an
//! external player like mpv/VLC reports position over its own IPC channel,
//! outside this snapshot type). It does not infer `watched` - there is no
//! reliable "finished naturally" signal at this layer, and guessing wrong
//! would be worse than leaving it to an explicit user action.

use crate::playback::{MediaRef, PlaybackCoordinator, PlaybackSnapshot};
use crate::profiles::{ProfileStore, TitleMeta, WatchState};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// Minimum time between two resume-position writes for the same title while
/// it keeps playing. State transitions (a new title, leaving playback)
/// always write immediately regardless of this.
const RESUME_WRITE_INTERVAL: Duration = Duration::from_secs(15);

/// Spawns the tracker as a background task and returns immediately. The task
/// runs until the coordinator's snapshot channel closes (app shutdown).
pub fn spawn(
    coordinator: Arc<PlaybackCoordinator>,
    profiles: Arc<ProfileStore>,
    current_profile_id: Arc<Mutex<Option<String>>>,
) {
    tokio::spawn(run(coordinator, profiles, current_profile_id));
}

async fn run(
    coordinator: Arc<PlaybackCoordinator>,
    profiles: Arc<ProfileStore>,
    current_profile_id: Arc<Mutex<Option<String>>>,
) {
    let mut rx = coordinator.subscribe();
    let mut last_write: Option<(String, Instant)> = None;

    loop {
        let snapshot = rx.borrow().clone();
        if let Some(candidate) = progress_candidate(&snapshot) {
            let should_write = match &last_write {
                Some((catalog_id, at)) if *catalog_id == candidate.catalog_id => {
                    at.elapsed() >= RESUME_WRITE_INTERVAL
                }
                _ => true,
            };
            if should_write {
                if let Some(profile_id) = current_profile_id.lock().await.clone() {
                    write_progress(&profiles, profile_id, candidate.clone()).await;
                }
                last_write = Some((candidate.catalog_id, Instant::now()));
            }
        } else {
            last_write = None;
        }

        if rx.changed().await.is_err() {
            // The coordinator was dropped - the app is shutting down.
            return;
        }
    }
}

async fn write_progress(
    profiles: &Arc<ProfileStore>,
    profile_id: String,
    candidate: ProgressCandidate,
) {
    let profiles = Arc::clone(profiles);
    let result = tokio::task::spawn_blocking(move || {
        // No title data is available from a playback snapshot:
        // `set_watch_status` keeps whatever was already stored (from the
        // add-to-list/set-status popover) rather than blanking it out on
        // every progress tick.
        profiles.set_watch_status(
            &profile_id,
            &candidate.catalog_id,
            WatchState::InProgress,
            candidate.episode_id.as_deref(),
            candidate.resume_seconds,
            &TitleMeta::default(),
        )
    })
    .await;

    match result {
        Ok(Ok(())) => {}
        Ok(Err(error)) => tracing::warn!("Failed to record playback progress: {error:#}"),
        Err(error) => tracing::warn!("Progress tracker write task panicked: {error}"),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProgressCandidate {
    catalog_id: String,
    episode_id: Option<String>,
    resume_seconds: Option<i64>,
}

/// Returns the progress this snapshot implies, or `None` when the snapshot
/// is not a playing state (loading, idle, stopping, failed) - those produce
/// no write and reset the throttle so the next real playback writes fresh.
fn progress_candidate(snapshot: &PlaybackSnapshot) -> Option<ProgressCandidate> {
    match snapshot {
        PlaybackSnapshot::Playing { media, .. } => Some(candidate_from_media(media, None)),
        PlaybackSnapshot::Browser {
            media, position_ms, ..
        } => Some(candidate_from_media(media, Some(*position_ms))),
        _ => None,
    }
}

fn candidate_from_media(media: &MediaRef, position_ms: Option<u64>) -> ProgressCandidate {
    let resume_seconds = position_ms.map(|ms| (ms / 1000) as i64);
    match media {
        MediaRef::Movie { catalog_id, .. } => ProgressCandidate {
            catalog_id: catalog_id.clone(),
            episode_id: None,
            resume_seconds,
        },
        MediaRef::Episode {
            catalog_id,
            season,
            episode,
            ..
        } => ProgressCandidate {
            catalog_id: catalog_id.clone(),
            episode_id: Some(format!("{catalog_id}:{season}:{episode}")),
            resume_seconds,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn movie(catalog_id: &str) -> MediaRef {
        MediaRef::Movie {
            catalog_id: catalog_id.to_string(),
            title: "A Movie".to_string(),
        }
    }

    fn episode(catalog_id: &str, season: u32, episode_no: u32) -> MediaRef {
        MediaRef::Episode {
            catalog_id: catalog_id.to_string(),
            stream_id: "stream".to_string(),
            imdb_id: None,
            season,
            episode: episode_no,
            title: None,
        }
    }

    #[test]
    fn playing_a_movie_yields_no_episode_id() {
        let snapshot = PlaybackSnapshot::Playing {
            media: movie("tt1"),
            title: "A Movie".to_string(),
            quality: "4K".to_string(),
            file_length: 0,
            player_name: "mpv".to_string(),
            subtitle_ready: false,
            has_next: false,
        };
        let candidate = progress_candidate(&snapshot).unwrap();
        assert_eq!(candidate.catalog_id, "tt1");
        assert_eq!(candidate.episode_id, None);
        assert_eq!(candidate.resume_seconds, None);
    }

    #[test]
    fn playing_an_episode_formats_the_episode_id_consistently_with_stremio() {
        let snapshot = PlaybackSnapshot::Playing {
            media: episode("tt2", 1, 3),
            title: "A Show".to_string(),
            quality: "1080p".to_string(),
            file_length: 0,
            player_name: "mpv".to_string(),
            subtitle_ready: false,
            has_next: true,
        };
        let candidate = progress_candidate(&snapshot).unwrap();
        assert_eq!(candidate.episode_id.as_deref(), Some("tt2:1:3"));
    }

    #[test]
    fn browser_playback_carries_a_resume_position_in_seconds() {
        let snapshot = PlaybackSnapshot::Browser {
            media: movie("tt3"),
            title: "A Movie".to_string(),
            quality: "1080p".to_string(),
            playback_id: "pb1".to_string(),
            stream_url: "http://x".to_string(),
            mime_type: "video/mp4".to_string(),
            subtitle_url: None,
            phase: crate::playback::BrowserPhase::Playing,
            position_ms: 92_500,
            has_next: false,
        };
        let candidate = progress_candidate(&snapshot).unwrap();
        assert_eq!(candidate.resume_seconds, Some(92));
    }

    #[test]
    fn idle_and_transitional_states_yield_no_candidate() {
        assert_eq!(progress_candidate(&PlaybackSnapshot::Idle), None);
        assert_eq!(progress_candidate(&PlaybackSnapshot::Stopping), None);
        assert_eq!(
            progress_candidate(&PlaybackSnapshot::LoadingTorrent {
                media: movie("tt1"),
                quality: "4K".to_string(),
            }),
            None
        );
    }
}
