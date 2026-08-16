use crate::stremio::{CatalogItem, CatalogKind, Episode};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SearchCommand {
    pub query: String,
    #[serde(default)]
    pub anime_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CatalogItemSummary {
    pub id: String,
    pub name: String,
    pub media_type: String,
    pub release_info: Option<String>,
    pub is_anime: bool,
}

impl From<&CatalogItem> for CatalogItemSummary {
    fn from(item: &CatalogItem) -> Self {
        Self {
            id: item.id.clone(),
            name: item.name.clone(),
            media_type: item.media_type.clone(),
            release_info: item.release_info.clone(),
            is_anime: item.kind == CatalogKind::AnimeKitsu,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SearchResponse {
    pub items: Vec<CatalogItemSummary>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EpisodesCommand {
    pub item_id: String,
    pub is_anime: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EpisodeSummary {
    pub id: String,
    pub title: Option<String>,
    pub season: u32,
    pub episode: u32,
}

impl From<&Episode> for EpisodeSummary {
    fn from(episode: &Episode) -> Self {
        Self {
            id: episode.id.clone(),
            title: episode.title.clone(),
            season: episode.season,
            episode: episode.episode,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EpisodesResponse {
    pub episodes: Vec<EpisodeSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StreamsCommand {
    pub media_type: String,
    pub stream_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StreamSummary {
    pub info_hash: String,
    pub name: String,
    pub file_name: Option<String>,
    pub file_index: Option<usize>,
    pub quality: String,
    pub is_recommended: bool,
    pub seeders: Option<u64>,
    pub size: Option<String>,
    pub magnet: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StreamsResponse {
    pub streams: Vec<StreamSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlaySource {
    pub torrent: String,
    pub file_index: Option<usize>,
    pub quality: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EpisodeQueueSeed {
    pub catalog_item: CatalogItemSummary,
    pub episodes: Vec<EpisodeSummary>,
    pub current_episode_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlayCommand {
    pub magnet: String,
    pub file_index: Option<usize>,
    pub queue_seed: Option<EpisodeQueueSeed>,
    #[serde(default)]
    pub alternatives: Vec<PlaySource>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum PlaybackState {
    Idle,
    Preparing {
        title: String,
        quality: String,
    },
    Playing {
        title: String,
        quality: String,
        url: String,
        subtitle_url: Option<String>,
        file_length: u64,
        player_path: String,
        has_next: bool,
    },
    Stopped,
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlaybackStatusResponse {
    pub state: PlaybackState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NextEpisodeCommand;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NextEpisodeResponse {
    pub state: PlaybackState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StopPlaybackCommand;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StopPlaybackResponse {
    pub success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum DownloadsAction {
    List,
    Add {
        magnet: String,
        file_index: Option<usize>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DownloadsCommand {
    pub action: DownloadsAction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DownloadEntrySummary {
    pub info_hash: String,
    pub display_name: String,
    pub title: Option<String>,
    pub year: Option<u32>,
    pub file_size: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DownloadsResponse {
    pub entries: Vec<DownloadEntrySummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum SettingsAction {
    Get,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SettingsCommand {
    pub action: SettingsAction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlayerSummary {
    pub name: String,
    pub path: String,
    pub is_available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SettingsResponse {
    pub download_dir: String,
    pub data_dir: String,
    pub players: Vec<PlayerSummary>,
    pub opensubtitles_configured: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActivationStatus {
    pub is_activated: bool,
    pub vlc_installed: bool,
    pub vlc_path: Option<String>,
    pub gateway_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RedeemInviteCommand {
    pub invite_code: String,
    pub gateway_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RedeemInviteResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VlcGuidance {
    pub platform: String,
    pub is_installed: bool,
    pub command: Option<String>,
    pub download_url: String,
    pub instructions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiagnosticsReport {
    pub app_version: String,
    pub is_activated: bool,
    pub gateway_reachable: bool,
    pub vlc_status: String,
    pub player_path: Option<String>,
    pub download_dir: String,
    pub data_dir: String,
    pub active_session: bool,
}
