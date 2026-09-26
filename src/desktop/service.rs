use crate::config::Config;
use crate::desktop::commands;
use crate::desktop::poster_cache::{self, PosterCache};
use crate::desktop::signed_url::{self, UrlSigner};
use crate::desktop::types::{
    ActivationStatus, AddListItemCommand, CreateListCommand, CreateProfileCommand,
    DownloadsCommand, DownloadsResponse, EpisodesCommand, EpisodesResponse, ListIdCommand,
    ListsResponse, NextEpisodeCommand, NextEpisodeResponse, PlayCommand, PlaybackStatusResponse,
    ProfileIdCommand, ProfileSummary, ProfilesResponse, ReaderChaptersCommand,
    ReaderChaptersResponse, ReaderPagesCommand, ReaderPagesResponse, ReaderProgressResponse,
    ReaderProgressSaveCommand, ReaderSearchCommand, ReaderSearchResponse, RecommendCommand,
    RecommendResponse, RedeemInviteCommand, RedeemInviteResponse, RemoveListItemCommand,
    RemoveWatchStatusCommand, SearchCommand, SearchResponse, SetWatchStatusCommand,
    SettingsCommand, SettingsResponse, StopPlaybackCommand, StopPlaybackResponse, StreamsCommand,
    StreamsResponse, UpdateProfileCommand, WatchStatusResponse,
};
use crate::desktop::updater::{UpdateInstallResult, UpdateStatus, Updater};
use crate::playback::{
    AlternativeSource, MediaRef, OperationAccepted, PlaybackCoordinator, PlaybackSnapshot,
    ResolvedSource,
};
use crate::profiles::ProfileStore;
use crate::recommend::RecommendPool;
use crate::stremio::StremioClient;
use crate::ttl_cache::TtlCache;
use anyhow::{anyhow, Context, Result};
use std::sync::Arc;
use tokio::sync::Mutex;

const SYNC_MIN_GAP: std::time::Duration = std::time::Duration::from_secs(30);
const PAGE_URL_TTL: std::time::Duration = std::time::Duration::from_secs(10 * 60);
const PAGE_URL_CHAPTERS: usize = 16;
const RECOMMEND_POOL_TTL: std::time::Duration = std::time::Duration::from_secs(5 * 60);
const RECOMMEND_POOLS: usize = 8;

fn reader_page_scope(chapter_id: &str, manga_id: &str, index: usize) -> String {
    format!("reader|{chapter_id}|{manga_id}|{index}")
}

fn encode_query_value(value: &str) -> String {
    percent_encoding::utf8_percent_encode(value, percent_encoding::NON_ALPHANUMERIC).to_string()
}

pub struct DesktopService {
    config: Config,
    stremio_client: Option<StremioClient>,
    coordinator: Arc<PlaybackCoordinator>,
    profiles: Arc<ProfileStore>,
    sync_gate: Arc<crate::desktop::profile_sync::SyncGate>,
    url_signer: UrlSigner,
    page_urls: TtlCache<Vec<String>>,
    recommend_pools: TtlCache<Mutex<RecommendPool>>,
    poster_cache: PosterCache,
    updater: Updater,
    // The profile every list, status, and progress write in this running
    // session is attributed to. See `set_active_profile` for why changing it
    // is refused during playback.
    current_profile_id: Arc<Mutex<Option<String>>>,
}

impl DesktopService {
    pub fn new(config: Config) -> Result<Self> {
        let stremio_client = StremioClient::from_env().ok();
        let coordinator = Arc::new(PlaybackCoordinator::new(config.clone()));
        let profiles = Arc::new(ProfileStore::open(&config.data_dir)?);
        let poster_cache = PosterCache::new(&config.data_dir)?;
        let updater = Updater::new(&config.data_dir)?;

        // A single-profile household should never see a picker: the one
        // profile that exists is simply active from the start.
        let initial_profile_id = {
            let mut existing = profiles.list_profiles()?;
            (existing.len() == 1).then(|| existing.remove(0).id)
        };

        Ok(Self {
            config,
            stremio_client,
            coordinator,
            profiles,
            sync_gate: crate::desktop::profile_sync::SyncGate::new(SYNC_MIN_GAP),
            url_signer: UrlSigner::new(),
            page_urls: TtlCache::new(PAGE_URL_TTL, PAGE_URL_CHAPTERS),
            recommend_pools: TtlCache::new(RECOMMEND_POOL_TTL, RECOMMEND_POOLS),
            poster_cache,
            updater,
            current_profile_id: Arc::new(Mutex::new(initial_profile_id)),
        })
    }

    pub fn coordinator(&self) -> Arc<PlaybackCoordinator> {
        Arc::clone(&self.coordinator)
    }

    pub fn with_updater(mut self, updater: Updater) -> Self {
        self.updater = updater;
        self
    }

    pub async fn check_update(&self) -> Result<UpdateStatus> {
        self.updater.check().await
    }

    pub async fn install_update(&self) -> Result<UpdateInstallResult> {
        self.updater.download_and_launch().await
    }

    pub fn profiles(&self) -> Arc<ProfileStore> {
        Arc::clone(&self.profiles)
    }

    pub async fn current_profile_id(&self) -> Option<String> {
        self.current_profile_id.lock().await.clone()
    }

    /// A shared handle onto the active-profile cell, for the progress
    /// tracker task to read from directly rather than holding a whole
    /// `DesktopService`.
    pub fn current_profile_id_handle(&self) -> Arc<Mutex<Option<String>>> {
        Arc::clone(&self.current_profile_id)
    }

    /// Sets the profile every list, status, and progress write in this
    /// session attributes to. Refused while playback is not idle - this is
    /// what guarantees two profiles' progress can never mix, by construction
    /// rather than by careful bookkeeping in the progress tracker.
    pub async fn set_active_profile(&self, profile_id: String) -> Result<()> {
        if !matches!(self.coordinator.snapshot(), PlaybackSnapshot::Idle) {
            return Err(anyhow!(
                "Cannot switch profiles while playback is active. Stop playback first."
            ));
        }
        let profiles = self.profiles();
        let exists = tokio::task::spawn_blocking(move || profiles.list_profiles())
            .await
            .context("Profile lookup task panicked")??
            .into_iter()
            .any(|profile| profile.id == profile_id);
        if !exists {
            return Err(anyhow!("No profile with id {profile_id}"));
        }
        *self.current_profile_id.lock().await = Some(profile_id);
        Ok(())
    }

    fn get_stremio_client(&self) -> Result<&StremioClient> {
        self.stremio_client
            .as_ref()
            .context("Stremio client is not available")
    }

    pub async fn search(&self, command: &SearchCommand) -> Result<SearchResponse> {
        let client = self.get_stremio_client()?;
        let started = std::time::Instant::now();
        let response = commands::handle_search(client, command)
            .await
            .map(|mut response| {
                for item in &mut response.items {
                    item.poster = self.proxy_poster(item.poster.take());
                }
                response
            });
        self.coordinator
            .record_debug_details(crate::desktop::debug_report::DebugDetails::Search {
                catalog: command.catalog,
                duration_ms: started.elapsed().as_millis() as u64,
                items: response.as_ref().map_or(0, |response| response.items.len()),
                failed: response
                    .as_ref()
                    .map_or(true, |response| !response.notes.is_empty()),
            });
        response
    }

    pub async fn recommend(&self, command: &RecommendCommand) -> Result<RecommendResponse> {
        let client = self.get_stremio_client()?;
        // One fetch serves every page of a query for a few minutes.
        let key = format!(
            "{}|{}|{}|{}",
            command.keywords.trim().to_lowercase(),
            command.show,
            command.movie,
            command.anime
        );
        let handle = match self.recommend_pools.get(&key) {
            Some(handle) => handle,
            None => self
                .recommend_pools
                .insert(&key, Mutex::new(RecommendPool::default())),
        };
        let mut response = {
            let mut pool = handle.lock().await;
            let mut page = commands::page_recommend_pool(&pool.items, command);
            // A page with too few items means the pool is short: fetch the
            // next round of add-on pages and cut the page again.
            while page.items.len() < command.limit && pool.can_grow() {
                let round = commands::fetch_recommend_round(client, command, pool.rounds).await?;
                pool.absorb(round);
                page = commands::page_recommend_pool(&pool.items, command);
            }
            page
        };
        for item in &mut response.items {
            item.poster = self.proxy_poster(item.poster.take());
        }
        Ok(response)
    }

    /// Records how long one local profile-store read took, for the
    /// exported diagnostics report. `rows` is the size of the result, so a
    /// slow query and a merely large one read differently in the report.
    fn record_db_timing(&self, operation: &'static str, started: std::time::Instant, rows: usize) {
        self.coordinator
            .record_debug_details(crate::desktop::debug_report::DebugDetails::Db {
                operation,
                duration_ms: started.elapsed().as_millis() as u64,
                rows,
            });
    }

    /// The signed local address of a poster, or the original address when
    /// its host is not on the allowed list.
    pub fn proxy_poster(&self, poster: Option<String>) -> Option<String> {
        poster.map(|url| {
            poster_cache::proxied_poster_url(&self.url_signer, &url, signed_url::unix_now())
        })
    }

    /// True when the query carries a valid, unexpired signature for this poster.
    pub fn verify_poster(&self, upstream: &str, expires_at: u64, signature: &str) -> bool {
        poster_cache::verify_poster_url(
            &self.url_signer,
            upstream,
            expires_at,
            signature,
            signed_url::unix_now(),
        )
    }

    pub async fn poster(&self, upstream: &str) -> Result<(Vec<u8>, &'static str)> {
        self.poster_cache.get(upstream).await
    }

    pub async fn episodes(&self, command: &EpisodesCommand) -> Result<EpisodesResponse> {
        let client = self.get_stremio_client()?;
        commands::handle_episodes(client, command).await
    }

    pub async fn title_lookup(
        &self,
        command: &crate::desktop::types::TitleLookupCommand,
    ) -> Result<crate::desktop::types::CatalogItemSummary> {
        let catalog_id = command.catalog_id.trim();
        let media_type = command.media_type.trim();
        if catalog_id.is_empty() || media_type.is_empty() {
            return Err(anyhow!("catalog_id and media_type are required"));
        }

        let started = std::time::Instant::now();
        let profiles = self.profiles();
        let cached = tokio::task::spawn_blocking({
            let profiles = Arc::clone(&profiles);
            let id = catalog_id.to_string();
            move || profiles.get_title(&id)
        })
        .await
        .context("Failed to spawn title lookup task")??;

        if let Some(stored) = cached {
            if let Some(title_name) = stored.title {
                self.record_db_timing("title_lookup", started, 1);
                let is_anime = stored.media_type.as_deref() == Some("anime")
                    || catalog_id.starts_with("kitsu:");
                let summary = crate::desktop::types::CatalogItemSummary {
                    id: stored.catalog_id,
                    name: title_name,
                    media_type: stored.media_type.unwrap_or_else(|| media_type.to_string()),
                    release_info: stored.year.map(|y| y.to_string()),
                    poster: self.proxy_poster(stored.poster),
                    is_anime,
                };
                return Ok(summary);
            }
        }
        self.record_db_timing("title_lookup", started, 0);

        let client = self.get_stremio_client()?;
        let mut item = commands::handle_title_lookup(client, command).await?;

        let year = item
            .release_info
            .as_deref()
            .and_then(|info| info.chars().take(4).collect::<String>().parse::<i64>().ok());
        let save_result = tokio::task::spawn_blocking({
            let profiles = Arc::clone(&profiles);
            let id = catalog_id.to_string();
            let m_type = item.media_type.clone();
            let name = item.name.clone();
            let poster = item.poster.clone();
            move || {
                profiles.save_title(
                    &id,
                    &crate::profiles::TitleMeta {
                        media_type: Some(&m_type),
                        title: Some(&name),
                        poster: poster.as_deref(),
                        year,
                        canonical_id: None,
                    },
                )
            }
        })
        .await;
        match save_result {
            Ok(Err(error)) => tracing::warn!("Failed to cache a looked-up title: {error:#}"),
            Err(error) => tracing::warn!("Title cache write task panicked: {error}"),
            Ok(Ok(())) => {}
        }

        item.poster = self.proxy_poster(item.poster.take());
        Ok(item)
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
                is_anime: command.media_ref.as_ref().is_some_and(MediaRef::is_anime)
                    || command
                        .queue_seed
                        .as_ref()
                        .is_some_and(|seed| seed.catalog_item.is_anime),
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
            .start_playback_on(media, source, command.queue_seed, command.target)
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

    pub async fn reader_search(
        &self,
        command: &ReaderSearchCommand,
    ) -> Result<ReaderSearchResponse> {
        commands::handle_reader_search(command).await
    }

    pub async fn reader_chapters(
        &self,
        command: &ReaderChaptersCommand,
    ) -> Result<ReaderChaptersResponse> {
        commands::handle_reader_chapters(command).await
    }

    /// Looks up the chapter once, keeps the upstream URLs in memory, and
    /// returns one signed local URL for each page.
    pub async fn reader_pages(&self, command: &ReaderPagesCommand) -> Result<ReaderPagesResponse> {
        let urls = commands::fetch_reader_page_urls(&command.chapter_id).await?;
        let page_count = self.page_urls.insert(&command.chapter_id, urls).len();
        let pages = (0..page_count)
            .map(|index| self.signed_reader_page_url(&command.chapter_id, &command.manga_id, index))
            .collect();
        Ok(ReaderPagesResponse {
            chapter_id: command.chapter_id.clone(),
            page_count,
            pages,
        })
    }

    pub fn signed_reader_page_url(&self, chapter_id: &str, manga_id: &str, index: usize) -> String {
        let (expires_at, signature) = self
            .url_signer
            .grant(&reader_page_scope(chapter_id, manga_id, index));
        format!(
            "/api/reader/page?chapter_id={}&manga_id={}&index={index}&exp={expires_at}&sig={signature}",
            encode_query_value(chapter_id),
            encode_query_value(manga_id),
        )
    }

    /// True when the query carries a valid, unexpired signature for this page.
    pub fn verify_reader_page(
        &self,
        chapter_id: &str,
        manga_id: &str,
        index: usize,
        expires_at: u64,
        signature: &str,
    ) -> bool {
        self.url_signer.verify(
            &reader_page_scope(chapter_id, manga_id, index),
            expires_at,
            signature,
            signed_url::unix_now(),
        )
    }

    /// Stores a chapter page list. Tests use it to run the reader without
    /// the network.
    pub fn seed_reader_pages(&self, chapter_id: &str, urls: Vec<String>) {
        self.page_urls.insert(chapter_id, urls);
    }

    pub async fn reader_page_download(
        &self,
        chapter_id: &str,
        manga_id: &str,
        index: usize,
    ) -> Result<(Vec<u8>, String)> {
        let urls = match self.page_urls.get(chapter_id) {
            Some(urls) => urls,
            None => {
                let fresh = commands::fetch_reader_page_urls(chapter_id).await?;
                self.page_urls.insert(chapter_id, fresh)
            }
        };
        let url = urls
            .get(index)
            .ok_or_else(|| anyhow!("Page index {index} out of range"))?;
        commands::handle_reader_page_download(&self.config, chapter_id, manga_id, index, url).await
    }

    pub fn reader_progress_get(&self, publication_id: &str) -> Result<ReaderProgressResponse> {
        commands::handle_reader_progress_get(&self.config, publication_id)
    }

    pub fn reader_progress_save(&self, command: &ReaderProgressSaveCommand) -> Result<()> {
        commands::handle_reader_progress_save(&self.config, command)
    }

    // ---- Profiles, lists, and watch status ----
    //
    // Every call below runs the actual SQLite work inside `spawn_blocking`,
    // so a disk write here can never stall the async runtime worker threads
    // the playback coordinator and torrent I/O also run on.

    async fn with_profiles<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&crate::profiles::ProfileStore) -> Result<T> + Send + 'static,
        T: Send + 'static,
    {
        let store = self.profiles();
        tokio::task::spawn_blocking(move || f(&store))
            .await
            .context("Profile store task panicked")?
    }

    /// Fires a best-effort sync in the background and returns immediately.
    /// Never awaited by a caller: a slow or failed gateway must not delay
    /// the local read the caller actually asked for. Does nothing when the
    /// device has never been activated, while playback is active, while a
    /// sync already runs, or within the minimum gap after the last start.
    fn trigger_background_sync(&self) {
        if !matches!(self.coordinator.snapshot(), PlaybackSnapshot::Idle) {
            return;
        }
        let Some(permit) = self.sync_gate.try_start() else {
            return;
        };
        let profiles = self.profiles();
        tokio::spawn(async move {
            let _permit = permit;
            match crate::desktop::profile_sync::GatewaySyncClient::from_saved_activation() {
                Ok(Some(client)) => crate::desktop::profile_sync::sync_all(profiles, &client).await,
                Ok(None) => {}
                Err(error) => tracing::warn!("Sync: could not build gateway client: {error:#}"),
            }
        });
    }

    pub async fn list_profiles(&self) -> Result<ProfilesResponse> {
        self.trigger_background_sync();
        let started = std::time::Instant::now();
        let active = self.current_profile_id().await;
        let response = self
            .with_profiles(move |store| commands::handle_list_profiles(store, active))
            .await?;
        self.record_db_timing("list_profiles", started, response.profiles.len());
        Ok(response)
    }

    pub async fn create_profile(&self, command: CreateProfileCommand) -> Result<ProfileSummary> {
        self.with_profiles(move |store| commands::handle_create_profile(store, &command))
            .await
    }

    pub async fn update_profile(&self, command: UpdateProfileCommand) -> Result<()> {
        self.with_profiles(move |store| commands::handle_update_profile(store, &command))
            .await
    }

    pub async fn delete_profile(&self, command: ProfileIdCommand) -> Result<()> {
        let deleted_id = command.profile_id.clone();
        self.with_profiles(move |store| commands::handle_delete_profile(store, &command))
            .await?;
        // A deleted profile can't stay active: fall back to no active
        // profile, exactly as if the app had just started with it removed.
        let mut current = self.current_profile_id.lock().await;
        if current.as_deref() == Some(deleted_id.as_str()) {
            *current = None;
        }
        Ok(())
    }

    pub async fn create_list(&self, command: CreateListCommand) -> Result<()> {
        self.with_profiles(move |store| commands::handle_create_list(store, &command))
            .await
    }

    pub async fn delete_list(&self, command: ListIdCommand) -> Result<()> {
        self.with_profiles(move |store| commands::handle_delete_list(store, &command))
            .await
    }

    pub async fn list_lists(&self, profile_id: String) -> Result<ListsResponse> {
        self.trigger_background_sync();
        let started = std::time::Instant::now();
        let mut response = self
            .with_profiles(move |store| commands::handle_list_lists(store, &profile_id))
            .await?;
        let rows: usize = response.lists.iter().map(|list| list.items.len()).sum();
        self.record_db_timing("list_lists", started, rows);
        for item in response
            .lists
            .iter_mut()
            .flat_map(|list| list.items.iter_mut())
        {
            item.poster = self.proxy_poster(item.poster.take());
        }
        Ok(response)
    }

    pub async fn add_list_item(&self, command: AddListItemCommand) -> Result<()> {
        self.with_profiles(move |store| commands::handle_add_list_item(store, &command))
            .await
    }

    pub async fn remove_list_item(&self, command: RemoveListItemCommand) -> Result<()> {
        self.with_profiles(move |store| commands::handle_remove_list_item(store, &command))
            .await
    }

    pub async fn set_watch_status(&self, command: SetWatchStatusCommand) -> Result<()> {
        self.with_profiles(move |store| commands::handle_set_watch_status(store, &command))
            .await
    }

    pub async fn remove_watch_status(&self, command: RemoveWatchStatusCommand) -> Result<()> {
        self.with_profiles(move |store| commands::handle_remove_watch_status(store, &command))
            .await
    }

    pub async fn list_watch_status(
        &self,
        profile_id: String,
        status: Option<String>,
    ) -> Result<WatchStatusResponse> {
        self.trigger_background_sync();
        let started = std::time::Instant::now();
        let mut response = self
            .with_profiles(move |store| {
                commands::handle_list_watch_status(store, &profile_id, status.as_deref())
            })
            .await?;
        self.record_db_timing("list_watch_status", started, response.watch_status.len());
        for item in &mut response.watch_status {
            item.poster = self.proxy_poster(item.poster.take());
        }
        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::playback::PlaybackSnapshot;

    /// Returns a service plus the `TempDir` its data directory lives in.
    /// The directory must stay alive (and bound in the caller's own scope)
    /// for as long as the service, or a re-open against the same path in
    /// the same test would find nothing there.
    fn test_service() -> (DesktopService, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let config = Config {
            data_dir: dir.path().join("data"),
            download_dir: dir.path().join("downloads"),
        };
        (DesktopService::new(config).unwrap(), dir)
    }

    #[tokio::test]
    async fn a_single_profile_is_active_from_startup_with_no_picker_step() {
        let (service, dir) = test_service();
        let created = service
            .create_profile(CreateProfileCommand {
                name: "Ada".to_string(),
                avatar_key: "star-violet".to_string(),
            })
            .await
            .unwrap();

        // Re-open against the same data directory: this is what a real
        // restart does, and is the only point auto-selection runs.
        let config = Config {
            data_dir: dir.path().join("data"),
            download_dir: dir.path().join("downloads"),
        };
        let reopened = DesktopService::new(config).unwrap();
        assert_eq!(reopened.current_profile_id().await, Some(created.id));
    }

    #[tokio::test]
    async fn multiple_profiles_leave_none_active_until_chosen() {
        let (service, dir) = test_service();
        service
            .create_profile(CreateProfileCommand {
                name: "Ada".to_string(),
                avatar_key: "star-violet".to_string(),
            })
            .await
            .unwrap();
        service
            .create_profile(CreateProfileCommand {
                name: "Femi".to_string(),
                avatar_key: "bolt-amber".to_string(),
            })
            .await
            .unwrap();

        let config = Config {
            data_dir: dir.path().join("data"),
            download_dir: dir.path().join("downloads"),
        };
        let reopened = DesktopService::new(config).unwrap();
        assert_eq!(reopened.current_profile_id().await, None);
    }

    #[tokio::test]
    async fn set_active_profile_succeeds_while_idle() {
        let (service, _dir) = test_service();
        let profile = service
            .create_profile(CreateProfileCommand {
                name: "Ada".to_string(),
                avatar_key: "star-violet".to_string(),
            })
            .await
            .unwrap();

        service
            .set_active_profile(profile.id.clone())
            .await
            .unwrap();
        assert_eq!(service.current_profile_id().await, Some(profile.id));
    }

    #[tokio::test]
    async fn set_active_profile_rejects_an_unknown_id() {
        let (service, _dir) = test_service();
        let err = service
            .set_active_profile("does-not-exist".to_string())
            .await
            .unwrap_err();
        assert!(err.to_string().contains("No profile"));
    }

    #[tokio::test]
    async fn set_active_profile_is_refused_while_playback_is_not_idle() {
        // This is the invariant that stops two profiles' progress from ever
        // mixing: see the Architecture decisions in the implementation plan.
        let (service, _dir) = test_service();
        let profile = service
            .create_profile(CreateProfileCommand {
                name: "Ada".to_string(),
                avatar_key: "star-violet".to_string(),
            })
            .await
            .unwrap();

        service
            .coordinator
            .force_snapshot_for_test(PlaybackSnapshot::Playing {
                media: crate::playback::MediaRef::Movie {
                    catalog_id: "tt1".to_string(),
                    title: "A Movie".to_string(),
                },
                title: "A Movie".to_string(),
                quality: "4K".to_string(),
                file_length: 0,
                player_name: "mpv".to_string(),
                subtitle_ready: false,
                has_next: false,
            });

        let err = service.set_active_profile(profile.id).await.unwrap_err();
        assert!(err.to_string().contains("Cannot switch profiles"));
        // The attempt must not have partially applied.
        assert_eq!(service.current_profile_id().await, None);
    }

    #[tokio::test]
    async fn deleting_the_active_profile_clears_it() {
        let (service, _dir) = test_service();
        let profile = service
            .create_profile(CreateProfileCommand {
                name: "Ada".to_string(),
                avatar_key: "star-violet".to_string(),
            })
            .await
            .unwrap();
        service
            .set_active_profile(profile.id.clone())
            .await
            .unwrap();

        service
            .delete_profile(ProfileIdCommand {
                profile_id: profile.id,
            })
            .await
            .unwrap();

        assert_eq!(service.current_profile_id().await, None);
    }

    #[tokio::test]
    async fn deleting_a_different_profile_leaves_the_active_one_alone() {
        let (service, _dir) = test_service();
        let active = service
            .create_profile(CreateProfileCommand {
                name: "Ada".to_string(),
                avatar_key: "star-violet".to_string(),
            })
            .await
            .unwrap();
        let other = service
            .create_profile(CreateProfileCommand {
                name: "Femi".to_string(),
                avatar_key: "bolt-amber".to_string(),
            })
            .await
            .unwrap();
        service.set_active_profile(active.id.clone()).await.unwrap();

        service
            .delete_profile(ProfileIdCommand {
                profile_id: other.id,
            })
            .await
            .unwrap();

        assert_eq!(service.current_profile_id().await, Some(active.id));
    }
}
