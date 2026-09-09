use crate::cli_playback::{PlaybackSource, SourceCatalog};
use crate::episode_queue::EpisodeQueue;
use crate::stremio::{self, CatalogItem, CatalogKind, Episode, StremioClient, TorrentStream};
use anyhow::{anyhow, Context, Result};
use std::collections::BTreeMap;
use std::io::{self, IsTerminal, Write};

const MAX_LISTED: usize = 30;

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
    /// Every source found, so playback can change source without a new search.
    pub catalog: SourceCatalog,
}

/// One answer to a numbered prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    Selected(usize),
    Back,
    Quit,
}

/// The step of the search that the user is on.
enum Stage {
    Title,
    Episode,
    Stream,
    Action,
}

pub async fn select(
    query: &str,
    anime_only: bool,
    action_override: Option<SearchAction>,
) -> Result<Option<SearchSelection>> {
    if query.trim().is_empty() {
        return Err(anyhow!("Search text cannot be empty"));
    }
    println!("Searching Stremio catalogs for \"{}\"...", query.trim());
    let client = StremioClient::from_env()?;
    let result = client.search(query, anime_only).await;
    for note in result.notes {
        eprintln!("{note}");
    }
    if result.items.is_empty() {
        println!("No matching movie, series, or anime was found.");
        return Ok(None);
    }
    select_from_items(&result.items, &client, action_override).await
}

pub async fn select_from_items(
    items: &[CatalogItem],
    client: &StremioClient,
    action_override: Option<SearchAction>,
) -> Result<Option<SearchSelection>> {
    let mut stage = Stage::Title;
    let mut item_index = 0usize;
    let mut episodes: Vec<Episode> = Vec::new();
    let mut episode_index: Option<usize> = None;
    let mut active_season: Option<u32> = None;
    let mut streams: Vec<TorrentStream> = Vec::new();
    let mut stream_index = 0usize;

    loop {
        match stage {
            Stage::Title => match choose_catalog_item(items)? {
                Choice::Selected(index) => {
                    item_index = index;
                    episodes.clear();
                    episode_index = None;
                    active_season = None;
                    streams.clear();
                    stage = Stage::Episode;
                }
                Choice::Back | Choice::Quit => return Ok(None),
            },
            Stage::Episode => {
                if !is_series(&items[item_index]) {
                    episode_index = None;
                    stage = Stage::Stream;
                    continue;
                }
                if episodes.is_empty() {
                    println!("Loading episodes for {}...", items[item_index].name);
                    match client.episodes(&items[item_index]).await {
                        Ok(found) if found.is_empty() => {
                            eprintln!("The selected title has no episode metadata.");
                            stage = Stage::Title;
                            continue;
                        }
                        Ok(found) => episodes = found,
                        Err(error) => {
                            eprintln!("Episode lookup failed: {error}");
                            stage = Stage::Title;
                            continue;
                        }
                    }
                }
                let (choice, season) = choose_episode(&episodes, active_season)?;
                active_season = season;
                match choice {
                    Choice::Selected(index) => {
                        episode_index = Some(index);
                        streams.clear();
                        stage = Stage::Stream;
                    }
                    Choice::Back => stage = Stage::Title,
                    Choice::Quit => return Ok(None),
                }
            }
            Stage::Stream => {
                let previous = if is_series(&items[item_index]) {
                    Stage::Episode
                } else {
                    Stage::Title
                };
                if streams.is_empty() {
                    let (media_type, stream_id) = stream_target(
                        &items[item_index],
                        episode_index.map(|index| &episodes[index]),
                    );
                    println!("Searching torrent sources...");
                    match client
                        .streams_for(
                            media_type,
                            &stream_id,
                            items[item_index].kind == CatalogKind::AnimeKitsu,
                        )
                        .await
                    {
                        Ok(found) if found.is_empty() => {
                            eprintln!("No torrent source was found for the selection.");
                            stage = previous;
                            continue;
                        }
                        Ok(found) => streams = found,
                        Err(error) => {
                            eprintln!("Torrent source lookup failed: {error}");
                            stage = previous;
                            continue;
                        }
                    }
                }
                match choose_stream(&streams)? {
                    Choice::Selected(index) => {
                        stream_index = index;
                        stage = Stage::Action;
                    }
                    Choice::Back => stage = previous,
                    Choice::Quit => return Ok(None),
                }
            }
            Stage::Action => {
                let action = match action_override {
                    Some(action) => action,
                    None => match choose_action()? {
                        Some(action) => action,
                        None => {
                            stage = Stage::Stream;
                            continue;
                        }
                    },
                };
                let stream = &streams[stream_index];
                let magnet = stremio::build_magnet(&stream.info_hash, stream.file_name.as_deref())?;
                let episode_queue = match episode_index {
                    Some(index) => Some(EpisodeQueue::new(
                        items[item_index].clone(),
                        episodes.clone(),
                        &episodes[index].id,
                    )?),
                    None => None,
                };
                return Ok(Some(SearchSelection {
                    magnet,
                    file_index: stream.file_index,
                    action,
                    episode_queue,
                    catalog: build_catalog(&streams, stream_index)?,
                }));
            }
        }
    }
}

/// Keeps every source for later, plus the automatic retry order that Flix uses
/// when the chosen source does not start.
fn build_catalog(streams: &[TorrentStream], selected: usize) -> Result<SourceCatalog> {
    let sources = streams
        .iter()
        .take(MAX_LISTED)
        .map(PlaybackSource::from_stream)
        .collect::<Result<Vec<_>>>()?;
    let selected_source = sources
        .get(selected)
        .context("The selected source is not available")?;
    let fallbacks = sources
        .iter()
        .filter(|source| source.torrent != selected_source.torrent)
        .cloned()
        .collect();
    Ok(SourceCatalog { sources, fallbacks })
}

fn stream_target(item: &CatalogItem, episode: Option<&Episode>) -> (&'static str, String) {
    match episode {
        Some(episode) => ("series", episode.stream_id.clone()),
        None => ("movie", item.id.clone()),
    }
}

fn choose_catalog_item(items: &[CatalogItem]) -> Result<Choice> {
    let count = items.len().min(MAX_LISTED);
    println!("Search results ({count} shown):");
    for (position, item) in items.iter().take(count).enumerate() {
        let source = match item.kind {
            CatalogKind::Cinemeta => item.media_type.as_str(),
            CatalogKind::AnimeKitsu => "anime",
        };
        println!(
            "  {:>2}. {} ({}, {})",
            position + 1,
            item.name,
            source,
            item.release_info.as_deref().unwrap_or("year unknown")
        );
    }
    choose_index("Select a title [1], [q] quit: ", count, false)
}

/// Lets the caller reopen the episode list for a known season directly, so a
/// failed stream search does not send the user back to the season menu.
fn choose_episode(
    episodes: &[Episode],
    preferred_season: Option<u32>,
) -> Result<(Choice, Option<u32>)> {
    let mut seasons: BTreeMap<u32, usize> = BTreeMap::new();
    for episode in episodes {
        *seasons.entry(episode.season).or_default() += 1;
    }
    let season_values: Vec<_> = seasons.keys().copied().collect();
    if season_values.is_empty() {
        return Err(anyhow!("No selectable season is available"));
    }
    let mut current_season = preferred_season.filter(|season| season_values.contains(season));
    let mut skip_season_prompt = current_season.is_some();
    loop {
        let selected_season = if season_values.len() == 1 {
            season_values[0]
        } else if skip_season_prompt {
            skip_season_prompt = false;
            current_season.expect("skip_season_prompt is only set when current_season is set")
        } else {
            println!("Seasons:");
            for (position, season) in season_values.iter().enumerate() {
                println!(
                    "  {:>2}. Season {} ({} episodes)",
                    position + 1,
                    season,
                    seasons[season]
                );
            }
            match choose_index(
                "Select a season [1], [b] back, [q] quit: ",
                season_values.len(),
                true,
            )? {
                Choice::Selected(index) => season_values[index],
                other => return Ok((other, current_season)),
            }
        };
        current_season = Some(selected_season);
        let positions: Vec<usize> = episodes
            .iter()
            .enumerate()
            .filter(|(_, episode)| episode.season == selected_season)
            .map(|(index, _)| index)
            .collect();
        println!("Season {selected_season} episodes:");
        for (position, index) in positions.iter().enumerate() {
            println!(
                "  {:>2}. E{:02} {}",
                position + 1,
                episodes[*index].episode,
                episodes[*index].title.as_deref().unwrap_or("")
            );
        }
        match choose_index(
            "Select an episode [1], [b] back, [q] quit: ",
            positions.len(),
            true,
        )? {
            Choice::Selected(index) => {
                return Ok((Choice::Selected(positions[index]), current_season))
            }
            // With one season there is no season step to return to.
            Choice::Back if season_values.len() == 1 => return Ok((Choice::Back, current_season)),
            Choice::Back => continue,
            Choice::Quit => return Ok((Choice::Quit, current_season)),
        }
    }
}

fn choose_stream(streams: &[TorrentStream]) -> Result<Choice> {
    let count = streams.len().min(MAX_LISTED);
    println!("Torrent sources ({count} shown, best first):");
    for (position, stream) in streams.iter().take(count).enumerate() {
        let seeds = stream
            .seeders
            .map(|value| value.to_string())
            .unwrap_or_else(|| "unknown".to_string());
        println!(
            "  {:>2}. [{}] {} | {} | {} seeds{}",
            position + 1,
            stremio::stream_quality_label(stream),
            stream.file_name.as_deref().unwrap_or(&stream.name),
            stream.size.as_deref().unwrap_or("size unknown"),
            seeds,
            if position == 0 { " | recommended" } else { "" }
        );
    }
    println!("Flix keeps this list. During playback press [s] to change source.");
    choose_index("Select a source [1], [b] back, [q] quit: ", count, true)
}

/// Reads the play, download, or magnet choice. Returns `None` for back.
fn choose_action() -> Result<Option<SearchAction>> {
    if !io::stdin().is_terminal() {
        return Ok(Some(SearchAction::Play));
    }
    loop {
        match read_line("[Enter/p] play, [d] download, [m] print magnet, [b] back: ")?
            .to_ascii_lowercase()
            .as_str()
        {
            "" | "p" => return Ok(Some(SearchAction::Play)),
            "d" => return Ok(Some(SearchAction::Download)),
            "m" => return Ok(Some(SearchAction::Magnet)),
            "b" | "back" => return Ok(None),
            _ => eprintln!("Choose play, download, magnet, or back."),
        }
    }
}

fn choose_index(prompt: &str, count: usize, allow_back: bool) -> Result<Choice> {
    if count == 0 {
        return Err(anyhow!("No selectable result is available"));
    }
    if !io::stdin().is_terminal() {
        return Ok(Choice::Selected(0));
    }
    loop {
        let input = read_line(prompt)?;
        match parse_choice(&input, count, allow_back) {
            Some(choice) => return Ok(choice),
            None => {
                let extra = if allow_back { ", b to go back," } else { "," };
                eprintln!("Enter a number from 1 to {count}{extra} or q to quit.");
            }
        }
    }
}

/// Turns a prompt answer into a choice. Returns `None` when the answer is not
/// valid. An empty answer selects the first entry.
pub fn parse_choice(input: &str, count: usize, allow_back: bool) -> Option<Choice> {
    let input = input.trim().to_ascii_lowercase();
    if input.is_empty() {
        return (count > 0).then_some(Choice::Selected(0));
    }
    if input == "q" || input == "quit" {
        return Some(Choice::Quit);
    }
    if allow_back && (input == "b" || input == "back") {
        return Some(Choice::Back);
    }
    let value = input.parse::<usize>().ok()?;
    (1..=count)
        .contains(&value)
        .then(|| Choice::Selected(value - 1))
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

#[cfg(test)]
mod tests {
    use super::{build_catalog, TorrentStream};

    fn stream(number: u8, name: &str) -> TorrentStream {
        TorrentStream {
            info_hash: format!("{number:040x}"),
            file_index: None,
            file_name: Some(name.to_string()),
            name: name.to_string(),
            title: name.to_string(),
            seeders: Some(1),
            size: None,
        }
    }

    #[test]
    fn catalog_retries_other_listed_sources_when_the_first_source_fails() {
        let streams = vec![
            stream(1, "first 720p"),
            stream(2, "second 720p"),
            stream(3, "third 720p"),
        ];

        let catalog = build_catalog(&streams, 0).expect("catalog");
        let labels: Vec<_> = catalog
            .fallbacks
            .iter()
            .map(|source| source.label.as_str())
            .collect();

        assert_eq!(labels, vec!["second 720p", "third 720p"]);
    }
}
