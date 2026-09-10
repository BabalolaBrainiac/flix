use crate::config::Config;
use crate::desktop::types::{
    ActivationStatus, CatalogItemSummary, DownloadEntrySummary, DownloadsAction, DownloadsCommand,
    DownloadsResponse, EpisodeSummary, EpisodesCommand, EpisodesResponse, PlayerSummary,
    ReaderChaptersCommand, ReaderChaptersResponse, ReaderPagesCommand, ReaderPagesResponse,
    ReaderProgressResponse, ReaderProgressSaveCommand, ReaderSearchCommand, ReaderSearchResponse,
    RecommendCommand, RecommendItemSummary, RecommendResponse, RedeemInviteCommand,
    RedeemInviteResponse, SearchCommand, SearchResponse, SettingsAction, SettingsCommand,
    SettingsResponse, StreamSummary, StreamsCommand, StreamsResponse,
};
use crate::library::Library;
use crate::player;
use crate::reader::cache::PageCache;
use crate::reader::progress::{ProgressStore, ReadingPosition};
use crate::reader::source::ReaderClient;
use crate::reader::ReaderSource;
use crate::stremio::{
    automatic_playback_candidates, stream_quality_label, CatalogItem, CatalogKind, StremioClient,
};
use crate::subtitles;
use anyhow::{anyhow, Result};

pub async fn handle_search(
    client: &StremioClient,
    command: &SearchCommand,
) -> Result<SearchResponse> {
    if command.query.trim().is_empty() {
        return Err(anyhow!("Search query cannot be empty"));
    }
    if command.query.chars().count() > 200 {
        return Err(anyhow!("Search query is too long"));
    }
    let results = match command.catalog {
        Some(catalog) => client.search_catalog(command.query.trim(), catalog).await,
        None => {
            client
                .search(command.query.trim(), command.anime_only)
                .await
        }
    };
    let items = results.items.iter().map(CatalogItemSummary::from).collect();
    Ok(SearchResponse {
        items,
        notes: results.notes,
    })
}

pub async fn handle_recommend(
    client: &StremioClient,
    command: &RecommendCommand,
) -> Result<RecommendResponse> {
    use crate::recommend;

    let resolved = recommend::resolve_keywords(&command.keywords);

    let search_all = !command.show && !command.movie && !command.anime;
    let mut all_items = Vec::new();

    // Fetch Cinemeta catalogs
    if search_all || command.movie || command.show {
        let types: Vec<&str> = if search_all {
            vec!["movie", "series"]
        } else {
            let mut t = Vec::new();
            if command.movie {
                t.push("movie");
            }
            if command.show {
                t.push("series");
            }
            t
        };

        for media_type in types {
            for genre in &resolved.genres {
                let extra = format!("genre={genre}");
                if let Ok(items) = client
                    .discover(media_type, "top", &extra, CatalogKind::Cinemeta)
                    .await
                {
                    all_items.extend(items);
                }
            }
            if resolved.genres.is_empty() {
                if let Ok(items) = client
                    .discover(media_type, "top", "genre=Action", CatalogKind::Cinemeta)
                    .await
                {
                    all_items.extend(items);
                }
            }
        }
    }

    // Fetch anime
    if search_all || command.anime {
        match recommend::anime_catalog_for_genres(&resolved.genres) {
            recommend::AnimeCatalogStrategy::GenreFiltered { catalog_id, genre } => {
                let extra = format!("genre={genre}");
                if let Ok(items) = client
                    .discover("anime", &catalog_id, &extra, CatalogKind::AnimeKitsu)
                    .await
                {
                    all_items.extend(items);
                }
            }
            recommend::AnimeCatalogStrategy::TrendingWithLocalFilter => {
                if let Ok(items) = client
                    .discover(
                        "anime",
                        "kitsu-anime-trending",
                        "genre=Action",
                        CatalogKind::AnimeKitsu,
                    )
                    .await
                {
                    all_items.extend(items);
                }
            }
            recommend::AnimeCatalogStrategy::Popular => {
                if let Ok(items) = client
                    .discover(
                        "anime",
                        "kitsu-anime-popular",
                        "genre=Action",
                        CatalogKind::AnimeKitsu,
                    )
                    .await
                {
                    all_items.extend(items);
                }
            }
        }
    }

    let scored = recommend::score_items(all_items, &resolved);
    let filter = recommend::RecommendFilter {
        min_rating: command.min_rating,
        since_year: command.since_year,
        limit: command.limit,
    };
    let filtered = recommend::apply_filter(scored, &filter);

    let items = filtered
        .into_iter()
        .map(|scored| RecommendItemSummary {
            id: scored.item.id,
            name: scored.item.name,
            media_type: scored.item.media_type,
            release_info: scored.item.release_info,
            poster: scored.item.poster,
            genres: scored.item.genres,
            imdb_rating: scored.item.imdb_rating,
            description: scored.item.description,
            is_anime: scored.item.kind == CatalogKind::AnimeKitsu,
            score: scored.score,
        })
        .collect();

    Ok(RecommendResponse { items })
}

pub async fn handle_episodes(
    client: &StremioClient,
    command: &EpisodesCommand,
) -> Result<EpisodesResponse> {
    let kind = if command.item_id.starts_with("kitsu:") {
        CatalogKind::AnimeKitsu
    } else {
        CatalogKind::Cinemeta
    };
    let item = CatalogItem {
        id: command.item_id.clone(),
        media_type: if kind == CatalogKind::AnimeKitsu {
            "anime".to_string()
        } else {
            "series".to_string()
        },
        name: String::new(),
        release_info: None,
        poster: None,
        genres: None,
        imdb_rating: None,
        description: None,
        kind,
    };
    let episodes = client.episodes(&item).await?;
    let is_anime = command.is_anime || client.content_is_anime("series", &command.item_id).await?;
    let summaries = episodes.iter().map(EpisodeSummary::from).collect();
    Ok(EpisodesResponse {
        episodes: summaries,
        is_anime,
    })
}

pub async fn handle_streams(
    client: &StremioClient,
    coordinator: &crate::playback::PlaybackCoordinator,
    command: &StreamsCommand,
) -> Result<StreamsResponse> {
    let media_type = command.media_type.as_deref().unwrap_or("series");
    let is_anime = command.is_anime
        || client
            .content_is_anime(media_type, &command.stream_id)
            .await?;
    let streams = client
        .streams_for(media_type, &command.stream_id, is_anime)
        .await?;
    let registered = coordinator
        .register_stream_candidates_for(&streams, is_anime)
        .await?;
    let recommended_candidates = automatic_playback_candidates(&streams);
    let recommended_first = recommended_candidates.first().copied();

    let mut summaries = Vec::new();
    for (source_id, stream) in registered {
        let is_recommended = recommended_first
            .as_ref()
            .is_some_and(|first| first.info_hash == stream.info_hash);
        let quality = stream_quality_label(&stream).to_string();
        summaries.push(StreamSummary {
            source_id,
            name: stream.name.clone(),
            file_name: stream.file_name.clone(),
            file_index: stream.file_index,
            quality,
            is_recommended,
            seeders: stream.seeders,
            size: stream.size.clone(),
        });
    }
    Ok(StreamsResponse {
        streams: summaries,
        is_anime,
    })
}

pub async fn handle_downloads(
    config: &Config,
    command: &DownloadsCommand,
) -> Result<DownloadsResponse> {
    let library = Library::load(&config.data_dir)?;
    match &command.action {
        DownloadsAction::List => {
            let entries = library
                .entries()
                .iter()
                .map(|entry| DownloadEntrySummary {
                    info_hash: entry.info_hash.clone(),
                    display_name: entry.display_name.clone(),
                    title: entry.meta.as_ref().map(|meta| meta.title.clone()),
                    year: entry.meta.as_ref().and_then(|meta| meta.year),
                    file_size: None,
                })
                .collect();
            Ok(DownloadsResponse { entries })
        }
        DownloadsAction::Add {
            magnet,
            file_index: _,
        } => {
            let source = crate::session::Source::parse(magnet)?;
            let session = crate::session::TorrentSession::new(&config.download_dir).await?;
            let id = session.add(source.clone()).await?;
            let files = session.files(id)?;
            let metadata = crate::meta::lookup(magnet).await;
            let display_name = crate::media::select_default_video(&files)
                .map(|file| file.name.clone())
                .unwrap_or_else(|| metadata.title.clone());
            let entry = crate::library::Entry {
                info_hash: session.info_hash(id)?,
                display_name,
                source: crate::library::SourceSerde::from(&source),
                added_at: std::time::SystemTime::now(),
                last_played: None,
                meta: Some(metadata),
                output_name: crate::library::output_name_from_files(&files),
            };
            let mut lib = library;
            lib.upsert(entry);
            lib.save()?;
            let entries = summarize(&lib);
            Ok(DownloadsResponse { entries })
        }
        DownloadsAction::Remove { info_hash } => {
            let mut lib = library;
            if let Some(entry) = lib.remove(info_hash) {
                // Delete the download's data from disk. The output name is the
                // top-level file or folder the torrent wrote under the download
                // directory. Older entries have none, so only the record goes.
                if let Some(name) = entry.output_name.as_ref() {
                    let target = config.download_dir.join(name);
                    if target.is_dir() {
                        let _ = std::fs::remove_dir_all(&target);
                    } else if target.exists() {
                        let _ = std::fs::remove_file(&target);
                    }
                }
                lib.save()?;
            }
            Ok(DownloadsResponse {
                entries: summarize(&lib),
            })
        }
    }
}

fn summarize(library: &Library) -> Vec<DownloadEntrySummary> {
    library
        .entries()
        .iter()
        .map(|entry| DownloadEntrySummary {
            info_hash: entry.info_hash.clone(),
            display_name: entry.display_name.clone(),
            title: entry.meta.as_ref().map(|meta| meta.title.clone()),
            year: entry.meta.as_ref().and_then(|meta| meta.year),
            file_size: None,
        })
        .collect()
}

pub fn handle_settings(config: &Config, command: &SettingsCommand) -> Result<SettingsResponse> {
    match command.action {
        SettingsAction::Get => {
            let detected = player::detect();
            let players = detected
                .iter()
                .map(|item| PlayerSummary {
                    name: format!("{:?}", item.kind),
                    path: item.path.display().to_string(),
                    is_available: true,
                })
                .collect();
            Ok(SettingsResponse {
                download_dir: config.download_dir.display().to_string(),
                data_dir: config.data_dir.display().to_string(),
                players,
                opensubtitles_configured: subtitles::is_configured(),
            })
        }
    }
}

pub fn handle_activation_status() -> ActivationStatus {
    let is_activated = crate::desktop::credentials::get_device_token()
        .ok()
        .flatten()
        .is_some();

    ActivationStatus {
        is_activated,
        gateway_url: Some(crate::desktop::gateway::resolve_base_url()),
    }
}

pub async fn handle_redeem_invite(command: &RedeemInviteCommand) -> Result<RedeemInviteResponse> {
    let invite_code = command.invite_code.trim();
    if invite_code.is_empty() {
        return Err(anyhow!("Invite code cannot be empty"));
    }

    let gateway_url = match command
        .gateway_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(value) => crate::desktop::gateway::normalize_gateway_url(value)?,
        None => crate::desktop::gateway::resolve_base_url(),
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    let response = client
        .post(format!("{gateway_url}/v1/invites/redeem"))
        .json(&serde_json::json!({ "invite_code": invite_code }))
        .send()
        .await
        .map_err(|_| anyhow!("Failed to connect to the subtitle gateway"))?;

    if !response.status().is_success() {
        return Err(anyhow!(
            "The invite code was rejected by the subtitle gateway"
        ));
    }

    let payload: serde_json::Value = response
        .json()
        .await
        .map_err(|_| anyhow!("The subtitle gateway returned an invalid activation response"))?;
    let device_token = payload
        .get("device_token")
        .and_then(|value| value.as_str())
        .ok_or_else(|| anyhow!("The subtitle gateway did not return a device token"))?;

    crate::desktop::credentials::save_device_token(device_token)?;
    // The gateway URL must survive a restart, or later subtitle requests go to
    // the packaged default instead of the gateway that granted the token.
    crate::desktop::gateway::save_base_url(&gateway_url)?;

    Ok(RedeemInviteResponse {
        success: true,
        message: "Device activation is complete.".to_string(),
    })
}

pub fn handle_vlc_guidance() -> crate::desktop::types::VlcGuidance {
    let detected = player::detect();
    let vlc = detected
        .into_iter()
        .find(|p| p.kind == player::PlayerKind::Vlc);
    let is_installed = vlc.is_some();

    #[cfg(target_os = "windows")]
    {
        crate::desktop::types::VlcGuidance {
            platform: "windows".to_string(),
            is_installed,
            command: Some("winget install VideoLAN.VLC".to_string()),
            download_url: "https://www.videolan.org/vlc/download-windows.html".to_string(),
            instructions: vec![
                "Open PowerShell or Command Prompt and run: winget install VideoLAN.VLC"
                    .to_string(),
                "Or download and run the installer from videolan.org/vlc".to_string(),
                "Restart Flix Desktop after installation finishes.".to_string(),
            ],
        }
    }

    #[cfg(target_os = "macos")]
    {
        crate::desktop::types::VlcGuidance {
            platform: "macos".to_string(),
            is_installed,
            command: None,
            download_url: "https://www.videolan.org/vlc/download-macosx.html".to_string(),
            instructions: vec![
                "Download the VLC disk image (.dmg) from videolan.org/vlc".to_string(),
                "Open the downloaded DMG and drag the VLC icon into Applications".to_string(),
                "Launch Flix Desktop to begin watching.".to_string(),
            ],
        }
    }

    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        crate::desktop::types::VlcGuidance {
            platform: "linux".to_string(),
            is_installed,
            command: Some("sudo apt install vlc".to_string()),
            download_url: "https://www.videolan.org/vlc/".to_string(),
            instructions: vec![
                "Install VLC with your distribution package manager (e.g. sudo apt install vlc)"
                    .to_string(),
                "Launch Flix Desktop to begin watching.".to_string(),
            ],
        }
    }
}

pub fn handle_diagnostics(
    config: &Config,
    active_playback: bool,
) -> Result<crate::desktop::types::DiagnosticsReport> {
    let detected_player = player::detect().into_iter().next();

    let vlc_status = if detected_player
        .as_ref()
        .is_some_and(|p| p.kind == player::PlayerKind::Vlc)
    {
        "Installed".to_string()
    } else {
        "Not detected".to_string()
    };

    Ok(crate::desktop::types::DiagnosticsReport {
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        vlc_status,
        player_path: detected_player.map(|p| p.path.display().to_string()),
        download_dir: config.download_dir.display().to_string(),
        data_dir: config.data_dir.display().to_string(),
        active_session: active_playback,
    })
}

pub async fn handle_reader_search(command: &ReaderSearchCommand) -> Result<ReaderSearchResponse> {
    let client = ReaderClient::new(ReaderSource::MangaDex)?;
    let publications = client.search(&command.query, 20).await?;
    Ok(ReaderSearchResponse { publications })
}

pub async fn handle_reader_chapters(
    command: &ReaderChaptersCommand,
) -> Result<ReaderChaptersResponse> {
    let client = ReaderClient::new(ReaderSource::MangaDex)?;
    let (chapters, available_languages) = client.chapters(&command.manga_id, &command.lang).await?;
    Ok(ReaderChaptersResponse {
        chapters,
        available_languages,
    })
}

pub async fn handle_reader_pages(
    _config: &Config,
    command: &ReaderPagesCommand,
) -> Result<ReaderPagesResponse> {
    let client = ReaderClient::new(ReaderSource::MangaDex)?;
    let page_set = client.pages(&command.chapter_id).await?;
    let page_count = page_set.page_urls.len();
    // Return local proxy URLs instead of external URLs
    let pages: Vec<String> = (0..page_count)
        .map(|i| {
            format!(
                "/api/reader/page?chapter_id={}&index={i}&manga_id={}",
                command.chapter_id, command.manga_id
            )
        })
        .collect();
    Ok(ReaderPagesResponse {
        chapter_id: command.chapter_id.clone(),
        page_count,
        pages,
    })
}

pub async fn handle_reader_page_download(
    config: &Config,
    chapter_id: &str,
    manga_id: &str,
    index: usize,
) -> Result<(Vec<u8>, String)> {
    let client = ReaderClient::new(ReaderSource::MangaDex)?;
    let page_set = client.pages(chapter_id).await?;
    let url = page_set
        .page_urls
        .get(index)
        .ok_or_else(|| anyhow!("Page index {index} out of range"))?;

    // Check cache first
    let cache = PageCache::new(&config.data_dir);
    let source_name = ReaderSource::MangaDex.to_string();
    let ext = url
        .rsplit('.')
        .next()
        .filter(|e| e.len() <= 5 && e.chars().all(|c| c.is_ascii_alphanumeric()))
        .unwrap_or("jpg");
    let path = cache.page_path(&source_name, manga_id, chapter_id, index + 1, ext);

    if cache.has_page(&path) {
        let data = std::fs::read(&path)?;
        let mime = mime_guess::from_path(&path)
            .first_or_octet_stream()
            .to_string();
        return Ok((data, mime));
    }

    // Download and cache
    let data = client.download_page(url).await?;
    cache.store_page(&path, &data).await?;
    let mime = mime_guess::from_path(&path)
        .first_or_octet_stream()
        .to_string();
    Ok((data, mime))
}

pub fn handle_reader_progress_get(
    config: &Config,
    publication_id: &str,
) -> Result<ReaderProgressResponse> {
    let store = ProgressStore::load(&config.data_dir)?;
    Ok(ReaderProgressResponse {
        position: store.get(publication_id).cloned(),
    })
}

pub fn handle_reader_progress_save(
    config: &Config,
    command: &ReaderProgressSaveCommand,
) -> Result<()> {
    let mut store = ProgressStore::load(&config.data_dir)?;
    store.set(ReadingPosition {
        publication_id: command.publication_id.clone(),
        chapter_id: command.chapter_id.clone(),
        page: command.page,
        updated_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .to_string(),
    });
    store.save()
}
