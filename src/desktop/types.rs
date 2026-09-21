use crate::playback::{MediaRef, PlaybackSnapshot};
use crate::stremio::{CatalogItem, CatalogKind, Episode};
use serde::{Deserialize, Serialize};

fn default_reader_lang() -> String {
    "en".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SearchCommand {
    pub query: String,
    #[serde(default)]
    pub anime_only: bool,
    #[serde(default)]
    pub catalog: Option<crate::stremio::SearchCatalog>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CatalogItemSummary {
    pub id: String,
    pub name: String,
    pub media_type: String,
    pub release_info: Option<String>,
    pub poster: Option<String>,
    pub is_anime: bool,
}

impl From<&CatalogItem> for CatalogItemSummary {
    fn from(item: &CatalogItem) -> Self {
        Self {
            id: item.id.clone(),
            name: item.name.clone(),
            media_type: item.media_type.clone(),
            release_info: item.release_info.clone(),
            poster: item.poster.clone(),
            is_anime: item.kind == CatalogKind::AnimeKitsu
                || item.genres.as_ref().is_some_and(|genres| {
                    genres
                        .iter()
                        .any(|genre| genre.eq_ignore_ascii_case("anime"))
                }),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SearchResponse {
    pub items: Vec<CatalogItemSummary>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecommendCommand {
    pub keywords: String,
    #[serde(default)]
    pub show: bool,
    #[serde(default)]
    pub movie: bool,
    #[serde(default)]
    pub anime: bool,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub min_rating: Option<f64>,
    #[serde(default)]
    pub since_year: Option<u32>,
    /// Number of ranked results to skip. Discover pages with this value.
    #[serde(default)]
    pub offset: usize,
}

fn default_limit() -> usize {
    20
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecommendItemSummary {
    pub id: String,
    pub name: String,
    pub media_type: String,
    pub release_info: Option<String>,
    pub poster: Option<String>,
    pub genres: Option<Vec<String>>,
    pub imdb_rating: Option<String>,
    pub description: Option<String>,
    pub is_anime: bool,
    pub score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecommendResponse {
    pub items: Vec<RecommendItemSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EpisodesCommand {
    pub item_id: String,
    pub is_anime: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TitleLookupCommand {
    pub catalog_id: String,
    pub media_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EpisodeSummary {
    pub id: String,
    #[serde(default)]
    pub stream_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub imdb_id: Option<String>,
    pub title: Option<String>,
    pub season: u32,
    pub episode: u32,
}

impl EpisodeSummary {
    pub fn display_label(&self) -> String {
        let ep = format!("S{:02}E{:02}", self.season, self.episode);
        match &self.title {
            Some(title) if !title.trim().is_empty() => format!("{} · {}", ep, title.trim()),
            _ => ep,
        }
    }
}

impl From<&Episode> for EpisodeSummary {
    fn from(episode: &Episode) -> Self {
        Self {
            id: episode.id.clone(),
            stream_id: episode.stream_id.clone(),
            imdb_id: episode.imdb_id.clone(),
            title: episode.title.clone(),
            season: episode.season,
            episode: episode.episode,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EpisodesResponse {
    pub episodes: Vec<EpisodeSummary>,
    pub is_anime: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StreamsCommand {
    #[serde(default)]
    pub is_anime: bool,
    #[serde(default)]
    pub media_type: Option<String>,
    pub stream_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StreamSummary {
    pub source_id: String,
    pub name: String,
    pub file_name: Option<String>,
    pub file_index: Option<usize>,
    pub quality: String,
    pub is_recommended: bool,
    pub seeders: Option<u64>,
    pub size: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StreamsResponse {
    pub streams: Vec<StreamSummary>,
    pub is_anime: bool,
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
    #[serde(default)]
    pub target: crate::playback::types::PlaybackTarget,
    #[serde(default)]
    pub source_id: Option<String>,
    #[serde(default)]
    pub magnet: Option<String>,
    pub file_index: Option<usize>,
    #[serde(default)]
    pub media_ref: Option<MediaRef>,
    pub queue_seed: Option<EpisodeQueueSeed>,
    #[serde(default)]
    pub alternatives: Vec<PlaySource>,
}

pub type PlaybackState = PlaybackSnapshot;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlaybackStatusResponse {
    pub state: PlaybackSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NextEpisodeCommand;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NextEpisodeResponse {
    pub state: PlaybackSnapshot,
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
    Remove {
        info_hash: String,
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
    pub gateway_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RedeemInviteCommand {
    pub invite_code: String,
    #[serde(default)]
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
    pub vlc_status: String,
    pub player_path: Option<String>,
    pub download_dir: String,
    pub data_dir: String,
    pub active_session: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReaderSearchCommand {
    pub query: String,
    #[serde(default = "default_reader_lang")]
    pub lang: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReaderSearchResponse {
    pub publications: Vec<crate::reader::Publication>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReaderChaptersCommand {
    pub manga_id: String,
    #[serde(default = "default_reader_lang")]
    pub lang: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReaderChaptersResponse {
    pub chapters: Vec<crate::reader::Chapter>,
    pub available_languages: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReaderPagesCommand {
    pub chapter_id: String,
    pub manga_id: String,
    #[serde(default = "default_reader_lang")]
    pub lang: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReaderPagesResponse {
    pub chapter_id: String,
    pub page_count: usize,
    pub pages: Vec<String>, // local /api/reader/page?... URLs
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReaderProgressResponse {
    pub position: Option<crate::reader::progress::ReadingPosition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReaderProgressSaveCommand {
    pub publication_id: String,
    pub chapter_id: String,
    pub page: usize,
}

// ---- Profiles, lists, and watch status ----

fn default_avatar_key() -> String {
    "amber".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateProfileCommand {
    pub name: String,
    #[serde(default = "default_avatar_key")]
    pub avatar_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateProfileCommand {
    pub profile_id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub avatar_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileIdCommand {
    pub profile_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfileSummary {
    pub id: String,
    pub name: String,
    pub avatar_key: String,
    pub created_at: i64,
    pub updated_at: i64,
}

impl From<crate::profiles::Profile> for ProfileSummary {
    fn from(profile: crate::profiles::Profile) -> Self {
        Self {
            id: profile.id,
            name: profile.name,
            avatar_key: profile.avatar_key,
            created_at: profile.created_at,
            updated_at: profile.updated_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfilesResponse {
    pub profiles: Vec<ProfileSummary>,
    // The session's current profile, or None before it has been chosen at
    // the picker (only possible when more than one profile exists).
    pub active_profile_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateListCommand {
    pub profile_id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListIdCommand {
    pub list_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ListItemSummary {
    pub catalog_id: String,
    pub media_type: String,
    pub added_at: i64,
    pub title: Option<String>,
    pub poster: Option<String>,
    pub year: Option<i64>,
    pub canonical_id: Option<String>,
}

impl From<crate::profiles::ListItem> for ListItemSummary {
    fn from(item: crate::profiles::ListItem) -> Self {
        Self {
            catalog_id: item.catalog_id,
            media_type: item.media_type,
            added_at: item.added_at,
            title: item.title,
            poster: item.poster,
            year: item.year,
            canonical_id: item.canonical_id,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ListSummary {
    pub id: String,
    pub profile_id: String,
    pub name: String,
    pub created_at: i64,
    pub items: Vec<ListItemSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListsResponse {
    pub lists: Vec<ListSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddListItemCommand {
    pub list_id: String,
    pub catalog_id: String,
    pub media_type: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub poster: Option<String>,
    #[serde(default)]
    pub year: Option<i64>,
    /// The IMDb id of the same title, when the client knows it. It stops one
    /// title from appearing twice under a `kitsu:` id and a `tt` id.
    #[serde(default)]
    pub canonical_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveListItemCommand {
    pub list_id: String,
    pub catalog_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveWatchStatusCommand {
    pub profile_id: String,
    pub catalog_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetWatchStatusCommand {
    pub profile_id: String,
    pub catalog_id: String,
    /// `to_watch`, `in_progress`, or `watched`.
    pub status: String,
    #[serde(default)]
    pub episode_id: Option<String>,
    #[serde(default)]
    pub resume_seconds: Option<i64>,
    /// Only needed the first time a title gets a status (usually from the
    /// add-to-list/set-status popover, which already has the catalog item's
    /// name and poster on hand) - omit on a later call (e.g. from the
    /// progress tracker) and the existing stored title/poster is kept.
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub poster: Option<String>,
    /// `movie`, `series`, or `anime`. Same "only needed the first time" rule
    /// as `title`/`poster` - needed to reopen this title directly from the
    /// watch-status board without a text search.
    #[serde(default)]
    pub media_type: Option<String>,
    #[serde(default)]
    pub year: Option<i64>,
    #[serde(default)]
    pub canonical_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WatchStatusSummary {
    pub catalog_id: String,
    pub status: String,
    pub episode_id: Option<String>,
    pub resume_seconds: Option<i64>,
    pub updated_at: i64,
    pub title: Option<String>,
    pub poster: Option<String>,
    pub media_type: Option<String>,
    pub year: Option<i64>,
    pub canonical_id: Option<String>,
}

impl From<crate::profiles::WatchStatus> for WatchStatusSummary {
    fn from(status: crate::profiles::WatchStatus) -> Self {
        Self {
            catalog_id: status.catalog_id,
            status: status.status.as_str().to_string(),
            episode_id: status.episode_id,
            resume_seconds: status.resume_seconds,
            updated_at: status.updated_at,
            title: status.title,
            poster: status.poster,
            media_type: status.media_type,
            year: status.year,
            canonical_id: status.canonical_id,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchStatusResponse {
    pub watch_status: Vec<WatchStatusSummary>,
}
