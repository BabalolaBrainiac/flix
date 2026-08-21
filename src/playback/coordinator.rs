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
const SUBTITLE_TIMEOUT: Duration = Duration::from_secs(25);

#[derive(Clone, Debug)]
pub struct ResolvedSource {
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
    pub media: MediaRef,
    pub cache: PlaybackCache,
    pub session: Arc<TorrentSession>,
    pub server: ServerHandle,
    pub torrent_id: TorrentId,
    pub videos: Vec<TorrentFile>,
    pub position: usize,
    pub player: Option<ManagedPlayer>,
    pub player_name: Option<String>,
    pub episode_queue: Option<EpisodeQueue>,
    pub quality: String,
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
    config: Config,
    snapshot_tx: watch::Sender<PlaybackSnapshot>,
    snapshot_rx: watch::Receiver<PlaybackSnapshot>,
    active_session: Arc<Mutex<Option<ActiveSession>>>,
    cancellation_token: Arc<Mutex<CancellationToken>>,
    source_map: Arc<RwLock<HashMap<String, SourceMapEntry>>>,
}

impl PlaybackCoordinator {
    pub fn new(config: Config) -> Self {
        let (snapshot_tx, snapshot_rx) = watch::channel(PlaybackSnapshot::Idle);
        Self {
            config,
            snapshot_tx,
            snapshot_rx,
            active_session: Arc::new(Mutex::new(None)),
            cancellation_token: Arc::new(Mutex::new(CancellationToken::new())),
            source_map: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn snapshot(&self) -> PlaybackSnapshot {
        self.snapshot_rx.borrow().clone()
    }

    pub fn subscribe(&self) -> watch::Receiver<PlaybackSnapshot> {
        self.snapshot_rx.clone()
    }

    pub async fn register_stream_candidates(
        &self,
        streams: &[TorrentStream],
    ) -> Result<Vec<(String, TorrentStream)>> {
        let candidates = automatic_playback_candidates(streams);
        let mut results = Vec::new();
        let mut map = self.source_map.write().await;
        prune_source_map(&mut map);

        for stream in streams {
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

        Ok(results)
    }

    pub async fn get_resolved_source(&self, source_id: &str) -> Option<ResolvedSource> {
        let mut map = self.source_map.write().await;
        prune_source_map(&mut map);
        map.get(source_id).map(|entry| entry.source.clone())
    }

    pub async fn stop(&self) -> Result<()> {
        self.set_snapshot(PlaybackSnapshot::Stopping);
        {
            let cancel = self.cancellation_token.lock().await;
            cancel.cancel();
        }

        let mut active = self.active_session.lock().await;
        if let Some(mut session) = active.take() {
            if let Some(mut player) = session.player.take() {
                let _ = player.stop();
                let _ = player.wait();
            }
        }

        self.set_snapshot(PlaybackSnapshot::Idle);
        Ok(())
    }

    /// Closes the current playback: kills the player process and drops the
    /// session so its torrent and stream server are released. Does not change
    /// the snapshot, because a new playback is about to set its own. This makes
    /// a new play reuse one player instead of opening another.
    async fn close_active_playback(&self) {
        let mut active = self.active_session.lock().await;
        if let Some(mut session) = active.take() {
            if let Some(mut player) = session.player.take() {
                let _ = player.stop();
                let _ = player.wait();
            }
        }
    }

    pub async fn start_playback(
        self: &Arc<Self>,
        media: MediaRef,
        source: ResolvedSource,
        episode_queue_seed: Option<crate::desktop::types::EpisodeQueueSeed>,
    ) -> OperationAccepted {
        let operation_id = Uuid::new_v4().to_string();
        let cancel_token = self.reset_cancellation_token().await;
        // Close the player from the previous playback before starting a new one.
        // Without this a new play opens a second player process and leaves the
        // old one running. The user wants a single player that the next play
        // reuses.
        self.close_active_playback().await;
        let coordinator = Arc::clone(self);

        tokio::spawn(async move {
            let result = coordinator
                .run_playback_pipeline(media, source, episode_queue_seed, cancel_token)
                .await;
            coordinator.finish_background_operation(result).await;
        });

        OperationAccepted {
            operation_id,
            status: "accepted".to_string(),
        }
    }

    pub async fn next(self: &Arc<Self>) -> Result<()> {
        let cancel_token = self.reset_cancellation_token().await;
        let coordinator = Arc::clone(self);

        tokio::spawn(async move {
            let result = coordinator.run_next_pipeline(cancel_token).await;
            coordinator.finish_background_operation(result).await;
        });

        Ok(())
    }

    async fn finish_background_operation(&self, result: Result<()>) {
        if let Err(error) = result {
            if is_cancelled_error(&error) {
                return;
            }
            tracing::error!("Playback pipeline failed: {:#}", error);
            self.set_snapshot(PlaybackSnapshot::Failed {
                error: safe_error_for_pipeline(&error),
            });
        }
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

        let cache = PlaybackCache::new()?;
        let session = Arc::new(TorrentSession::new(cache.path()).await?);
        let server = stream_server::serve(session.clone()).await?;

        let (torrent_id, videos, position, quality) =
            match load_torrent(&session, &source.magnet, source.file_index, &cancel_token).await {
                Ok((id, loaded_videos, loaded_position)) => {
                    (id, loaded_videos, loaded_position, source.quality.clone())
                }
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
        session
            .select_files(torrent_id, &[videos[position].index])
            .await?;

        let active = ActiveSession {
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

        self.launch_active_session(active, cancel_token).await
    }

    async fn run_next_pipeline(&self, cancel_token: CancellationToken) -> Result<()> {
        let mut active = self
            .active_session
            .lock()
            .await
            .take()
            .context("There is no active playback session")?;

        if let Some(mut player) = active.player.take() {
            let _ = player.stop();
            let _ = player.wait();
        }

        if active.has_next_video() {
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
            return self.launch_active_session(active, cancel_token).await;
        }

        let next_episode = active
            .episode_queue
            .as_ref()
            .and_then(EpisodeQueue::next)
            .cloned()
            .context("There is no next episode")?;
        let client = stremio::StremioClient::from_env()?;
        let streams = client.streams("series", &next_episode.stream_id).await?;
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
        let (torrent_id, videos, position, quality) = match load_torrent(
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
            ),
            Err(primary_error) => {
                if alternatives.is_empty() {
                    return Err(primary_error);
                }
                load_alternatives(&active.session, &alternatives, &cancel_token).await?
            }
        };

        active
            .session
            .select_files(torrent_id, &[videos[position].index])
            .await?;
        let _ = active.session.remove(previous_torrent_id, true).await;

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

        self.launch_active_session(active, cancel_token).await
    }

    async fn launch_active_session(
        &self,
        mut active: ActiveSession,
        cancel_token: CancellationToken,
    ) -> Result<()> {
        if cancel_token.is_cancelled() {
            return Ok(());
        }

        let selected_player = player::detect()
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("No supported media player is available"))?;
        let media = active.media.clone();
        let quality = active.quality.clone();
        let video = active.current_video().clone();
        let subtitle_context = current_subtitle_context(active.episode_queue.as_ref());
        let prepared = self
            .prepare_video(PrepareVideoRequest {
                media: &media,
                quality: &quality,
                session: &active.session,
                torrent_id: active.torrent_id,
                video: &video,
                server: &active.server,
                subtitle_context: subtitle_context.as_ref(),
                cancel_token: &cancel_token,
            })
            .await?;

        if cancel_token.is_cancelled() {
            return Ok(());
        }

        let player_name = selected_player.kind_label().to_string();
        self.set_snapshot(PlaybackSnapshot::LaunchingPlayer {
            media: media.clone(),
            quality: quality.clone(),
            player_name: player_name.clone(),
        });

        let options = PlaybackOptions::new(prepared.title, prepared.file_length)
            .with_standard_languages()
            .with_subtitles(prepared.subtitle_files.clone())
            .with_show_output(std::env::var_os("FLIX_PLAYER_LOGS").is_some());
        let managed_player = player::launch(&selected_player, &prepared.url, &options)?;
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
            subtitle_ready: !prepared.subtitle_files.is_empty(),
            has_next,
        });

        let mut active_guard = self.active_session.lock().await;
        *active_guard = Some(active);
        Ok(())
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
            let cached = subtitles::cached_english_subtitles(
                &self.config.data_dir,
                &info_hash,
                request.video.index,
            )
            .await?;
            if !cached.is_empty() {
                return Ok(SubtitlePreparation::Ready(cached));
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
            let (anime_result, gateway_result) = tokio::join!(anime_lookup, gateway_lookup);

            match anime_result {
                Ok(Some(path)) => return Ok(SubtitlePreparation::Ready(vec![path])),
                Ok(None) => {}
                Err(error) => tracing::warn!("Anime subtitle lookup failed: {:#}", error),
            }

            match gateway_result {
                Ok(Some(path)) => Ok(SubtitlePreparation::Ready(vec![path])),
                Ok(None) => Ok(SubtitlePreparation::Unavailable(
                    "No prepared English subtitle was found.".to_string(),
                )),
                Err(error) => Err(error),
            }
        };

        let (warmup_result, subtitle_result) = tokio::join!(
            warmup,
            tokio::time::timeout(SUBTITLE_TIMEOUT, subtitle_work)
        );

        if let Err(error) = warmup_result {
            tracing::warn!("Stream warm-up was incomplete: {:#}", error);
        }

        // Subtitles never block playback. When an English subtitle is not
        // available, or preparation fails or times out, play without
        // subtitles. The player reports the "no subtitles" state, so the
        // user sees a message, but the video still starts.
        let subtitle_paths = match subtitle_result {
            Ok(Ok(SubtitlePreparation::Ready(paths))) => paths,
            Ok(Ok(SubtitlePreparation::Unavailable(message))) => {
                tracing::info!("Playing without subtitles: {}", message);
                Vec::new()
            }
            Ok(Err(error)) => {
                tracing::warn!(
                    "Subtitle preparation failed, playing without subtitles: {:#}",
                    error
                );
                Vec::new()
            }
            Err(_) => {
                tracing::warn!("Subtitle preparation timed out, playing without subtitles");
                Vec::new()
            }
        };

        Ok(PreparedPlayback {
            title: request.video.name.clone(),
            url: stream_server::stream_url(request.server, request.torrent_id, request.video.index),
            subtitle_files: subtitle_paths
                .iter()
                .map(|path| path.to_string_lossy().into_owned())
                .collect(),
            file_length: request.video.length,
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
        let _ = self.snapshot_tx.send(snapshot);
    }
}

impl player::Player {
    pub fn kind_label(&self) -> &'static str {
        match self.kind {
            player::PlayerKind::Mpv => "mpv",
            player::PlayerKind::Vlc => "VLC",
        }
    }
}

struct PreparedPlayback {
    title: String,
    url: String,
    subtitle_files: Vec<String>,
    file_length: u64,
}

struct PrepareVideoRequest<'a> {
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
    let id = session.add(Source::parse(torrent)?).await?;
    let files = session.files(id)?;
    let videos: Vec<_> = media::ordered_videos(&files).into_iter().cloned().collect();
    if videos.is_empty() {
        return Err(anyhow!("Torrent contains no supported video file"));
    }
    session.select_files(id, &[]).await?;
    let position = preferred_file_index
        .and_then(|file_index| videos.iter().position(|video| video.index == file_index))
        .unwrap_or(0);
    Ok((id, videos, position))
}

async fn load_alternatives(
    session: &TorrentSession,
    alternatives: &[AlternativeSource],
    cancel_token: &CancellationToken,
) -> Result<(TorrentId, Vec<TorrentFile>, usize, String)> {
    let mut last_error = None;
    for source in alternatives {
        if cancel_token.is_cancelled() {
            return Err(anyhow!("Operation cancelled"));
        }
        match load_torrent(session, &source.magnet, source.file_index, cancel_token).await {
            Ok((id, videos, position)) => {
                return Ok((id, videos, position, source.quality.clone()))
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
}

fn is_cancelled_error(error: &anyhow::Error) -> bool {
    error.to_string().contains("Operation cancelled")
}

fn safe_error_for_pipeline(error: &anyhow::Error) -> SafeError {
    let message = error.to_string().to_ascii_lowercase();
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
