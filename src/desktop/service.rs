use crate::config::Config;
use crate::desktop::commands;
use crate::desktop::types::{
    ActivationStatus, DownloadsCommand, DownloadsResponse, EpisodesCommand, EpisodesResponse,
    NextEpisodeCommand, NextEpisodeResponse, PlayCommand, PlaybackStatusResponse,
    RedeemInviteCommand, RedeemInviteResponse, SearchCommand, SearchResponse, SettingsCommand,
    SettingsResponse, StopPlaybackCommand, StopPlaybackResponse, StreamsCommand, StreamsResponse,
};
use crate::playback::{
    AlternativeSource, MediaRef, OperationAccepted, PlaybackCoordinator, ResolvedSource,
};
use crate::stremio::StremioClient;
use anyhow::{anyhow, Context, Result};
use std::sync::Arc;

pub struct DesktopService {
    config: Config,
    stremio_client: Option<StremioClient>,
    coordinator: Arc<PlaybackCoordinator>,
}

impl DesktopService {
    pub fn new(config: Config) -> Self {
        let stremio_client = StremioClient::from_env().ok();
        let coordinator = Arc::new(PlaybackCoordinator::new(config.clone()));
        Self {
            config,
            stremio_client,
            coordinator,
        }
    }

    pub fn coordinator(&self) -> Arc<PlaybackCoordinator> {
        Arc::clone(&self.coordinator)
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
        commands::handle_streams(client, &self.coordinator, command).await
    }

    pub async fn downloads(&self, command: &DownloadsCommand) -> Result<DownloadsResponse> {
        commands::handle_downloads(&self.config, command).await
    }

    pub fn settings(&self, command: &SettingsCommand) -> Result<SettingsResponse> {
        commands::handle_settings(&self.config, command)
    }

    pub fn vlc_guidance(&self) -> crate::desktop::types::VlcGuidance {
        commands::handle_vlc_guidance()
    }

    pub fn activation_status(&self) -> ActivationStatus {
        commands::handle_activation_status()
    }

    pub async fn redeem_invite(
        &self,
        command: &RedeemInviteCommand,
    ) -> Result<RedeemInviteResponse> {
        commands::handle_redeem_invite(command).await
    }

    pub fn reset_activation(&self) -> Result<()> {
        crate::desktop::credentials::delete_device_token()
    }

    pub fn diagnostics(&self) -> Result<crate::desktop::types::DiagnosticsReport> {
        let has_active = !matches!(
            self.coordinator.snapshot(),
            crate::playback::PlaybackSnapshot::Idle
        );
        commands::handle_diagnostics(&self.config, has_active)
    }

    pub async fn status(&self) -> Result<PlaybackStatusResponse> {
        let snapshot = self.coordinator.snapshot();
        Ok(PlaybackStatusResponse { state: snapshot })
    }

    pub async fn play(&self, command: PlayCommand) -> Result<OperationAccepted> {
        let source = if let Some(source_id) = &command.source_id {
            self.coordinator
                .get_resolved_source(source_id)
                .await
                .ok_or_else(|| anyhow!("Unknown source identifier: {}", source_id))?
        } else if let Some(magnet) = &command.magnet {
            let alternatives = command
                .alternatives
                .iter()
                .map(|alt| AlternativeSource {
                    magnet: alt.torrent.clone(),
                    file_index: alt.file_index,
                    quality: alt.quality.clone(),
                })
                .collect();
            ResolvedSource {
                magnet: magnet.clone(),
                file_index: command.file_index,
                quality: "4K".to_string(),
                alternatives,
            }
        } else {
            return Err(anyhow!("Missing source_id or magnet in PlayCommand"));
        };

        let media = command.media_ref.unwrap_or_else(|| {
            if let Some(seed) = &command.queue_seed {
                if let Some(ep) = seed
                    .episodes
                    .iter()
                    .find(|e| e.id == seed.current_episode_id)
                {
                    MediaRef::Episode {
                        catalog_id: seed.catalog_item.id.clone(),
                        stream_id: ep.stream_id.clone(),
                        imdb_id: ep.imdb_id.clone(),
                        season: ep.season,
                        episode: ep.episode,
                        title: ep.title.clone(),
                    }
                } else {
                    MediaRef::Movie {
                        catalog_id: seed.catalog_item.id.clone(),
                        title: seed.catalog_item.name.clone(),
                    }
                }
            } else {
                MediaRef::Movie {
                    catalog_id: "unknown".to_string(),
                    title: "Playing Media".to_string(),
                }
            }
        });

        let res = self
            .coordinator
            .start_playback(media, source, command.queue_seed)
            .await;
        Ok(res)
    }

    pub async fn next(&self, _command: NextEpisodeCommand) -> Result<NextEpisodeResponse> {
        self.coordinator.next().await?;
        let snapshot = self.coordinator.snapshot();
        Ok(NextEpisodeResponse { state: snapshot })
    }

    pub async fn stop(&self, _command: StopPlaybackCommand) -> Result<StopPlaybackResponse> {
        self.coordinator.stop().await?;
        Ok(StopPlaybackResponse { success: true })
    }
}
