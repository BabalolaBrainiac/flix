use crate::episode_queue::EpisodeQueue;
use crate::stremio::{CatalogItem, CatalogKind, Episode, StremioClient, TorrentStream};
use anyhow::{anyhow, Result};
use std::collections::BTreeMap;
use std::io::{self, IsTerminal, Write};

#[derive(Clone, Copy)]
pub enum SearchAction {
    Play,
    Download,
    Magnet,
}

pub struct SearchSelection {
    pub magnet: String,
    pub file_index: Option<usize>,
    pub action: SearchAction,
    pub episode_queue: Option<EpisodeQueue>,
    pub alternatives: Vec<crate::cli_playback::PlaybackSource>,
}

pub async fn select(
    query: &str,
    anime_only: bool,
    action_override: Option<SearchAction>,
) -> Result<Option<SearchSelection>> {
    if query.trim().is_empty() {
        return Err(anyhow!("Search text cannot be empty"));
    }
    println!("Searching Stremio catalogs...");
    let client = StremioClient::from_env()?;
    let result = client.search(query, anime_only).await;
    for note in result.notes {
        eprintln!("{note}");
    }
    if result.items.is_empty() {
        println!("No matching movie, series, or anime was found.");
        return Ok(None);
    }
    let item = choose_catalog_item(&result.items)?;
    let (media_type, stream_id, episode_queue) = if is_series(item) {
        let episodes = client.episodes(item).await?;
        if episodes.is_empty() {
            return Err(anyhow!("The selected title has no episode metadata"));
        }
        let episode = choose_episode(&episodes)?;
        let episode_id = episode.id.clone();
        let queue = EpisodeQueue::new(item.clone(), episodes, &episode_id)?;
        ("series", episode_id, Some(queue))
    } else {
        ("movie", item.id.clone(), None)
    };

    println!("Searching torrent streams...");
    let streams = client.streams(media_type, &stream_id).await?;
    if streams.is_empty() {
        println!("No torrent stream was found for the selected title.");
        return Ok(None);
    }
    let stream = choose_stream(&streams)?;
    let magnet = crate::stremio::build_magnet(&stream.info_hash, stream.file_name.as_deref())?;
    let alternatives = if std::ptr::eq(stream, &streams[0]) {
        crate::stremio::automatic_playback_candidates(&streams)
            .into_iter()
            .skip(1)
            .map(|stream| {
                Ok(crate::cli_playback::PlaybackSource {
                    torrent: crate::stremio::build_magnet(
                        &stream.info_hash,
                        stream.file_name.as_deref(),
                    )?,
                    file_index: stream.file_index,
                    quality: crate::stremio::stream_quality_label(stream).to_string(),
                })
            })
            .collect::<Result<Vec<_>>>()?
    } else {
        Vec::new()
    };
    let action = match action_override {
        Some(action) => action,
        None => choose_action()?,
    };
    Ok(Some(SearchSelection {
        magnet,
        file_index: stream.file_index,
        action,
        episode_queue,
        alternatives,
    }))
}

fn choose_catalog_item(items: &[CatalogItem]) -> Result<&CatalogItem> {
    println!("Search results:");
    for (position, item) in items.iter().take(30).enumerate() {
        let source = match item.kind {
            CatalogKind::Cinemeta => item.media_type.as_str(),
            CatalogKind::AnimeKitsu => "anime",
        };
        println!(
            "  {}. {} ({}, {})",
            position + 1,
            item.name,
            source,
            item.release_info.as_deref().unwrap_or("year unknown")
        );
    }
    let index = choose_index("Select a title [1]: ", items.len().min(30))?;
    Ok(&items[index])
}

fn choose_episode(episodes: &[Episode]) -> Result<&Episode> {
    let mut seasons: BTreeMap<u32, usize> = BTreeMap::new();
    for episode in episodes {
        *seasons.entry(episode.season).or_default() += 1;
    }
    let selected_season = if seasons.len() == 1 {
        seasons
            .keys()
            .next()
            .copied()
            .ok_or_else(|| anyhow!("No selectable season is available"))?
    } else {
        let season_values: Vec<_> = seasons.keys().copied().collect();
        println!("Seasons:");
        for (position, season) in season_values.iter().enumerate() {
            println!(
                "  {}. Season {} ({} episodes)",
                position + 1,
                season,
                seasons[season]
            );
        }
        season_values[choose_index("Select a season [1]: ", season_values.len())?]
    };
    let choices: Vec<_> = episodes
        .iter()
        .filter(|episode| episode.season == selected_season)
        .collect();
    println!("Episodes:");
    for (position, episode) in choices.iter().enumerate() {
        println!(
            "  {}. E{:02} {}",
            position + 1,
            episode.episode,
            episode.title.as_deref().unwrap_or("")
        );
    }
    Ok(choices[choose_index("Select an episode [1]: ", choices.len())?])
}

fn choose_stream(streams: &[TorrentStream]) -> Result<&TorrentStream> {
    let count = streams.len().min(30);
    println!("Torrent results:");
    for (position, stream) in streams.iter().take(count).enumerate() {
        let seeds = stream
            .seeders
            .map(|value| value.to_string())
            .unwrap_or_else(|| "unknown".to_string());
        let default = if position == 0 {
            format!(
                " | default: {}",
                crate::stremio::stream_quality_label(stream)
            )
        } else {
            String::new()
        };
        println!(
            "  {}. {} | {} | {} seeds{}",
            position + 1,
            stream.file_name.as_deref().unwrap_or(&stream.name),
            stream.size.as_deref().unwrap_or("size unknown"),
            seeds,
            default
        );
    }
    Ok(&streams[choose_index("Select a torrent [1]: ", count)?])
}

fn choose_action() -> Result<SearchAction> {
    if !io::stdin().is_terminal() {
        return Ok(SearchAction::Play);
    }
    loop {
        match read_line("[Enter/p] play, [d] download, [m] print magnet: ")?
            .to_ascii_lowercase()
            .as_str()
        {
            "" | "p" => return Ok(SearchAction::Play),
            "d" => return Ok(SearchAction::Download),
            "m" => return Ok(SearchAction::Magnet),
            _ => eprintln!("Choose play, download, or magnet."),
        }
    }
}

fn choose_index(prompt: &str, count: usize) -> Result<usize> {
    if count == 0 {
        return Err(anyhow!("No selectable result is available"));
    }
    if !io::stdin().is_terminal() {
        return Ok(0);
    }
    loop {
        let input = read_line(prompt)?;
        if input.is_empty() {
            return Ok(0);
        }
        if let Ok(value) = input.parse::<usize>() {
            if (1..=count).contains(&value) {
                return Ok(value - 1);
            }
        }
        eprintln!("Enter a number from 1 to {count}.");
    }
}

fn read_line(prompt: &str) -> Result<String> {
    print!("{prompt}");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().to_string())
}

fn is_series(item: &CatalogItem) -> bool {
    item.kind == CatalogKind::AnimeKitsu || item.media_type == "series"
}
