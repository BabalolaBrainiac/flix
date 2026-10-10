//! Background-buffering playback queue (issue #43).
//!
//! A queued item downloads in the background while the foreground item
//! keeps playing. It never competes for more than the foreground stream can
//! spare: a backoff guard watches the foreground `PlaybackSnapshot` and caps
//! the queued item's download rate the instant the foreground struggles,
//! lifting the cap only once it recovers. A queued item buffers only its
//! first `BUFFER_TARGET_PERCENT` of its file, sequentially from the start,
//! then pauses rather than holding a full extra copy on disk or in memory.
//! A queued item stays in the queue, with its buffer, until the user plays
//! it or removes it. No timer removes it: a timer cleared the queue during
//! a long foreground watch, which is when the queue is in use.

use crate::config::PlaybackCache;
use crate::playback::types::{BrowserPhase, MediaRef, PlaybackSnapshot};
use crate::session::{TorrentFile, TorrentId, TorrentSession};
use anyhow::{Context, Result};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::AsyncReadExt;
use tokio::sync::{watch, Mutex};
use tokio_util::sync::CancellationToken;

/// A queued item buffers only this fraction of its file before pausing. A
/// starting point to tune against the performance benchmark, not a fixed
/// final number.
pub const BUFFER_TARGET_PERCENT: u64 = 30;

/// The floor a queued item's download rate drops to while the foreground
/// stream is unhealthy. Not zero: `set_download_rate_limit` takes a
/// non-zero rate, and a small floor still lets librqbit service tracker and
/// peer housekeeping instead of going fully silent.
const BACKOFF_FLOOR_BPS: u32 = 4096;

/// How many items a user can hold in the background queue at once.
pub const MAX_QUEUED_ITEMS: usize = 2;

const BUFFER_READ_CHUNK: usize = 256 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueItemState {
    Buffering,
    Ready,
}

/// A point-in-time view of one queued item, safe to serialize for the API
/// and the UI.
#[derive(Debug, Clone)]
pub struct QueueItemSnapshot {
    pub id: String,
    pub media: MediaRef,
    pub state: QueueItemState,
    pub buffered_bytes: u64,
    pub target_bytes: u64,
}

/// One item buffering in the background. Holds its own `TorrentSession` and
/// `PlaybackCache`, mirroring `ActiveSession`'s design, so a queued item
/// never shares a session or a cache directory with the foreground stream
/// or with another queued item.
pub struct QueuedSession {
    pub id: String,
    pub media: MediaRef,
    pub quality: String,
    pub is_anime: bool,
    pub session: Arc<TorrentSession>,
    pub torrent_id: TorrentId,
    pub video: TorrentFile,
    /// `None` only after `take_for_promotion`, which moves it into the new
    /// foreground `ActiveSession`.
    pub cache: Option<PlaybackCache>,
    state_tx: watch::Sender<QueueItemState>,
    buffered_bytes: Arc<AtomicU64>,
    /// Cancels the buffering reader and the backoff guard together.
    /// Playing or removing a queued item cancels this first.
    pub tasks_cancel: CancellationToken,
    /// Set by `take_for_promotion`, so `Drop` does not cancel a session
    /// that has just become the foreground session.
    promoted: bool,
}

/// What a queued item hands off to become the foreground `ActiveSession`.
pub struct PromotedQueueItem {
    pub media: MediaRef,
    pub quality: String,
    pub is_anime: bool,
    pub session: Arc<TorrentSession>,
    pub torrent_id: TorrentId,
    pub video: TorrentFile,
    pub cache: PlaybackCache,
    /// The item's buffering state at the moment it was taken for
    /// promotion. librqbit's `unpause` fails with "torrent is already
    /// live" if the torrent was never paused, so the caller must only
    /// unpause an item that had reached `Ready` (and so was paused at its
    /// 30% target) — a still-`Buffering` item is already live and needs no
    /// resume at all.
    pub state_at_promotion: QueueItemState,
}

impl Drop for QueuedSession {
    fn drop(&mut self) {
        self.tasks_cancel.cancel();
        if self.promoted {
            return;
        }
        self.session.cancel();
    }
}

impl QueuedSession {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: String,
        media: MediaRef,
        quality: String,
        is_anime: bool,
        session: Arc<TorrentSession>,
        torrent_id: TorrentId,
        video: TorrentFile,
        cache: PlaybackCache,
    ) -> Self {
        let (state_tx, _) = watch::channel(QueueItemState::Buffering);
        Self {
            id,
            media,
            quality,
            is_anime,
            session,
            torrent_id,
            video,
            cache: Some(cache),
            state_tx,
            buffered_bytes: Arc::new(AtomicU64::new(0)),
            tasks_cancel: CancellationToken::new(),
            promoted: false,
        }
    }

    pub fn state(&self) -> QueueItemState {
        *self.state_tx.borrow()
    }

    pub fn target_bytes(&self) -> u64 {
        buffer_target_bytes(self.video.length)
    }

    pub fn snapshot(&self) -> QueueItemSnapshot {
        QueueItemSnapshot {
            id: self.id.clone(),
            media: self.media.clone(),
            state: self.state(),
            buffered_bytes: self.buffered_bytes.load(Ordering::Relaxed),
            target_bytes: self.target_bytes(),
        }
    }

    /// Takes this item's fields so they can become the foreground
    /// `ActiveSession`, and marks the item promoted so its eventual `Drop`
    /// does not cancel the session being handed off. Panics if called twice
    /// on the same item (a coordinator bug: an item can only be promoted
    /// once, by whoever holds it after removing it from the background
    /// queue).
    pub fn take_for_promotion(&mut self) -> PromotedQueueItem {
        let state_at_promotion = self.state();
        self.promoted = true;
        PromotedQueueItem {
            media: self.media.clone(),
            quality: self.quality.clone(),
            is_anime: self.is_anime,
            session: self.session.clone(),
            torrent_id: self.torrent_id,
            video: self.video.clone(),
            cache: self
                .cache
                .take()
                .expect("a queued item can only be promoted once"),
            state_at_promotion,
        }
    }
}

/// The byte count a queued item should buffer to before pausing.
pub fn buffer_target_bytes(file_length: u64) -> u64 {
    file_length.saturating_mul(BUFFER_TARGET_PERCENT) / 100
}

/// Decides whether the foreground stream is healthy enough that background
/// buffering may run at full speed. `false` means every background session
/// must drop to its backoff floor immediately. A pure function of the
/// snapshot alone, so the rule is testable without a real network or a real
/// `TorrentSession`.
pub fn is_foreground_healthy(snapshot: &PlaybackSnapshot) -> bool {
    match snapshot {
        PlaybackSnapshot::Idle
        | PlaybackSnapshot::Stopping
        | PlaybackSnapshot::Failed { .. }
        | PlaybackSnapshot::Playing { .. } => true,
        PlaybackSnapshot::Browser { phase, .. } => *phase != BrowserPhase::Buffering,
        PlaybackSnapshot::ResolvingSource { .. }
        | PlaybackSnapshot::LoadingTorrent { .. }
        | PlaybackSnapshot::Buffering { .. }
        | PlaybackSnapshot::FindingSubtitle { .. }
        | PlaybackSnapshot::DownloadingSubtitle { .. }
        | PlaybackSnapshot::LaunchingPlayer { .. } => false,
    }
}

/// Watches the foreground snapshot and dials a queued item's download rate
/// accordingly: uncapped while the foreground is healthy, floored the
/// instant it is not. Returns when `cancel` fires or the foreground
/// coordinator shuts down.
pub async fn run_backoff_guard(
    mut foreground: watch::Receiver<PlaybackSnapshot>,
    session: Arc<TorrentSession>,
    cancel: CancellationToken,
) {
    // Apply the current state once at startup, not just on the next change.
    let healthy = is_foreground_healthy(&foreground.borrow());
    session.set_download_rate_limit(if healthy {
        None
    } else {
        Some(BACKOFF_FLOOR_BPS)
    });

    loop {
        tokio::select! {
            _ = cancel.cancelled() => return,
            changed = foreground.changed() => {
                if changed.is_err() {
                    return;
                }
                let healthy = is_foreground_healthy(&foreground.borrow());
                session.set_download_rate_limit(if healthy { None } else { Some(BACKOFF_FLOOR_BPS) });
            }
        }
    }
}

/// Buffers a queued item sequentially from the start of its file up to its
/// 30% target, then pauses it. Keeps an open stream reader so librqbit's
/// stream-position piece priority stays ahead of its own fallback order —
/// without a reader, a queued item would otherwise grab pieces from the end
/// of the file almost immediately. Reads are discarded as they arrive, so
/// this holds at most `BUFFER_READ_CHUNK` bytes at a time, never the whole
/// target in memory.
pub async fn buffer_to_threshold(item: Arc<Mutex<QueuedSession>>) -> Result<()> {
    let (session, torrent_id, file_index, target_bytes, buffered_bytes, cancel) = {
        let guard = item.lock().await;
        (
            guard.session.clone(),
            guard.torrent_id,
            guard.video.index,
            guard.target_bytes(),
            guard.buffered_bytes.clone(),
            guard.tasks_cancel.clone(),
        )
    };

    if target_bytes > 0 {
        let (mut reader, _length) = session.open_stream(torrent_id, file_index)?;
        let mut buffer = vec![0u8; BUFFER_READ_CHUNK];
        let mut read_total = 0u64;
        while read_total < target_bytes {
            if cancel.is_cancelled() {
                return Ok(());
            }
            let remaining = (target_bytes - read_total).min(BUFFER_READ_CHUNK as u64) as usize;
            let n = tokio::select! {
                _ = cancel.cancelled() => return Ok(()),
                read = reader.read(&mut buffer[..remaining]) => read.context("Background buffering read failed")?,
            };
            if n == 0 {
                break;
            }
            read_total += n as u64;
            buffered_bytes.store(read_total, Ordering::Relaxed);
        }
    }

    if cancel.is_cancelled() {
        return Ok(());
    }

    session.pause(torrent_id).await?;

    item.lock()
        .await
        .state_tx
        .send_replace(QueueItemState::Ready);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::playback::types::MediaRef;

    fn playing_snapshot() -> PlaybackSnapshot {
        PlaybackSnapshot::Playing {
            media: MediaRef::Movie {
                catalog_id: "tt0111161".to_string(),
                title: "The Shawshank Redemption".to_string(),
            },
            title: "The Shawshank Redemption".to_string(),
            quality: "1080p".to_string(),
            file_length: 1_000_000,
            player_name: "mpv".to_string(),
            subtitle_ready: true,
            has_next: false,
        }
    }

    fn buffering_snapshot() -> PlaybackSnapshot {
        PlaybackSnapshot::Buffering {
            media: MediaRef::Movie {
                catalog_id: "tt0111161".to_string(),
                title: "The Shawshank Redemption".to_string(),
            },
            quality: "1080p".to_string(),
            file_name: "movie.mp4".to_string(),
        }
    }

    #[test]
    fn computes_30_percent_of_the_file_length() {
        assert_eq!(buffer_target_bytes(1_000), 300);
        assert_eq!(buffer_target_bytes(0), 0);
        // Rounds down rather than over-buffering.
        assert_eq!(buffer_target_bytes(10), 3);
    }

    #[test]
    fn treats_playing_and_idle_as_healthy() {
        assert!(is_foreground_healthy(&PlaybackSnapshot::Idle));
        assert!(is_foreground_healthy(&PlaybackSnapshot::Stopping));
        assert!(is_foreground_healthy(&playing_snapshot()));
    }

    #[test]
    fn treats_buffering_and_loading_as_unhealthy() {
        assert!(!is_foreground_healthy(&buffering_snapshot()));
        assert!(!is_foreground_healthy(&PlaybackSnapshot::LoadingTorrent {
            media: MediaRef::Movie {
                catalog_id: "tt0111161".to_string(),
                title: "x".to_string(),
            },
            quality: "1080p".to_string(),
        }));
    }

    #[test]
    fn treats_a_buffering_browser_phase_as_unhealthy_but_other_phases_as_healthy() {
        use crate::playback::types::BrowserPhase;
        let browser = |phase: BrowserPhase| PlaybackSnapshot::Browser {
            media: MediaRef::Movie {
                catalog_id: "tt0111161".to_string(),
                title: "x".to_string(),
            },
            title: "x".to_string(),
            quality: "1080p".to_string(),
            playback_id: "id".to_string(),
            stream_url: "http://127.0.0.1/s".to_string(),
            mime_type: "video/mp4".to_string(),
            subtitle_url: None,
            phase,
            position_ms: 0,
            has_next: false,
        };
        assert!(!is_foreground_healthy(&browser(BrowserPhase::Buffering)));
        assert!(is_foreground_healthy(&browser(BrowserPhase::Ready)));
        assert!(is_foreground_healthy(&browser(BrowserPhase::Playing)));
        assert!(is_foreground_healthy(&browser(BrowserPhase::Paused)));
    }

    // The tests below are the mandatory performance/behavior validation for
    // issue #43: they run against a real local seed and real TorrentSessions,
    // not mocks, so they prove the actual mechanism (the live rate-limiter
    // dial, and the 30%-then-pause reader) rather than just the pure
    // decision functions above.
    //
    // These are deterministic functional checks of the mechanism itself
    // (does the backoff guard actually move the rate limiter in response to
    // a snapshot change; does buffering actually stop at the target and
    // pause) rather than a statistical multi-scenario throughput benchmark.
    // A fuller A/B/C throughput comparison (as sketched during design) is
    // real future work, left for when there is a release-time benchmarking
    // harness to run it in; it would be too flaky as a `cargo test` given
    // two synthetic local sessions racing each other over loopback.

    use librqbit::{create_torrent, AddTorrent, AddTorrentOptions, CreateTorrentOptions, Session};
    use std::io::Write;
    use std::num::NonZeroU32;
    use std::time::Duration;

    /// Seeds `content` on a local, upload-throttled `librqbit::Session`
    /// bound to an OS-assigned port, so a downloading `TorrentSession` can
    /// connect to it directly via `initial_peers` with no DHT/tracker and no
    /// real network dependency. Mirrors the pattern in `src/session.rs`'s
    /// own tests. Returns the seed session (kept alive by the caller) and
    /// the torrent's raw bytes.
    async fn seed_locally(
        content: &[u8],
        piece_length: u32,
        upload_bps: u32,
    ) -> (Arc<Session>, std::net::SocketAddr, Vec<u8>) {
        let seed_dir = tempfile::tempdir().unwrap();
        let path = seed_dir.path().join("content.bin");
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(content).unwrap();
        drop(file);
        let torrent_bytes = create_torrent(
            &path,
            CreateTorrentOptions {
                piece_length: Some(piece_length),
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .as_bytes()
        .unwrap()
        .to_vec();

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let seed = Session::new_with_opts(
            seed_dir.keep(),
            librqbit::SessionOptions {
                disable_dht: true,
                listen_port_range: Some(address.port()..address.port() + 1),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        seed.ratelimits.set_upload_bps(NonZeroU32::new(upload_bps));
        let seed_handle = seed
            .add_torrent(
                AddTorrent::from_bytes(torrent_bytes.clone()),
                Some(AddTorrentOptions {
                    overwrite: true,
                    ..Default::default()
                }),
            )
            .await
            .unwrap()
            .into_handle()
            .unwrap();
        seed_handle.wait_until_initialized().await.unwrap();
        (seed, address, torrent_bytes)
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn backoff_guard_dials_the_live_rate_limiter_in_response_to_foreground_health() {
        // A deliberately slow seed: fast enough that an uncapped session
        // visibly makes progress within the test's timeout, slow enough
        // that BACKOFF_FLOOR_BPS is a real, measurable restriction by
        // comparison.
        let content = vec![7u8; 4 * 1024 * 1024];
        let (_seed, address, torrent_bytes) = seed_locally(&content, 256 * 1024, 512 * 1024).await;

        let download_dir = tempfile::tempdir().unwrap();
        let background = TorrentSession::new(download_dir.path()).await.unwrap();
        let id = background
            .add_bytes_with_peers(torrent_bytes, vec![address])
            .await
            .unwrap();

        let (snapshot_tx, snapshot_rx) = watch::channel(PlaybackSnapshot::Buffering {
            media: MediaRef::Movie {
                catalog_id: "tt0111161".to_string(),
                title: "x".to_string(),
            },
            quality: "1080p".to_string(),
            file_name: "x.mp4".to_string(),
        });
        let session = Arc::new(background);
        let cancel = CancellationToken::new();
        let guard = tokio::spawn(run_backoff_guard(
            snapshot_rx,
            session.clone(),
            cancel.clone(),
        ));

        // Foreground reported unhealthy: the floor applies. A short wait
        // (not longer) keeps the backlog of requests already queued under
        // the old, near-zero limiter small, since a token-bucket limiter
        // only affects requests that acquire from it *after* a swap — a
        // request already waiting on the old limiter finishes on the old
        // schedule regardless of a later swap, so a deep backlog would
        // otherwise delay the recovery check below for reasons that have
        // nothing to do with whether the live dial itself works.
        tokio::time::sleep(Duration::from_millis(200)).await;
        let throttled_progress = session.progress(id).unwrap().done;
        assert!(
            throttled_progress < 128 * 1024,
            "expected the backoff floor to keep progress small, got {throttled_progress} bytes"
        );

        // Foreground recovers: the cap lifts and progress should resume.
        // Generous timeout for the same in-flight-backlog reason above.
        snapshot_tx.send(playing_snapshot()).unwrap();
        tokio::time::timeout(Duration::from_secs(25), async {
            while session.progress(id).unwrap().done <= throttled_progress {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("progress must resume once the foreground is healthy again");

        cancel.cancel();
        guard.await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn buffers_to_the_target_then_pauses_without_downloading_the_whole_file() {
        let content = vec![9u8; 2 * 1024 * 1024];
        let (_seed, address, torrent_bytes) =
            seed_locally(&content, 32 * 1024, 8 * 1024 * 1024).await;

        let download_dir = tempfile::tempdir().unwrap();
        let session = TorrentSession::new(download_dir.path()).await.unwrap();
        let torrent_id = session
            .add_bytes_with_peers(torrent_bytes, vec![address])
            .await
            .unwrap();
        let video = session.files(torrent_id).unwrap().remove(0);
        assert_eq!(video.length, content.len() as u64);

        let cache = PlaybackCache::new().unwrap();
        let item = Arc::new(Mutex::new(QueuedSession::new(
            "queued-1".to_string(),
            MediaRef::Movie {
                catalog_id: "tt0111161".to_string(),
                title: "x".to_string(),
            },
            "1080p".to_string(),
            false,
            Arc::new(session),
            torrent_id,
            video.clone(),
            cache,
        )));

        tokio::time::timeout(Duration::from_secs(20), buffer_to_threshold(item.clone()))
            .await
            .expect("buffering must finish well within the test timeout")
            .unwrap();

        let guard = item.lock().await;
        assert_eq!(guard.state(), QueueItemState::Ready);
        let target = buffer_target_bytes(video.length);
        let buffered = guard.buffered_bytes.load(Ordering::Relaxed);
        assert!(
            buffered >= target,
            "must buffer at least the target: buffered {buffered}, target {target}"
        );
        // It must actually stop, not fall through to the whole file: allow
        // one extra piece of slack over the exact target.
        assert!(
            buffered < target + 64 * 1024,
            "must stop near the target, not continue past it: buffered {buffered}, target {target}"
        );
        assert!(
            buffered < video.length,
            "a queued item must never buffer the whole file: buffered {buffered}, file length {}",
            video.length
        );
    }

    // Locks in the exact librqbit behavior `run_promote_queued_pipeline`
    // depends on: unpause() is only valid on a torrent that was actually
    // paused. Promoting a still-buffering (never paused) queued item must
    // skip the unpause call entirely, or this error is what the user would
    // see instead of their video playing.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn unpausing_a_torrent_that_was_never_paused_is_rejected() {
        let content = vec![3u8; 256 * 1024];
        let (_seed, address, torrent_bytes) = seed_locally(&content, 32 * 1024, 1024 * 1024).await;

        let download_dir = tempfile::tempdir().unwrap();
        let session = TorrentSession::new(download_dir.path()).await.unwrap();
        let torrent_id = session
            .add_bytes_with_peers(torrent_bytes, vec![address])
            .await
            .unwrap();

        let error = session.unpause(torrent_id).await.unwrap_err();
        assert!(
            format!("{error:#}").contains("already live"),
            "expected librqbit to reject unpausing a live torrent, got: {error:#}"
        );

        // The same session, once actually paused, unpauses cleanly. This is
        // the Ready path `run_promote_queued_pipeline` takes.
        session.pause(torrent_id).await.unwrap();
        session.unpause(torrent_id).await.unwrap();
    }
}
