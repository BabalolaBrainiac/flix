use crate::config::Config;
use crate::desktop::types::{
    ActivationStatus, CatalogItemSummary, DownloadEntrySummary, DownloadsAction, DownloadsCommand,
    DownloadsResponse, EpisodeSummary, EpisodesCommand, EpisodesResponse, PlayerSummary,
    RedeemInviteCommand, RedeemInviteResponse, SearchCommand, SearchResponse, SettingsAction,
    SettingsCommand, SettingsResponse, StreamSummary, StreamsCommand, StreamsResponse,
};
use crate::library::Library;
use crate::player;
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
    let results = client.search(&command.query, command.anime_only).await;
    let items = results.items.iter().map(CatalogItemSummary::from).collect();
    Ok(SearchResponse {
        items,
        notes: results.notes,
    })
}

pub async fn handle_episodes(
    client: &StremioClient,
    command: &EpisodesCommand,
) -> Result<EpisodesResponse> {
    let kind = if command.is_anime {
        CatalogKind::AnimeKitsu
    } else {
        CatalogKind::Cinemeta
    };
    let item = CatalogItem {
        id: command.item_id.clone(),
        media_type: if command.is_anime {
            "anime".to_string()
        } else {
            "series".to_string()
        },
        name: String::new(),
        release_info: None,
        poster: None,
        kind,
    };
    let episodes = client.episodes(&item).await?;
    let summaries = episodes.iter().map(EpisodeSummary::from).collect();
    Ok(EpisodesResponse {
        episodes: summaries,
    })
}

pub async fn handle_streams(
    client: &StremioClient,
    coordinator: &crate::playback::PlaybackCoordinator,
    command: &StreamsCommand,
) -> Result<StreamsResponse> {
    let media_type = command.media_type.as_deref().unwrap_or("series");
    let streams = client.streams(media_type, &command.stream_id).await?;
    let registered = coordinator.register_stream_candidates(&streams).await?;
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
    Ok(StreamsResponse { streams: summaries })
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
            };
            let mut lib = library;
            lib.upsert(entry);
            lib.save()?;
            let entries = lib
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
    }
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
