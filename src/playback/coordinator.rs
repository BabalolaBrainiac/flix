use super::preparation::{first_subtitle, prepare_with_subtitles};
use crate::config::{Config, PlaybackCache};
use crate::episode_queue::EpisodeQueue;
use crate::media;
use crate::playback::types::{MediaRef, OperationAccepted, PlaybackSnapshot, SafeError};
use crate::player::{self, ManagedPlayer, PlaybackOptions};
use crate::session::{Source, TorrentFile, TorrentId, TorrentSession};
use crate::stream_server::{self, ServerHandle};
use crate::stremio::{
    self, automatic_playback_candidates, stream_quality_label, CatalogItem, CatalogKind, Episode,
    SubtitleContext, TorrentStream,
};
use crate::subtitles;
use anyhow::{anyhow, Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{watch, Mutex, RwLock};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

const SOURCE_ID_TTL: Duration = Duration::from_secs(15 * 60);
const MAX_SOURCE_MAP_ENTRIES: usize = 512;
const SUBTITLE_TIMEOUT: Duration = Duration::from_secs(25);
const SUBTITLE_GRACE: Duration = Duration::from_secs(2);

#[derive(Clone, Debug)]
pub struct ResolvedSource {
    pub is_anime: bool,
    pub magnet: String,
    pub file_index: Option<usize>,
    pub quality: String,
    pub alternatives: Vec<AlternativeSource>,
}

#[derive(Clone, Debug)]
pub struct AlternativeSource {
    pub magnet: String,
    pub file_index: Option<usize>,
    pub quality: String,
}

struct SourceMapEntry {
    source: ResolvedSource,
    inserted_at: Instant,
}

pub struct ActiveSession {
    pub is_anime: bool,
    pub media: MediaRef,
    pub server: ServerHandle,
    pub session: Arc<TorrentSession>,
    pub torrent_id: TorrentId,
    pub videos: Vec<TorrentFile>,
    pub position: usize,
    pub player: Option<ManagedPlayer>,
    pub player_name: Option<String>,
    pub episode_queue: Option<EpisodeQueue>,
    pub quality: String,
    pub cache: PlaybackCache,
}

impl Drop for ActiveSession {
    fn drop(&mut self) {
        self.session.cancel();
    }
}

impl ActiveSession {
    pub fn current_video(&self) -> &TorrentFile {
        &self.videos[self.position]
    }

    pub fn has_next_video(&self) -> bool {
        self.position + 1 < self.videos.len()
    }
}

enum SubtitlePreparation {
    Ready(Vec<PathBuf>),
    Unavailable(String),
}

pub struct PlaybackCoordinator {
    debug_log: crate::desktop::debug_report::DebugLog,
    config: Config,
    snapshot_tx: watch::Sender<PlaybackSnapshot>,
    snapshot_rx: watch::Receiver<PlaybackSnapshot>,
    active_session: Arc<Mutex<Option<Arc<Mutex<ActiveSession>>>>>,
    cancellation_token: Arc<Mutex<CancellationToken>>,
    operation_lock: Arc<Mutex<()>>,
    control_lock: Mutex<()>,
    source_map: Arc<RwLock<HashMap<String, SourceMapEntry>>>,
}

impl PlaybackCoordinator {
    pub fn new(config: Config) -> Self {
        let (snapshot_tx, snapshot_rx) = watch::channel(PlaybackSnapshot::Idle);
        Self {
            debug_log: crate::desktop::debug_report::DebugLog::new(&config.data_dir),
            config,
            snapshot_tx,
            snapshot_rx,
            active_session: Arc::new(Mutex::new(None)),
            cancellation_token: Arc::new(Mutex::new(CancellationToken::new())),
            operation_lock: Arc::new(Mutex::new(())),
            control_lock: Mutex::new(()),
            source_map: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn snapshot(&self) -> PlaybackSnapshot {
        self.snapshot_rx.borrow().clone()
    }

    pub fn debug_report(&self) -> crate::desktop::debug_report::DebugReport {
        self.debug_log.export()
    }

    pub fn subscribe(&self) -> watch::Receiver<PlaybackSnapshot> {
        self.snapshot_rx.clone()
    }

    pub async fn register_stream_candidates(
        &self,
        streams: &[TorrentStream],
    ) -> Result<Vec<(String, TorrentStream)>> {
        self.register_stream_candidates_for(streams, false).await
    }

    pub async fn register_stream_candidates_for(
        &self,
        streams: &[TorrentStream],
        is_anime: bool,
    ) -> Result<Vec<(String, TorrentStream)>> {
        let candidates = automatic_playback_candidates(streams);
        let mut results = Vec::new();
        let mut map = self.source_map.write().await;
        prune_source_map(&mut map);

        for stream in streams.iter().take(MAX_SOURCE_MAP_ENTRIES) {
            let source_id = Uuid::new_v4().to_string();
            let magnet = stremio::build_magnet(&stream.info_hash, stream.file_name.as_deref())?;
            let quality = stream_quality_label(stream).to_string();

            let alternatives: Vec<AlternativeSource> = candidates
                .iter()
                .filter(|candidate| candidate.info_hash != stream.info_hash)
                .filter_map(|candidate| {
                    let alt_magnet =
                        stremio::build_magnet(&candidate.info_hash, candidate.file_name.as_deref())
                            .ok()?;
                    Some(AlternativeSource {
                        magnet: alt_magnet,
                        file_index: candidate.file_index,
                        quality: stream_quality_label(candidate).to_string(),
                    })
                })
                .collect();

            map.insert(
                source_id.clone(),
                SourceMapEntry {
                    source: ResolvedSource {
                        is_anime,
                        magnet,
                        file_index: stream.file_index,
                        quality,
                        alternatives,
                    },
                    inserted_at: Instant::now(),
                },
            );
            results.push((source_id, stream.clone()));
        }
        prune_source_map(&mut map);

        Ok(results)
    }

    pub async fn get_resolved_source(&self, source_id: &str) -> Option<ResolvedSource> {
        let mut map = self.source_map.write().await;
        prune_source_map(&mut map);
        map.get(source_id).map(|entry| entry.source.clone())
    }

    pub async fn stop(&self) -> Result<()> {
        let _control = self.control_lock.lock().await;
        self.set_snapshot(PlaybackSnapshot::Stopping);
        {
            let cancel = self.cancellation_token.lock().await;
            cancel.cancel();
        }

        let _operation_guard = self.operation_lock.lock().await;
        self.close_active_playback().await;

        self.set_snapshot(PlaybackSnapshot::Idle);
        Ok(())
    }

    /// Closes the current playback: kills the player process and drops the
    /// session so its torrent and stream server are released. Does not change
    /// the snapshot, because a new playback is about to set its own. This makes
    /// a new play reuse one player instead of opening another.
    async fn close_active_playback(&self) {
        let active = self.active_session.lock().await.take();
        if let Some(active) = active {
            let mut session = active.lock().await;
            session.session.cancel();
            if let Some(mut player) = session.player.take() {
                let _ = player.stop();
                let _ = player.wait();
            }
            session.server.shutdown().await;
            session.session.shutdown().await;
        }
    }

    pub async fn start_playback(
        self: &Arc<Self>,
        media: MediaRef,
        source: ResolvedSource,
        episode_queue_seed: Option<crate::desktop::types::EpisodeQueueSeed>,
    ) -> OperationAccepted {
        let _control = self.control_lock.lock().await;
        let operation_id = Uuid::new_v4().to_string();
        let cancel_token = self.reset_cancellation_token().await;
        let coordinator = Arc::clone(self);
        let operation_lock = Arc::clone(&self.operation_lock);

        tokio::spawn(async move {
            let _operation_guard = operation_lock.lock_owned().await;
            if cancel_token.is_cancelled() {
                return;
            }
            coordinator.close_active_playback().await;
            let result = tokio::select! {
                biased;
                _ = cancel_token.cancelled() => Err(anyhow!("Operation cancelled")),
                result = coordinator.run_playback_pipeline(media, source, episode_queue_seed, cancel_token.clone()) => result,
            };
            coordinator.finish_background_operation(result).await;
            coordinator.monitor_player().await;
        });

        OperationAccepted {
            operation_id,
            status: "accepted".to_string(),
        }
    }

    pub async fn next(self: &Arc<Self>) -> Result<()> {
        let _control = self.control_lock.lock().await;
        let operation_guard = Arc::clone(&self.operation_lock)
            .try_lock_owned()
            .map_err(|_| anyhow!("A playback transition is already in progress"))?;
        let cancel_token = self.reset_cancellation_token().await;
        let coordinator = Arc::clone(self);

        tokio::spawn(async move {
            let _operation_guard = operation_guard;
            let result = tokio::select! {
                biased;
                _ = cancel_token.cancelled() => Err(anyhow!("Operation cancelled")),
                result = coordinator.run_next_pipeline(cancel_token.clone()) => result,
            };
            coordinator.finish_background_operation(result).await;
        });

        Ok(())
    }

    async fn finish_background_operation(&self, result: Result<()>) {
        if let Err(error) = result {
            if is_cancelled_error(&error) {
                self.close_active_playback().await;
                return;
            }
            tracing::error!("Playback pipeline failed: {:#}", error);
            self.close_active_playback().await;
            self.set_snapshot(PlaybackSnapshot::Failed {
                error: safe_error_for_pipeline(&error),
            });
        }
    }

    async fn monitor_player(self: &Arc<Self>) {
        let Some(active) = self
            .active_session
            .lock()
            .await
            .as_ref()
            .map(Arc::downgrade)
        else {
            return;
        };
        let coordinator = Arc::downgrade(self);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(1)).await;
                let (Some(coordinator), Some(active)) = (coordinator.upgrade(), active.upgrade())
                else {
                    return;
                };
                let Ok(_operation) = coordinator.operation_lock.try_lock() else {
                    continue;
                };
                let (exit, player_name) = {
                    let mut session = active.lock().await;
                    let exit = match session.player.as_mut() {
                        Some(player) => player.poll_exit(),
                        None => return,
                    };
                    (exit, session.player_name.clone().unwrap_or_default())
                };
                let snapshot = match exit {
                    Ok(None) => continue,
                    Ok(Some(exit)) => {
                        let success = exit.success;
                        coordinator.debug_log.record_player_exit(&player_name, exit);
                        if success {
                            PlaybackSnapshot::Idle
                        } else {
                            PlaybackSnapshot::Failed {
                                error: SafeError::new("PLAYER_EXITED",
                                    "The player closed with an error. Try another source or export a debug report.", true),
                            }
                        }
                    }
                    Err(_) => PlaybackSnapshot::Failed {
                        error: SafeError::new(
                            "PLAYER_STATUS_FAILED",
                            "Flix could not read the player status. Start playback again.",
                            true,
                        ),
                    },
                };
                coordinator.close_active_playback().await;
                coordinator.set_snapshot(snapshot);
                return;
            }
        });
    }

    async fn run_playback_pipeline(
        &self,
        media: MediaRef,
        source: ResolvedSource,
        episode_queue_seed: Option<crate::desktop::types::EpisodeQueueSeed>,
        cancel_token: CancellationToken,
    ) -> Result<()> {
        if cancel_token.is_cancelled() {
            return Ok(());
        }

        self.set_snapshot(PlaybackSnapshot::LoadingTorrent {
            media: media.clone(),
            quality: source.quality.clone(),
        });

        let started = Instant::now();
        let cache = PlaybackCache::new()?;
        let session = Arc::new(TorrentSession::new(cache.path()).await?);
        let server = stream_server::serve(session.clone()).await?;

        let (torrent_id, videos, position, quality, alternatives) =
            match load_torrent(&session, &source.magnet, source.file_index, &cancel_token).await {
                Ok((id, loaded_videos, loaded_position)) => (
                    id,
                    loaded_videos,
                    loaded_position,
                    source.quality.clone(),
                    source.alternatives.clone(),
                ),
                Err(primary_error) => {
                    if source.alternatives.is_empty() {
                        return Err(primary_error);
                    }
                    load_alternatives(&session, &source.alternatives, &cancel_token).await?
                }
            };

        if cancel_token.is_cancelled() {
            return Ok(());
        }

        let mut episode_queue = episode_queue_seed.and_then(|seed| {
            reconstruct_queue(&seed, videos.get(position).map(|video| video.name.as_str()))
        });
        if let Some(video) = videos.get(position) {
            sync_queue_with_video(&mut episode_queue, video);
        }
        let is_anime = source.is_anime
            || episode_queue
                .as_ref()
                .is_some_and(|queue| queue.subtitle_context(queue.current()).is_some())
            || media.is_anime();
        let mut active = ActiveSession {
            is_anime,
            media,
            cache,
            session,
            server,
            torrent_id,
            videos,
            position,
            player: None,
            player_name: None,
            episode_queue,
            quality,
        };

        self.launch_active_session(&mut active, alternatives, cancel_token)
            .await?;
        tracing::info!(
            elapsed_ms = started.elapsed().as_millis() as u64,
            "playback preparation completed"
        );
        *self.active_session.lock().await = Some(Arc::new(Mutex::new(active)));
        Ok(())
    }

    async fn run_next_pipeline(&self, cancel_token: CancellationToken) -> Result<()> {
        let active = self
            .active_session
            .lock()
            .await
            .as_ref()
            .cloned()
            .context("There is no active playback session")?;
        let mut active = active.lock().await;

        if active.has_next_video() {
            stop_active_player(&mut active);
            active.position += 1;
            if let Some(queue) = active.episode_queue.as_mut() {
                let _ = queue.advance();
            }
            let next_video = active.current_video().clone();
            sync_queue_with_video(&mut active.episode_queue, &next_video);
            active.media = updated_media_for_session(&active.media, active.episode_queue.as_ref());
            active
                .session
                .select_files(active.torrent_id, &[next_video.index])
                .await?;
            return self
                .launch_active_session(&mut active, Vec::new(), cancel_token)
                .await;
        }

        let next_episode = active
            .episode_queue
            .as_ref()
            .and_then(EpisodeQueue::next)
            .cloned()
            .context("There is no next episode")?;
        let client = stremio::StremioClient::from_env()?;
        let streams = client
            .streams_for("series", &next_episode.stream_id, active.is_anime)
            .await?;
        let candidates = automatic_playback_candidates(&streams);
        let selected = candidates
            .first()
            .copied()
            .context("No torrent stream was found for the next episode")?;
        let magnet = stremio::build_magnet(&selected.info_hash, selected.file_name.as_deref())?;
        let alternatives = candidates
            .iter()
            .skip(1)
            .filter_map(|candidate| {
                let alt_magnet =
                    stremio::build_magnet(&candidate.info_hash, candidate.file_name.as_deref())
                        .ok()?;
                Some(AlternativeSource {
                    magnet: alt_magnet,
                    file_index: candidate.file_index,
                    quality: stream_quality_label(candidate).to_string(),
                })
            })
            .collect::<Vec<_>>();

        let previous_torrent_id = active.torrent_id;
        let (torrent_id, videos, position, quality, remaining_alternatives) = match load_torrent(
            &active.session,
            &magnet,
            selected.file_index,
            &cancel_token,
        )
        .await
        {
            Ok((id, loaded_videos, loaded_position)) => (
                id,
                loaded_videos,
                loaded_position,
                stream_quality_label(selected).to_string(),
                alternatives.clone(),
            ),
            Err(primary_error) => {
                if alternatives.is_empty() {
                    return Err(primary_error);
                }
                load_alternatives(&active.session, &alternatives, &cancel_token).await?
            }
        };

        stop_active_player(&mut active);

        if let Some(queue) = active.episode_queue.as_mut() {
            let _ = queue.advance();
        }
        active.media = media_from_episode(&active.media, &next_episode);
        active.torrent_id = torrent_id;
        active.videos = videos;
        active.position = position;
        active.quality = quality;
        let next_video = active.current_video().clone();
        sync_queue_with_video(&mut active.episode_queue, &next_video);

        self.launch_active_session(&mut active, remaining_alternatives, cancel_token)
            .await?;
        if let Err(error) = active.session.remove(previous_torrent_id, true).await {
            tracing::warn!("Could not remove the previous torrent: {error:#}");
        }
        Ok(())
    }

    async fn launch_active_session(
        &self,
        active: &mut ActiveSession,
        mut alternatives: Vec<AlternativeSource>,
        cancel_token: CancellationToken,
    ) -> Result<()> {
        if cancel_token.is_cancelled() {
            return Ok(());
        }

        let selected_player = player::detect()
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("No supported media player is available"))?;
        let mut prepared = self.prepare_active_video(active, &cancel_token).await;

        while prepared
            .as_ref()
            .map_or(true, |prepared| !prepared.warmup_ready)
        {
            let Some(alternative) = alternatives.first().cloned() else {
                return Err(prepared.err().unwrap_or_else(|| {
                    anyhow!("No listed source buffered in time. Try another source.")
                }));
            };
            alternatives.remove(0);
            tracing::warn!(
                "The selected torrent did not warm the stream. Trying a {} source.",
                alternative.quality
            );
            let loaded = load_torrent(
                &active.session,
                &alternative.magnet,
                alternative.file_index,
                &cancel_token,
            )
            .await;
            let (torrent_id, videos, position) = match loaded {
                Ok(loaded) => loaded,
                Err(error) => {
                    tracing::warn!("Alternative torrent failed: {error:#}");
                    continue;
                }
            };
            let previous_torrent_id = active.torrent_id;
            active.torrent_id = torrent_id;
            active.videos = videos;
            active.position = position;
            active.quality = alternative.quality;
            let current_video = active.current_video().clone();
            sync_queue_with_video(&mut active.episode_queue, &current_video);
            if let Err(error) = active.session.remove(previous_torrent_id, true).await {
                tracing::warn!("Could not remove an unresponsive torrent: {error:#}");
            }
            prepared = self.prepare_active_video(active, &cancel_token).await;
        }

        if cancel_token.is_cancelled() {
            return Ok(());
        }

        let prepared = prepared?;
        let media = active.media.clone();
        let quality = active.quality.clone();
        let player_name = selected_player.kind_label().to_string();
        self.set_snapshot(PlaybackSnapshot::LaunchingPlayer {
            media: media.clone(),
            quality: quality.clone(),
            player_name: player_name.clone(),
        });

        let options = PlaybackOptions::new(prepared.title, prepared.file_length)
            .with_selected_tracks(prepared.selected_tracks.clone())
            .with_subtitles(prepared.subtitle_files.clone())
            .with_show_output(std::env::var_os("FLIX_PLAYER_LOGS").is_some());
        let mut managed_player = player::launch(&selected_player, &prepared.url, &options)?;
        if let Err(error) = managed_player.wait_for_startup().await {
            if let Some(failure) = error.downcast_ref::<player::PlayerStartupError>() {
                self.debug_log
                    .record_player_exit(&player_name, failure.0.clone());
            }
            return Err(error);
        }
        let has_next = active.has_next_video()
            || active
                .episode_queue
                .as_ref()
                .is_some_and(EpisodeQueue::has_next);

        active.player = Some(managed_player);
        active.player_name = Some(player_name.clone());

        self.set_snapshot(PlaybackSnapshot::Playing {
            media,
            title: active.media.display_title(),
            quality,
            file_length: prepared.file_length,
            player_name,
            subtitle_ready: !prepared.subtitle_files.is_empty()
                || prepared
                    .selected_tracks
                    .as_ref()
                    .is_some_and(|tracks| tracks.subtitle.is_some()),
            has_next,
        });

        Ok(())
    }

    async fn prepare_active_video(
        &self,
        active: &ActiveSession,
        cancel_token: &CancellationToken,
    ) -> Result<PreparedPlayback> {
        let video = active.current_video().clone();
        let subtitle_context = current_subtitle_context(active.episode_queue.as_ref());
        self.prepare_video(PrepareVideoRequest {
            is_anime: active.is_anime,
            media: &active.media,
            quality: &active.quality,
            session: &active.session,
            torrent_id: active.torrent_id,
            video: &video,
            server: &active.server,
            subtitle_context: subtitle_context.as_ref(),
            cancel_token,
        })
        .await
    }

    async fn prepare_video(&self, request: PrepareVideoRequest<'_>) -> Result<PreparedPlayback> {
        if request.cancel_token.is_cancelled() {
            return Err(anyhow!("Operation cancelled"));
        }

        self.set_snapshot(PlaybackSnapshot::Buffering {
            media: request.media.clone(),
            quality: request.quality.to_string(),
            file_name: request.video.name.clone(),
        });

        let info_hash = request.session.info_hash(request.torrent_id)?;
        let warmup = request
            .session
            .warm_stream(request.torrent_id, request.video.index);
        let subtitle_work = async {
            if request.is_anime {
                let (reader, _) = request
                    .session
                    .open_stream(request.torrent_id, request.video.index)?;
                if matches!(
                    tokio::time::timeout(
                        Duration::from_secs(3),
                        super::anime::inspect(reader, false)
                    )
                    .await,
                    Ok(Ok(_))
                ) {
                    return Ok::<_, anyhow::Error>(SubtitlePreparation::Ready(Vec::new()));
                }
            }
            let cached = subtitles::cached_english_subtitles(
                &self.config.data_dir,
                &info_hash,
                request.video.index,
            )
            .await?;
            if !cached.is_empty() {
                return Ok::<_, anyhow::Error>(SubtitlePreparation::Ready(cached));
            }

            self.set_snapshot(PlaybackSnapshot::FindingSubtitle {
                media: request.media.clone(),
                quality: request.quality.to_string(),
            });

            let anime_lookup = async {
                let Some(context) = request.subtitle_context else {
                    return Ok(None);
                };
                crate::stremio::find_anime_subtitle(
                    &self.config.data_dir,
                    &info_hash,
                    request.video.index,
                    &request.video.name,
                    context,
                )
                .await
            };
            let gateway_lookup = subtitles::resolve_gateway_english_subtitle(
                &self.config.data_dir,
                request.media,
                &info_hash,
                request.video.index,
            );
            match first_subtitle(anime_lookup, gateway_lookup).await? {
                Some(path) => Ok(SubtitlePreparation::Ready(vec![path])),
                None => Ok(SubtitlePreparation::Unavailable(
                    "No prepared English subtitle was found.".to_string(),
                )),
            }
        };

        let subtitles = async {
            match tokio::time::timeout(SUBTITLE_TIMEOUT, subtitle_work).await {
                Ok(Ok(SubtitlePreparation::Ready(paths))) => paths,
                Ok(Ok(SubtitlePreparation::Unavailable(message))) => {
                    tracing::info!("{}", message);
                    Vec::new()
                }
                Ok(Err(error)) => {
                    tracing::warn!("Subtitle preparation failed: {error:#}");
                    Vec::new()
                }
                Err(_) => Vec::new(),
            }
        };
        let (warmup_ready, subtitle_paths) =
            match prepare_with_subtitles(warmup, subtitles, request.cancel_token, SUBTITLE_GRACE)
                .await
            {
                Ok((report, paths)) => {
                    tracing::info!("Stream buffer is ready: {report}");
                    (true, paths)
                }
                Err(error) if is_cancelled_error(&error) => return Err(error),
                Err(error) => {
                    tracing::warn!("Stream buffering failed: {error:#}");
                    (false, Vec::new())
                }
            };

        let selected_tracks = if request.is_anime && warmup_ready {
            let (reader, _) = request
                .session
                .open_stream(request.torrent_id, request.video.index)?;
            Some(
                tokio::time::timeout(
                    Duration::from_secs(3),
                    super::anime::inspect(reader, !subtitle_paths.is_empty()),
                )
                .await
                .context("Anime language check timed out")??,
            )
        } else {
            None
        };
        Ok(PreparedPlayback {
            selected_tracks,
            title: request.video.name.clone(),
            url: stream_server::stream_url(request.server, request.torrent_id, request.video.index),
            subtitle_files: subtitle_paths
                .iter()
                .map(|path| path.to_string_lossy().into_owned())
                .collect(),
            file_length: request.video.length,
            warmup_ready,
        })
    }

    async fn reset_cancellation_token(&self) -> CancellationToken {
        let mut cancel_guard = self.cancellation_token.lock().await;
        cancel_guard.cancel();
        let new_token = CancellationToken::new();
        *cancel_guard = new_token.clone();
        new_token
    }

    fn set_snapshot(&self, snapshot: PlaybackSnapshot) {
        self.debug_log.record(&snapshot);
        let _ = self.snapshot_tx.send(snapshot);
    }
}

impl player::Player {
    pub fn kind_label(&self) -> &'static str {
        self.kind.label()
    }
}

struct PreparedPlayback {
    selected_tracks: Option<super::anime::SelectedTracks>,
    title: String,
    url: String,
    subtitle_files: Vec<String>,
    file_length: u64,
    warmup_ready: bool,
}

struct PrepareVideoRequest<'a> {
    is_anime: bool,
    media: &'a MediaRef,
    quality: &'a str,
    session: &'a TorrentSession,
    torrent_id: TorrentId,
    video: &'a TorrentFile,
    server: &'a ServerHandle,
    subtitle_context: Option<&'a SubtitleContext>,
    cancel_token: &'a CancellationToken,
}

async fn load_torrent(
    session: &TorrentSession,
    torrent: &str,
    preferred_file_index: Option<usize>,
    cancel_token: &CancellationToken,
) -> Result<(TorrentId, Vec<TorrentFile>, usize)> {
    if cancel_token.is_cancelled() {
        return Err(anyhow!("Operation cancelled"));
    }
    let source = Source::parse(torrent)?;
    // Select the file at add time when the catalog gives its index. A second
    // file selection on a live torrent can deadlock inside librqbit, and it
    // drops the peers that connect while nothing is selected.
    let (id, preselected) = match preferred_file_index {
        Some(file_index) => session.add_with_file(source, file_index).await?,
        None => (session.add(source).await?, false),
    };
    let result = async {
        if cancel_token.is_cancelled() {
            return Err(anyhow!("Operation cancelled"));
        }
        let files = session.files(id)?;
        let videos: Vec<_> = media::ordered_videos(&files).into_iter().cloned().collect();
        if videos.is_empty() {
            return Err(anyhow!("Torrent contains no supported video file"));
        }
        let position = preferred_file_index
            .and_then(|file_index| videos.iter().position(|video| video.index == file_index))
            .unwrap_or(0);
        let target = videos[position].index;
        if crate::session::needs_file_selection(preferred_file_index, preselected, target) {
            session.select_files(id, &[target]).await?;
        }
        Ok((id, videos, position))
    }
    .await;

    if result.is_err() {
        if let Err(error) = session.remove(id, true).await {
            tracing::warn!("Could not remove a failed torrent: {error:#}");
        }
    }
    result
}

async fn load_alternatives(
    session: &TorrentSession,
    alternatives: &[AlternativeSource],
    cancel_token: &CancellationToken,
) -> Result<(
    TorrentId,
    Vec<TorrentFile>,
    usize,
    String,
    Vec<AlternativeSource>,
)> {
    let mut last_error = None;
    for (index, source) in alternatives.iter().enumerate() {
        if cancel_token.is_cancelled() {
            return Err(anyhow!("Operation cancelled"));
        }
        match load_torrent(session, &source.magnet, source.file_index, cancel_token).await {
            Ok((id, videos, position)) => {
                return Ok((
                    id,
                    videos,
                    position,
                    source.quality.clone(),
                    alternatives[index + 1..].to_vec(),
                ))
            }
            Err(error) => last_error = Some(error),
        }
    }

    match last_error {
        Some(error) => Err(error),
        None => Err(anyhow!("No automatic playback alternative was available")),
    }
}

fn current_subtitle_context(queue: Option<&EpisodeQueue>) -> Option<SubtitleContext> {
    queue.and_then(|episode_queue| episode_queue.subtitle_context(episode_queue.current()))
}

fn stop_active_player(active: &mut ActiveSession) {
    if let Some(mut player) = active.player.take() {
        let _ = player.stop();
        let _ = player.wait();
    }
}

fn sync_queue_with_video(queue: &mut Option<EpisodeQueue>, video: &TorrentFile) {
    let parsed = crate::meta::parse::parse(&video.name);
    let (Some(season), Some(episode)) = (parsed.season, parsed.episode) else {
        return;
    };
    if let Some(queue) = queue {
        queue.select(season, episode);
    }
}

fn media_from_episode(current_media: &MediaRef, episode: &Episode) -> MediaRef {
    let catalog_id = match current_media {
        MediaRef::Movie { catalog_id, .. } | MediaRef::Episode { catalog_id, .. } => {
            catalog_id.clone()
        }
    };
    MediaRef::Episode {
        catalog_id,
        stream_id: episode.stream_id.clone(),
        imdb_id: episode.imdb_id.clone(),
        season: episode.season,
        episode: episode.episode,
        title: episode.title.clone(),
    }
}

fn updated_media_for_session(current_media: &MediaRef, queue: Option<&EpisodeQueue>) -> MediaRef {
    match queue {
        Some(queue) => media_from_episode(current_media, queue.current()),
        None => current_media.clone(),
    }
}

fn reconstruct_queue(
    seed: &crate::desktop::types::EpisodeQueueSeed,
    current_video_name: Option<&str>,
) -> Option<EpisodeQueue> {
    let item = CatalogItem {
        id: seed.catalog_item.id.clone(),
        media_type: seed.catalog_item.media_type.clone(),
        name: seed.catalog_item.name.clone(),
        release_info: seed.catalog_item.release_info.clone(),
        poster: seed.catalog_item.poster.clone(),
        genres: None,
        imdb_rating: None,
        description: None,
        kind: if seed.catalog_item.is_anime {
            CatalogKind::AnimeKitsu
        } else {
            CatalogKind::Cinemeta
        },
    };
    let episodes: Vec<Episode> = seed
        .episodes
        .iter()
        .map(|episode| Episode {
            id: episode.id.clone(),
            stream_id: if episode.stream_id.is_empty() {
                episode.id.clone()
            } else {
                episode.stream_id.clone()
            },
            imdb_id: episode.imdb_id.clone(),
            title: episode.title.clone(),
            season: episode.season,
            episode: episode.episode,
        })
        .collect();
    let mut queue = EpisodeQueue::new(item, episodes, &seed.current_episode_id).ok()?;
    if let Some(name) = current_video_name {
        let parsed = crate::meta::parse::parse(name);
        if let (Some(season), Some(episode)) = (parsed.season, parsed.episode) {
            queue.select(season, episode);
        }
    }
    Some(queue)
}

fn prune_source_map(map: &mut HashMap<String, SourceMapEntry>) {
    let now = Instant::now();
    map.retain(|_, entry| now.duration_since(entry.inserted_at) <= SOURCE_ID_TTL);

    if map.len() <= MAX_SOURCE_MAP_ENTRIES {
        return;
    }

    let mut entries: Vec<_> = map
        .iter()
        .map(|(source_id, entry)| (source_id.clone(), entry.inserted_at))
        .collect();
    entries.sort_unstable_by_key(|(_, inserted_at)| *inserted_at);

    for (source_id, _) in entries.into_iter().take(map.len() - MAX_SOURCE_MAP_ENTRIES) {
        map.remove(&source_id);
    }
}

fn is_cancelled_error(error: &anyhow::Error) -> bool {
    error.to_string().contains("Operation cancelled")
}

fn safe_error_for_pipeline(error: &anyhow::Error) -> SafeError {
    if error.is::<player::PlayerStartupError>() {
        return SafeError::new("PLAYER_START_FAILED",
            "The player closed before startup completed. Export a debug report to check the failure.", true);
    }
    let message = error.to_string().to_ascii_lowercase();
    if message.contains("too many open files") || message.contains("os error 24") {
        return SafeError::new(
            "RESOURCE_EXHAUSTED",
            "Flix reached the open file limit. Stop the active playback and retry.",
            true,
        );
    }
    if message.contains("anime") {
        return SafeError::new("ANIME_LANGUAGE_UNAVAILABLE",
            "This source has no verified original audio and English subtitles. Select another MKV source.", true);
    }
    if message.contains("failed to spawn player") {
        return SafeError::new(
            "PLAYER_START_FAILED",
            "Flix could not open the player. Check its installation and try again.",
            true,
        );
    }
    if message.contains("player") {
        return SafeError::player_unavailable("No supported media player is available.");
    }
    if message.contains("subtitle") {
        return SafeError::new(
            "SUBTITLE_PREPARATION_FAILED",
            "English subtitle preparation failed.",
            true,
        );
    }
    // Torrent load failures are the common reason a source will not play. Give
    // the reason and a next step instead of one generic message, so the user
    // knows to pick another source rather than assume the app is broken.
    if message.contains("metadata") {
        return SafeError::playback_failed(
            "No peers shared this torrent in time. The source may be dead. Try another source.",
        );
    }
    if message.contains("timed out") || message.contains("timeout") {
        return SafeError::playback_failed(
            "The source did not respond in time. Check your connection or try another source.",
        );
    }
    if message.contains("no peers") || message.contains("no seeds") || message.contains("seeders") {
        return SafeError::playback_failed(
            "No seeders are available for this source. Try another source.",
        );
    }
    if message.contains("video") || message.contains("no playable") || message.contains("file") {
        return SafeError::playback_failed(
            "This torrent has no playable video file. Try another source.",
        );
    }
    SafeError::playback_failed("Playback could not start. Try another source.")
}

#[cfg(test)]
mod tests {
    use super::{
        prune_source_map, safe_error_for_pipeline, ResolvedSource, SourceMapEntry,
        MAX_SOURCE_MAP_ENTRIES,
    };
    use crate::config::Config;
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    #[test]
    fn reports_open_file_exhaustion_as_a_resource_error() {
        let error = anyhow::anyhow!("Failed to spawn player: Too many open files (os error 24)");

        let safe_error = safe_error_for_pipeline(&error);

        assert_eq!(safe_error.code, "RESOURCE_EXHAUSTED");
        assert!(safe_error.retryable);
    }

    #[test]
    fn distinguishes_player_startup_failure_from_a_missing_player() {
        let error = crate::player::PlayerStartupError(crate::player::PlayerExit {
            success: false,
            exit_code: Some(1),
            signal: None,
            runtime_ms: 50,
        })
        .into();
        assert_eq!(safe_error_for_pipeline(&error).code, "PLAYER_START_FAILED");
    }

    #[tokio::test]
    async fn rejects_overlapping_episode_transition() {
        let directory = tempfile::tempdir().unwrap();
        let coordinator = Arc::new(super::PlaybackCoordinator::new(Config {
            data_dir: directory.path().join("data"),
            download_dir: directory.path().join("downloads"),
        }));
        let operation_guard = coordinator.operation_lock.clone().try_lock_owned().unwrap();

        let error = coordinator.next().await.unwrap_err();

        assert!(error
            .to_string()
            .contains("A playback transition is already in progress"));
        drop(operation_guard);
    }

    #[tokio::test]
    async fn stop_waits_for_the_active_transition() {
        let directory = tempfile::tempdir().unwrap();
        let coordinator = Arc::new(super::PlaybackCoordinator::new(Config {
            data_dir: directory.path().join("data"),
            download_dir: directory.path().join("downloads"),
        }));
        let operation_guard = coordinator.operation_lock.clone().try_lock_owned().unwrap();
        let stop_task = tokio::spawn({
            let coordinator = Arc::clone(&coordinator);
            async move { coordinator.stop().await }
        });

        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        assert!(!stop_task.is_finished());

        drop(operation_guard);
        stop_task.await.unwrap().unwrap();
    }

    #[test]
    fn limits_source_map_to_recent_entries() {
        let mut map = HashMap::new();
        let now = Instant::now();
        for index in 0..=MAX_SOURCE_MAP_ENTRIES {
            map.insert(
                index.to_string(),
                SourceMapEntry {
                    source: ResolvedSource {
                        is_anime: false,
                        magnet: format!("magnet-{index}"),
                        file_index: None,
                        quality: "1080p".to_string(),
                        alternatives: Vec::new(),
                    },
                    inserted_at: now
                        - Duration::from_millis((MAX_SOURCE_MAP_ENTRIES - index) as u64),
                },
            );
        }

        prune_source_map(&mut map);

        assert_eq!(map.len(), MAX_SOURCE_MAP_ENTRIES);
        assert!(!map.contains_key("0"));
        assert!(map.contains_key(&MAX_SOURCE_MAP_ENTRIES.to_string()));
    }
}
