use crate::config::{Config, PlaybackCache};
use crate::desktop::commands;
use crate::desktop::types::{
    DownloadsCommand, DownloadsResponse, EpisodeQueueSeed, EpisodesCommand, EpisodesResponse,
    NextEpisodeCommand, NextEpisodeResponse, PlayCommand, PlaySource, PlaybackState,
    PlaybackStatusResponse, SearchCommand, SearchResponse, SettingsCommand, SettingsResponse,
    StopPlaybackCommand, StopPlaybackResponse, StreamsCommand, StreamsResponse,
};
use crate::episode_queue::EpisodeQueue;
use crate::media;
use crate::playback;
use crate::player::{self, ManagedPlayer, PlaybackOptions};
use crate::session::{Source, TorrentFile, TorrentId, TorrentSession};
use crate::stream_server::{self, ServerHandle};
use crate::stremio::{
    self, automatic_playback_candidates, stream_quality_label, CatalogItem, CatalogKind, Episode,
    StremioClient, SubtitleContext,
};
use crate::subtitles;
use anyhow::{anyhow, Context, Result};
use std::sync::Arc;
use std::time::Duration;

pub struct DesktopService {
    config: Config,
    stremio_client: Option<StremioClient>,
    active_session: Option<ActivePlayback>,
}

struct ActivePlayback {
    _cache: PlaybackCache,
    session: Arc<TorrentSession>,
    server: ServerHandle,
    torrent_id: TorrentId,
    videos: Vec<TorrentFile>,
    position: usize,
    player: ManagedPlayer,
    player_path: String,
    episode_queue: Option<EpisodeQueue>,
    quality: String,
    title: String,
}

impl ActivePlayback {
    fn current_video(&self) -> &TorrentFile {
        &self.videos[self.position]
    }

    fn has_next_video(&self) -> bool {
        self.position + 1 < self.videos.len()
    }
}

impl DesktopService {
    pub fn new(config: Config) -> Self {
        let stremio_client = StremioClient::from_env().ok();
        Self {
            config,
            stremio_client,
            active_session: None,
        }
    }

    fn get_stremio_client(&self) -> Result<&StremioClient> {
        self.stremio_client
            .as_ref()
            .context("Stremio client is not available")
    }

    pub async fn search(&self, command: &SearchCommand) -> Result<SearchResponse> {
        let client = self.get_stremio_client()?;
        commands::handle_search(client, command).await
    }

    pub async fn episodes(&self, command: &EpisodesCommand) -> Result<EpisodesResponse> {
        let client = self.get_stremio_client()?;
        commands::handle_episodes(client, command).await
    }

    pub async fn streams(&self, command: &StreamsCommand) -> Result<StreamsResponse> {
        let client = self.get_stremio_client()?;
        commands::handle_streams(client, command).await
    }

    pub async fn downloads(&self, command: &DownloadsCommand) -> Result<DownloadsResponse> {
        commands::handle_downloads(&self.config, command).await
    }

    pub fn settings(&self, command: &SettingsCommand) -> Result<SettingsResponse> {
        commands::handle_settings(&self.config, command)
    }

    pub fn activation_status(&self) -> crate::desktop::types::ActivationStatus {
        commands::handle_activation_status()
    }

    pub async fn redeem_invite(
        &self,
        cmd: &crate::desktop::types::RedeemInviteCommand,
    ) -> Result<crate::desktop::types::RedeemInviteResponse> {
        commands::handle_redeem_invite(cmd).await
    }

    pub fn reset_activation(&self) -> Result<()> {
        crate::desktop::credentials::delete_device_token()
    }

    pub fn vlc_guidance(&self) -> crate::desktop::types::VlcGuidance {
        commands::handle_vlc_guidance()
    }

    pub fn diagnostics(&self) -> Result<crate::desktop::types::DiagnosticsReport> {
        commands::handle_diagnostics(&self.config, self.active_session.is_some())
    }

    pub async fn status(&mut self) -> Result<PlaybackStatusResponse> {
        if let Some(active) = &mut self.active_session {
            if active.player.has_exited()? {
                let _ = active.player.wait();
                self.stop_internal().await?;
                return Ok(PlaybackStatusResponse {
                    state: PlaybackState::Stopped,
                });
            }
            let has_next = active.has_next_video()
                || active
                    .episode_queue
                    .as_ref()
                    .is_some_and(EpisodeQueue::has_next);
            let video = active.current_video();
            let url = stream_server::stream_url(&active.server, active.torrent_id, video.index);
            return Ok(PlaybackStatusResponse {
                state: PlaybackState::Playing {
                    title: active.title.clone(),
                    quality: active.quality.clone(),
                    url,
                    subtitle_url: None,
                    file_length: video.length,
                    player_path: active.player_path.clone(),
                    has_next,
                },
            });
        }
        Ok(PlaybackStatusResponse {
            state: PlaybackState::Idle,
        })
    }

    pub async fn play(&mut self, command: PlayCommand) -> Result<PlaybackStatusResponse> {
        self.stop_internal().await?;

        let players = player::detect();
        let selected_player = players
            .first()
            .context("No supported media player was found on the system")?;

        let cache = PlaybackCache::new()?;
        let session = Arc::new(TorrentSession::new(cache.path()).await?);
        let server = stream_server::serve(session.clone()).await?;

        let (torrent_id, videos, position, quality) =
            match load_torrent(&session, &command.magnet, command.file_index).await {
                Ok((id, videos, pos)) => (id, videos, pos, "4K".to_string()),
                Err(primary_error) => {
                    if command.alternatives.is_empty() {
                        return Err(primary_error);
                    }
                    load_alternatives(&session, &command.alternatives).await?
                }
            };

        let mut episode_queue = command.queue_seed.and_then(|seed| {
            reconstruct_queue(&seed, videos.get(position).map(|file| file.name.as_str()))
        });

        if let Some(video) = videos.get(position) {
            sync_queue_with_video(&mut episode_queue, video);
        }

        session
            .select_files(torrent_id, &[videos[position].index])
            .await?;

        let subtitle_context = current_subtitle_context(episode_queue.as_ref());
        let prepared = prepare_video(
            &session,
            &self.config.data_dir,
            torrent_id,
            &videos[position],
            &server,
            subtitle_context.as_ref(),
        )
        .await?;

        let managed_player = player::launch(
            selected_player,
            &prepared.url,
            &PlaybackOptions {
                title: prepared.title.clone(),
                subtitle_url: prepared.subtitle_url.clone(),
                start_at: None,
                ipc_socket: None,
                show_output: std::env::var_os("FLIX_PLAYER_LOGS").is_some(),
                file_length: prepared.file_length,
            },
        )?;

        let has_next = (position + 1 < videos.len())
            || episode_queue.as_ref().is_some_and(EpisodeQueue::has_next);
        let player_path = selected_player.path.display().to_string();

        let state = PlaybackState::Playing {
            title: prepared.title.clone(),
            quality: quality.clone(),
            url: prepared.url.clone(),
            subtitle_url: prepared.subtitle_url.clone(),
            file_length: prepared.file_length,
            player_path: player_path.clone(),
            has_next,
        };

        self.active_session = Some(ActivePlayback {
            _cache: cache,
            session,
            server,
            torrent_id,
            videos,
            position,
            player: managed_player,
            player_path,
            episode_queue,
            quality,
            title: prepared.title,
        });

        Ok(PlaybackStatusResponse { state })
    }

    pub async fn next(&mut self, _command: NextEpisodeCommand) -> Result<NextEpisodeResponse> {
        let players = player::detect();
        let selected_player = players
            .first()
            .context("No supported media player was found on the system")?;

        let active = self
            .active_session
            .as_mut()
            .context("No active playback session to advance")?;

        let next_episode = active.episode_queue.as_ref().and_then(EpisodeQueue::next);
        let subtitle_context = active.episode_queue.as_ref().and_then(|queue| {
            next_episode
                .as_ref()
                .and_then(|ep| queue.subtitle_context(ep))
        });

        if active.has_next_video() {
            let next_position = active.position + 1;
            let next_video = &active.videos[next_position];
            active
                .session
                .select_files(active.torrent_id, &[next_video.index])
                .await?;

            let prepared = prepare_video(
                &active.session,
                &self.config.data_dir,
                active.torrent_id,
                next_video,
                &active.server,
                subtitle_context.as_ref(),
            )
            .await?;

            active.player.stop()?;
            active.position = next_position;
            if next_episode.is_some() {
                if let Some(queue) = &mut active.episode_queue {
                    queue.advance();
                }
            }

            active.player = player::launch(
                selected_player,
                &prepared.url,
                &PlaybackOptions {
                    title: prepared.title.clone(),
                    subtitle_url: prepared.subtitle_url.clone(),
                    start_at: None,
                    ipc_socket: None,
                    show_output: std::env::var_os("FLIX_PLAYER_LOGS").is_some(),
                    file_length: prepared.file_length,
                },
            )?;

            let has_next = active.has_next_video()
                || active
                    .episode_queue
                    .as_ref()
                    .is_some_and(EpisodeQueue::has_next);

            let state = PlaybackState::Playing {
                title: prepared.title.clone(),
                quality: active.quality.clone(),
                url: prepared.url,
                subtitle_url: prepared.subtitle_url,
                file_length: prepared.file_length,
                player_path: active.player_path.clone(),
                has_next,
            };
            return Ok(NextEpisodeResponse { state });
        }

        let episode = next_episode.context("There is no next episode in the queue")?;
        let client = self
            .stremio_client
            .as_ref()
            .context("Stremio client is not available")?;

        let streams = client.streams("series", &episode.id).await?;
        let candidates = automatic_playback_candidates(&streams);
        let stream = candidates
            .first()
            .copied()
            .context("No torrent stream was found for the next episode")?;
        let quality = stream_quality_label(stream).to_string();
        let magnet = stremio::build_magnet(&stream.info_hash, stream.file_name.as_deref())?;

        let alternatives: Vec<PlaySource> = candidates
            .into_iter()
            .skip(1)
            .map(|candidate| {
                Ok(PlaySource {
                    torrent: stremio::build_magnet(
                        &candidate.info_hash,
                        candidate.file_name.as_deref(),
                    )?,
                    file_index: candidate.file_index,
                    quality: stream_quality_label(candidate).to_string(),
                })
            })
            .collect::<Result<Vec<_>>>()?;

        let active = self.active_session.as_mut().unwrap();

        let (next_id, next_videos, next_pos, next_quality) =
            match load_torrent(&active.session, &magnet, stream.file_index).await {
                Ok((id, vids, pos)) => (id, vids, pos, quality),
                Err(primary_error) => {
                    if alternatives.is_empty() {
                        return Err(primary_error);
                    }
                    load_alternatives(&active.session, &alternatives).await?
                }
            };

        let next_video = &next_videos[next_pos];
        active
            .session
            .select_files(next_id, &[next_video.index])
            .await?;

        let prepared = prepare_video(
            &active.session,
            &self.config.data_dir,
            next_id,
            next_video,
            &active.server,
            subtitle_context.as_ref(),
        )
        .await?;

        active.player.stop()?;
        let old_torrent_id = active.torrent_id;
        let _ = active.session.remove(old_torrent_id, true).await;

        active.torrent_id = next_id;
        active.videos = next_videos;
        active.position = next_pos;
        active.quality = next_quality.clone();
        active.title = prepared.title.clone();

        if let Some(queue) = &mut active.episode_queue {
            queue.advance();
        }

        active.player = player::launch(
            selected_player,
            &prepared.url,
            &PlaybackOptions {
                title: prepared.title.clone(),
                subtitle_url: prepared.subtitle_url.clone(),
                start_at: None,
                ipc_socket: None,
                show_output: std::env::var_os("FLIX_PLAYER_LOGS").is_some(),
                file_length: prepared.file_length,
            },
        )?;

        let has_next = active.has_next_video()
            || active
                .episode_queue
                .as_ref()
                .is_some_and(EpisodeQueue::has_next);

        let state = PlaybackState::Playing {
            title: prepared.title,
            quality: next_quality,
            url: prepared.url,
            subtitle_url: prepared.subtitle_url,
            file_length: prepared.file_length,
            player_path: active.player_path.clone(),
            has_next,
        };

        Ok(NextEpisodeResponse { state })
    }

    pub async fn stop(&mut self, _command: StopPlaybackCommand) -> Result<StopPlaybackResponse> {
        self.stop_internal().await?;
        Ok(StopPlaybackResponse { success: true })
    }

    async fn stop_internal(&mut self) -> Result<()> {
        if let Some(mut active) = self.active_session.take() {
            let _ = active.player.stop();
            let _ = active.player.wait();
        }
        Ok(())
    }
}

struct PreparedPlayback {
    title: String,
    url: String,
    subtitle_url: Option<String>,
    file_length: u64,
}

async fn load_torrent(
    session: &TorrentSession,
    torrent: &str,
    preferred_file_index: Option<usize>,
) -> Result<(TorrentId, Vec<TorrentFile>, usize)> {
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
    alternatives: &[PlaySource],
) -> Result<(TorrentId, Vec<TorrentFile>, usize, String)> {
    let mut last_error = None;
    for source in alternatives {
        match load_torrent(session, &source.torrent, source.file_index).await {
            Ok((id, videos, pos)) => return Ok((id, videos, pos, source.quality.clone())),
            Err(error) => last_error = Some(error),
        }
    }
    match last_error {
        Some(error) => Err(error),
        None => Err(anyhow!("No automatic playback alternative was available")),
    }
}

async fn prepare_video(
    session: &TorrentSession,
    data_dir: &std::path::Path,
    id: TorrentId,
    video: &TorrentFile,
    server: &ServerHandle,
    subtitle_context: Option<&SubtitleContext>,
) -> Result<PreparedPlayback> {
    let preparation =
        playback::prepare_interactive(session, data_dir, id, video, subtitle_context).await;
    let selected_subtitle = preparation.subtitle_results.first();
    let subtitle = if let Some(path) = preparation.cached_subtitle {
        Some(path)
    } else if let Some(result) = selected_subtitle {
        match tokio::time::timeout(
            Duration::from_secs(25),
            subtitles::download_english_subtitle(
                data_dir,
                &session.info_hash(id)?,
                video.index,
                result,
            ),
        )
        .await
        {
            Ok(Ok(path)) => Some(path),
            _ => None,
        }
    } else {
        None
    };
    let subtitle_url = subtitle.map(|path| path.to_string_lossy().into_owned());
    Ok(PreparedPlayback {
        title: video.name.clone(),
        url: stream_server::stream_url(server, id, video.index),
        subtitle_url,
        file_length: video.length,
    })
}

fn current_subtitle_context(queue: Option<&EpisodeQueue>) -> Option<SubtitleContext> {
    queue.and_then(|queue| queue.subtitle_context(queue.current()))
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

fn reconstruct_queue(
    seed: &EpisodeQueueSeed,
    current_video_name: Option<&str>,
) -> Option<EpisodeQueue> {
    let item = CatalogItem {
        id: seed.catalog_item.id.clone(),
        media_type: seed.catalog_item.media_type.clone(),
        name: seed.catalog_item.name.clone(),
        release_info: seed.catalog_item.release_info.clone(),
        kind: if seed.catalog_item.is_anime {
            CatalogKind::AnimeKitsu
        } else {
            CatalogKind::Cinemeta
        },
    };
    let episodes: Vec<Episode> = seed
        .episodes
        .iter()
        .map(|ep| Episode {
            id: ep.id.clone(),
            title: ep.title.clone(),
            season: ep.season,
            episode: ep.episode,
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
