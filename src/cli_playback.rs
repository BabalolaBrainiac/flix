use crate::episode_queue::EpisodeQueue;
use crate::media;
use crate::playback;
use crate::player::{self, ManagedPlayer, PlaybackOptions};
use crate::session::{Source, TorrentFile, TorrentId, TorrentSession};
use crate::stream_server;
use crate::stremio::{self, StremioClient, SubtitleContext};
use crate::subtitles;
use anyhow::{anyhow, Context, Result};
use crossterm::event::{Event, EventStream, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use futures_util::StreamExt;
use std::io::{self, IsTerminal, Write};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone)]
pub struct PlaybackSource {
    pub torrent: String,
    pub file_index: Option<usize>,
    pub quality: String,
}

pub async fn run(session: Arc<TorrentSession>, data_dir: &Path, torrent: &str) -> Result<()> {
    run_with_file_index(session, data_dir, torrent, None, None, Vec::new()).await
}

pub async fn run_with_file_index(
    session: Arc<TorrentSession>,
    data_dir: &Path,
    torrent: &str,
    preferred_file_index: Option<usize>,
    mut episode_queue: Option<EpisodeQueue>,
    alternatives: Vec<PlaybackSource>,
) -> Result<()> {
    let players = player::detect();
    let selected_player = players
        .first()
        .context(player::install::prompt_install_guidance()?)?;
    let server = stream_server::serve(session.clone()).await?;
    let mut active = match load_torrent(&session, torrent, preferred_file_index, true).await {
        Ok(active) => active,
        Err(primary_error) => {
            if alternatives.is_empty() {
                return Err(primary_error);
            }
            eprintln!("The preferred 4K torrent could not start: {primary_error}");
            load_alternatives(&session, &alternatives).await?
        }
    };
    sync_queue_with_video(&mut episode_queue, active.current());
    session
        .select_files(active.id, &[active.current().index])
        .await?;
    let context = current_subtitle_context(episode_queue.as_ref());
    let prepared =
        prepare_video(&session, data_dir, &active, &server, context.as_ref(), true).await?;
    let mut current_player = launch_prepared(selected_player, prepared)?;

    loop {
        let has_next =
            active.has_next_video() || episode_queue.as_ref().is_some_and(EpisodeQueue::has_next);
        match wait_for_player(&mut current_player, has_next).await? {
            PlaybackEvent::NextRequested => {
                match prepare_next(
                    &session,
                    data_dir,
                    &active,
                    episode_queue.as_ref(),
                    &server,
                    true,
                )
                .await
                {
                    Ok(next) => {
                        current_player.stop()?;
                        apply_next(&session, &mut active, &mut episode_queue, &next).await;
                        current_player = launch_prepared(selected_player, next.prepared)?;
                    }
                    Err(error) => {
                        eprintln!("Next episode preparation failed: {error}");
                        eprintln!("The current episode is still playing.");
                    }
                }
            }
            PlaybackEvent::QuitRequested => {
                current_player.stop()?;
                break;
            }
            PlaybackEvent::Exited => {
                current_player.wait()?;
                let next_player = loop {
                    let has_next = active.has_next_video()
                        || episode_queue.as_ref().is_some_and(EpisodeQueue::has_next);
                    match next_action(active.position, active.videos.len(), has_next)? {
                        Action::Next => match prepare_next(
                            &session,
                            data_dir,
                            &active,
                            episode_queue.as_ref(),
                            &server,
                            false,
                        )
                        .await
                        {
                            Ok(next) => {
                                apply_next(&session, &mut active, &mut episode_queue, &next).await;
                                break Some(launch_prepared(selected_player, next.prepared)?);
                            }
                            Err(error) => eprintln!("Next episode preparation failed: {error}"),
                        },
                        Action::Choose => {
                            active.position = choose_video(&active.videos)?;
                            sync_queue_with_video(&mut episode_queue, active.current());
                            session
                                .select_files(active.id, &[active.current().index])
                                .await?;
                            let context = current_subtitle_context(episode_queue.as_ref());
                            let prepared = prepare_video(
                                &session,
                                data_dir,
                                &active,
                                &server,
                                context.as_ref(),
                                true,
                            )
                            .await?;
                            break Some(launch_prepared(selected_player, prepared)?);
                        }
                        Action::Replay => {
                            session
                                .select_files(active.id, &[active.current().index])
                                .await?;
                            let context = current_subtitle_context(episode_queue.as_ref());
                            let prepared = prepare_video(
                                &session,
                                data_dir,
                                &active,
                                &server,
                                context.as_ref(),
                                false,
                            )
                            .await?;
                            break Some(launch_prepared(selected_player, prepared)?);
                        }
                        Action::Quit => break None,
                    }
                };
                match next_player {
                    Some(player) => current_player = player,
                    None => break,
                }
            }
        }
    }
    println!("Playback stopped. Flix cleaned up the active session.");
    Ok(())
}

#[derive(Clone)]
struct ActiveTorrent {
    id: TorrentId,
    videos: Vec<TorrentFile>,
    position: usize,
}

impl ActiveTorrent {
    fn current(&self) -> &TorrentFile {
        &self.videos[self.position]
    }

    fn has_next_video(&self) -> bool {
        self.position + 1 < self.videos.len()
    }
}

struct PreparedPlayback {
    title: String,
    url: String,
    subtitle_url: Option<String>,
    file_length: u64,
}

struct PreparedNext {
    active: ActiveTorrent,
    prepared: PreparedPlayback,
    advance_episode_queue: bool,
    remove_torrent: Option<TorrentId>,
}

async fn load_torrent(
    session: &TorrentSession,
    torrent: &str,
    preferred_file_index: Option<usize>,
    allow_choice: bool,
) -> Result<ActiveTorrent> {
    println!("Adding torrent...");
    let id = session.add(Source::parse(torrent)?).await?;
    let files = session.files(id)?;
    let videos: Vec<_> = media::ordered_videos(&files).into_iter().cloned().collect();
    if videos.is_empty() {
        return Err(anyhow!("Torrent contains no supported video file"));
    }
    session.select_files(id, &[]).await?;
    let position = preferred_file_index
        .and_then(|file_index| videos.iter().position(|video| video.index == file_index))
        .map(Ok)
        .unwrap_or_else(|| {
            if allow_choice {
                choose_video(&videos)
            } else {
                Ok(0)
            }
        })?;
    Ok(ActiveTorrent {
        id,
        videos,
        position,
    })
}

async fn load_alternatives(
    session: &TorrentSession,
    alternatives: &[PlaybackSource],
) -> Result<ActiveTorrent> {
    let mut last_error = None;
    for source in alternatives {
        if source.quality == "4K" {
            println!("Trying another native 4K stream...");
        } else {
            println!("Trying the best {} stream...", source.quality);
        }
        match load_torrent(session, &source.torrent, source.file_index, false).await {
            Ok(active) => return Ok(active),
            Err(error) => {
                eprintln!("The {} stream could not start: {error}", source.quality);
                last_error = Some(error);
            }
        }
    }
    match last_error {
        Some(error) => Err(error),
        None => Err(anyhow!("No automatic playback alternative was available")),
    }
}

async fn prepare_video(
    session: &TorrentSession,
    data_dir: &Path,
    active: &ActiveTorrent,
    server: &stream_server::ServerHandle,
    subtitle_context: Option<&SubtitleContext>,
    prompt_for_subtitle: bool,
) -> Result<PreparedPlayback> {
    let video = active.current();
    println!("Selected video: {}", video.name);
    println!("Preparing stream and English subtitles...");
    let preparation =
        playback::prepare_interactive(session, data_dir, active.id, video, subtitle_context).await;
    for note in &preparation.notes {
        eprintln!("{note}");
    }
    let selected_subtitle = if prompt_for_subtitle {
        choose_subtitle(&preparation.subtitle_results)?
    } else {
        preparation.subtitle_results.first()
    };
    let subtitle = if let Some(path) = preparation.cached_subtitle {
        Some(path)
    } else if let Some(result) = selected_subtitle {
        println!("Downloading the selected English subtitle...");
        match tokio::time::timeout(
            Duration::from_secs(25),
            subtitles::download_english_subtitle(
                data_dir,
                &session.info_hash(active.id)?,
                video.index,
                result,
            ),
        )
        .await
        {
            Ok(Ok(path)) => Some(path),
            Ok(Err(error)) => {
                eprintln!("English subtitle download failed: {error}");
                None
            }
            Err(_) => {
                eprintln!("English subtitle download timed out.");
                None
            }
        }
    } else {
        None
    };
    let subtitle_url = subtitle.map(|path| path.to_string_lossy().into_owned());
    if subtitle_url.is_some() {
        println!("English subtitles are ready.");
    }
    Ok(PreparedPlayback {
        title: video.name.clone(),
        url: stream_server::stream_url(server, active.id, video.index),
        subtitle_url,
        file_length: video.length,
    })
}

fn launch_prepared(
    selected_player: &player::Player,
    prepared: PreparedPlayback,
) -> Result<ManagedPlayer> {
    println!("Launching player at {}", selected_player.path.display());
    player::launch(
        selected_player,
        &prepared.url,
        &PlaybackOptions {
            title: prepared.title,
            subtitle_url: prepared.subtitle_url,
            start_at: None,
            ipc_socket: None,
            show_output: std::env::var_os("FLIX_PLAYER_LOGS").is_some(),
            file_length: prepared.file_length,
        },
    )
}

async fn prepare_next(
    session: &TorrentSession,
    data_dir: &Path,
    active: &ActiveTorrent,
    episode_queue: Option<&EpisodeQueue>,
    server: &stream_server::ServerHandle,
    current_is_playing: bool,
) -> Result<PreparedNext> {
    let next_episode = episode_queue.and_then(EpisodeQueue::next);
    let subtitle_context = episode_queue
        .and_then(|queue| next_episode.and_then(|episode| queue.subtitle_context(episode)));
    if active.has_next_video() {
        let mut next_active = active.clone();
        next_active.position += 1;
        let selected_files = if current_is_playing {
            vec![active.current().index, next_active.current().index]
        } else {
            vec![next_active.current().index]
        };
        session.select_files(active.id, &selected_files).await?;
        println!("Preparing the next file from this torrent pack...");
        let prepared = prepare_video(
            session,
            data_dir,
            &next_active,
            server,
            subtitle_context.as_ref(),
            false,
        )
        .await?;
        return Ok(PreparedNext {
            active: next_active,
            prepared,
            advance_episode_queue: next_episode.is_some(),
            remove_torrent: None,
        });
    }

    let episode = next_episode.context("There is no next episode")?;
    println!(
        "Searching 4K streams for S{:02}E{:02} {}...",
        episode.season,
        episode.episode,
        episode.title.as_deref().unwrap_or_default()
    );
    let client = StremioClient::from_env()?;
    let streams = client.streams("series", &episode.id).await?;
    let candidates = stremio::automatic_playback_candidates(&streams);
    let stream = candidates
        .first()
        .copied()
        .context("No torrent stream was found for the next episode")?;
    let quality = stremio::stream_quality_label(stream);
    if quality == "1080p" {
        println!("No native 4K stream is available. Using the best 1080p stream.");
    } else {
        println!("Selected next stream quality: {quality}.");
    }
    println!(
        "Next stream: {}",
        stream.file_name.as_deref().unwrap_or(&stream.name)
    );
    let magnet = stremio::build_magnet(&stream.info_hash, stream.file_name.as_deref())?;
    let alternatives = candidates
        .into_iter()
        .skip(1)
        .map(|stream| {
            Ok(PlaybackSource {
                torrent: stremio::build_magnet(&stream.info_hash, stream.file_name.as_deref())?,
                file_index: stream.file_index,
                quality: stremio::stream_quality_label(stream).to_string(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let next_active = match load_torrent(session, &magnet, stream.file_index, false).await {
        Ok(active) => active,
        Err(primary_error) => {
            if alternatives.is_empty() {
                return Err(primary_error);
            }
            eprintln!("The preferred 4K torrent could not start: {primary_error}");
            load_alternatives(session, &alternatives).await?
        }
    };
    session
        .select_files(next_active.id, &[next_active.current().index])
        .await?;
    let prepared = match prepare_video(
        session,
        data_dir,
        &next_active,
        server,
        subtitle_context.as_ref(),
        false,
    )
    .await
    {
        Ok(prepared) => prepared,
        Err(error) => {
            let _ = session.remove(next_active.id, true).await;
            return Err(error);
        }
    };
    Ok(PreparedNext {
        active: next_active,
        prepared,
        advance_episode_queue: true,
        remove_torrent: Some(active.id),
    })
}

async fn apply_next(
    session: &TorrentSession,
    active: &mut ActiveTorrent,
    episode_queue: &mut Option<EpisodeQueue>,
    next: &PreparedNext,
) {
    if next.advance_episode_queue {
        if let Some(queue) = episode_queue {
            queue.advance();
        }
    }
    if let Err(error) = session
        .select_files(next.active.id, &[next.active.current().index])
        .await
    {
        eprintln!("Could not narrow the next torrent file selection: {error}");
    }
    if let Some(id) = next.remove_torrent {
        if let Err(error) = session.remove(id, true).await {
            eprintln!("Old episode cleanup will finish when Flix exits: {error}");
        }
    }
    *active = next.active.clone();
}

async fn wait_for_player(player: &mut ManagedPlayer, has_next: bool) -> Result<PlaybackEvent> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        while !player.has_exited()? {
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        return Ok(PlaybackEvent::Exited);
    }
    if has_next {
        println!("Playback controls: focus this terminal and press [n] next or [q] stop.");
    } else {
        println!("Playback controls: focus this terminal and press [q] stop.");
    }
    let raw_mode = RawModeGuard::enter()?;
    let mut events = EventStream::new();
    let mut poll = tokio::time::interval(Duration::from_millis(200));
    poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let event = loop {
        tokio::select! {
            _ = poll.tick() => {
                if player.has_exited()? {
                    break PlaybackEvent::Exited;
                }
            }
            event = events.next() => {
                match event {
                    Some(Ok(Event::Key(key))) if key.kind == KeyEventKind::Press => {
                        if has_next && matches!(key.code, KeyCode::Char('n') | KeyCode::Char('N')) {
                            break PlaybackEvent::NextRequested;
                        }
                        if matches!(key.code, KeyCode::Char('q') | KeyCode::Char('Q'))
                            || (key.code == KeyCode::Char('c')
                                && key.modifiers.contains(KeyModifiers::CONTROL))
                        {
                            break PlaybackEvent::QuitRequested;
                        }
                    }
                    Some(Ok(_)) => {}
                    Some(Err(error)) => return Err(error.into()),
                    None => {}
                }
            }
        }
    };
    drop(raw_mode);
    Ok(event)
}

struct RawModeGuard;

impl RawModeGuard {
    fn enter() -> Result<Self> {
        enable_raw_mode()?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
    }
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

fn choose_subtitle(
    results: &[subtitles::SubtitleResult],
) -> Result<Option<&subtitles::SubtitleResult>> {
    if results.is_empty() {
        println!("No English subtitle result was found.");
        return Ok(None);
    }
    if !io::stdin().is_terminal() {
        return Ok(results.first());
    }
    println!("English subtitle results:");
    for (position, result) in results.iter().take(20).enumerate() {
        println!(
            "  {}. {} ({} downloads)",
            position + 1,
            result
                .file_name
                .as_deref()
                .or(result.release.as_deref())
                .unwrap_or("Unnamed subtitle"),
            result.download_count
        );
    }
    let count = results.len().min(20);
    loop {
        let input = read_line("Select an English subtitle [1], or [0] none: ")?;
        if input.is_empty() {
            return Ok(results.first());
        }
        if input == "0" {
            return Ok(None);
        }
        if let Ok(value) = input.parse::<usize>() {
            if (1..=count).contains(&value) {
                return Ok(results.get(value - 1));
            }
        }
        eprintln!("Enter a number from 0 to {count}.");
    }
}

fn choose_video(videos: &[TorrentFile]) -> Result<usize> {
    if videos.len() == 1 || !io::stdin().is_terminal() {
        return Ok(0);
    }
    print_video_list(videos);
    loop {
        let input = read_line("Select an episode or video [1]: ")?;
        if input.is_empty() {
            return Ok(0);
        }
        if let Ok(value) = input.parse::<usize>() {
            if (1..=videos.len()).contains(&value) {
                return Ok(value - 1);
            }
        }
        eprintln!("Enter a number from 1 to {}.", videos.len());
    }
}

fn print_video_list(videos: &[TorrentFile]) {
    println!("Available videos:");
    for (position, video) in videos.iter().enumerate() {
        println!(
            "  {}. {} ({:.1} MiB)",
            position + 1,
            video.name,
            video.length as f64 / 1_048_576.0
        );
    }
}

fn next_action(position: usize, count: usize, has_next: bool) -> Result<Action> {
    if !io::stdin().is_terminal() || (count == 1 && !has_next) {
        return Ok(Action::Quit);
    }
    loop {
        let prompt = if has_next {
            "Player exited. [Enter/n] next, [l] list, [r] replay, [q] quit: "
        } else {
            "Player exited. [l] list, [r] replay, [q/Enter] quit: "
        };
        match read_line(prompt)?.to_ascii_lowercase().as_str() {
            "" | "n" if has_next => return Ok(Action::Next),
            "" | "q" => return Ok(Action::Quit),
            "l" | "b" if count > 1 => return Ok(Action::Choose),
            "r" => return Ok(Action::Replay),
            _ => {
                let suffix = if position + 1 < count || has_next {
                    "next, list, replay, or quit"
                } else {
                    "list, replay, or quit"
                };
                eprintln!("Choose {suffix}.");
            }
        }
    }
}

fn read_line(prompt: &str) -> Result<String> {
    print!("{prompt}");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().to_string())
}

enum PlaybackEvent {
    Exited,
    NextRequested,
    QuitRequested,
}

enum Action {
    Next,
    Choose,
    Replay,
    Quit,
}
