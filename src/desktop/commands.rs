use crate::config::Config;
use crate::desktop::types::{
    CatalogItemSummary, DownloadEntrySummary, DownloadsAction, DownloadsCommand, DownloadsResponse,
    EpisodeSummary, EpisodesCommand, EpisodesResponse, PlayerSummary, SearchCommand,
    SearchResponse, SettingsAction, SettingsCommand, SettingsResponse, StreamSummary,
    StreamsCommand, StreamsResponse,
};
use crate::library::Library;
use crate::player;
use crate::stremio::{
    self, automatic_playback_candidates, stream_quality_label, CatalogItem, CatalogKind,
    StremioClient,
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
    command: &StreamsCommand,
) -> Result<StreamsResponse> {
    let streams = client
        .streams(&command.media_type, &command.stream_id)
        .await?;
    let recommended_candidates = automatic_playback_candidates(&streams);
    let recommended_first = recommended_candidates.first().copied();

    let mut summaries = Vec::new();
    for stream in &streams {
        let is_recommended = recommended_first.is_some_and(|first| std::ptr::eq(first, stream));
        let quality = stream_quality_label(stream).to_string();
        let magnet = stremio::build_magnet(&stream.info_hash, stream.file_name.as_deref())?;
        summaries.push(StreamSummary {
            info_hash: stream.info_hash.clone(),
            name: stream.name.clone(),
            file_name: stream.file_name.clone(),
            file_index: stream.file_index,
            quality,
            is_recommended,
            seeders: stream.seeders,
            size: stream.size.clone(),
            magnet,
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

pub fn handle_activation_status() -> crate::desktop::types::ActivationStatus {
    let token = crate::desktop::credentials::get_device_token().unwrap_or(None);
    let is_activated = token.is_some();
    let detected = player::detect();
    let vlc = detected
        .into_iter()
        .find(|p| p.kind == player::PlayerKind::Vlc);
    let vlc_installed = vlc.is_some();
    let vlc_path = vlc.map(|p| p.path.display().to_string());

    crate::desktop::types::ActivationStatus {
        is_activated,
        vlc_installed,
        vlc_path,
        gateway_url: std::env::var("FLIX_GATEWAY_URL").ok(),
    }
}

pub async fn handle_redeem_invite(
    cmd: &crate::desktop::types::RedeemInviteCommand,
) -> Result<crate::desktop::types::RedeemInviteResponse> {
    let raw_code = cmd.invite_code.trim();
    if raw_code.is_empty() {
        return Err(anyhow!("Invite code cannot be empty"));
    }

    let base_url = cmd
        .gateway_url
        .clone()
        .or_else(|| std::env::var("FLIX_GATEWAY_URL").ok())
        .unwrap_or_else(|| "http://127.0.0.1:8787".to_string());
    let base_url = base_url.trim_end_matches('/').to_string();

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    let redeem_url = format!("{}/v1/invites/redeem", base_url);
    let res = client
        .post(&redeem_url)
        .json(&serde_json::json!({ "invite_code": raw_code }))
        .send()
        .await
        .map_err(|e| anyhow!("Failed to connect to gateway: {}", e))?;

    if !res.status().is_success() {
        let status = res.status();
        let body_text = res.text().await.unwrap_or_default();
        return Err(anyhow!(
            "Gateway rejected invite (HTTP {}): {}",
            status,
            body_text
        ));
    }

    let payload: serde_json::Value = res
        .json()
        .await
        .map_err(|e| anyhow!("Invalid response from gateway: {}", e))?;

    let device_token = payload["device_token"]
        .as_str()
        .ok_or_else(|| anyhow!("Gateway response missing device_token"))?;

    crate::desktop::credentials::save_device_token(device_token)?;

    Ok(crate::desktop::types::RedeemInviteResponse {
        success: true,
        message: "Device activated and token securely stored in OS credential store.".to_string(),
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
    let activation = handle_activation_status();
    let detected_player = player::detect().into_iter().next();

    let vlc_status = if activation.vlc_installed {
        "Installed".to_string()
    } else {
        "Not detected".to_string()
    };

    Ok(crate::desktop::types::DiagnosticsReport {
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        is_activated: activation.is_activated,
        gateway_reachable: activation.gateway_url.is_some(),
        vlc_status,
        player_path: detected_player.map(|p| p.path.display().to_string()),
        download_dir: config.download_dir.display().to_string(),
        data_dir: config.data_dir.display().to_string(),
        active_session: active_playback,
    })
}
