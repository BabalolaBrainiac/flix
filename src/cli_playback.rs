use crate::episode_queue::EpisodeQueue;
use crate::media;
use crate::playback;
use crate::player::{self, ManagedPlayer, PlaybackOptions};
use crate::session::{Source, TorrentFile, TorrentId, TorrentSession};
use crate::stream_server;
use crate::stremio::{self, StremioClient, SubtitleContext};
use crate::subtitles;
use anyhow::{anyhow, Context, Result};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

// A player that closes this quickly almost always failed to open the stream.
const QUICK_EXIT: Duration = Duration::from_secs(25);
// Downloading more than this wastes the daily OpenSubtitles quota.
const MAX_SUBTITLE_DOWNLOADS: usize = 3;
const SUBTITLE_TIMEOUT: Duration = Duration::from_secs(25);

#[derive(Clone)]
pub struct PlaybackSource {
    pub torrent: String,
    pub file_index: Option<usize>,
    pub quality: String,
    /// Release name shown when the user picks another source.
    pub label: String,
    pub size: Option<String>,
    pub seeders: Option<u64>,
}

impl PlaybackSource {
    /// Builds a source from a Stremio torrent stream.
    pub fn from_stream(stream: &stremio::TorrentStream) -> Result<Self> {
        Ok(Self {
            torrent: stremio::build_magnet(&stream.info_hash, stream.file_name.as_deref())?,
            file_index: stream.file_index,
            quality: stremio::stream_quality_label(stream).to_string(),
            label: stream
                .file_name
                .clone()
                .unwrap_or_else(|| stream.name.clone()),
            size: stream.size.clone(),
            seeders: stream.seeders,
        })
    }
}

/// Every torrent source found for the title that is playing, so that the user
/// can change source without running the search again.
#[derive(Clone, Default)]
pub struct SourceCatalog {
    /// All sources in the order the search showed them.
    pub sources: Vec<PlaybackSource>,
    /// Sources that Flix tries on its own when the selected source fails.
    pub fallbacks: Vec<PlaybackSource>,
}

impl SourceCatalog {
    pub fn is_switchable(&self) -> bool {
        self.sources.len() > 1
    }
}

pub async fn run(session: Arc<TorrentSession>, data_dir: &Path, torrent: &str) -> Result<()> {
    run_with_file_index(
        session,
        data_dir,
        torrent,
        None,
        None,
        SourceCatalog::default(),
    )
    .await
}

pub async fn run_with_file_index(
    session: Arc<TorrentSession>,
    data_dir: &Path,
    torrent: &str,
    preferred_file_index: Option<usize>,
    mut episode_queue: Option<EpisodeQueue>,
    catalog: SourceCatalog,
) -> Result<()> {
    let players = player::detect();
    let selected_player = players
        .first()
        .context(player::install::prompt_install_guidance()?)?;
    let server = stream_server::serve(session.clone()).await?;
    let mut active = match load_torrent(&session, torrent, preferred_file_index, true).await {
        Ok(active) => active,
        Err(primary_error) => {
            if catalog.fallbacks.is_empty() {
                return Err(primary_error);
            }
            eprintln!("The preferred 4K torrent could not start: {primary_error}");
            load_alternatives(&session, &catalog.fallbacks).await?
        }
    };
    sync_queue_with_video(&mut episode_queue, active.current());
    let context = current_subtitle_context(episode_queue.as_ref());
    let mut prepared =
        prepare_video(&session, data_dir, &active, &server, context.as_ref(), true).await?;
    if !prepared.stream_ready {
        for source in &catalog.fallbacks {
            println!("The selected source did not warm. Trying: {}", source.label);
            let next_active =
                match load_torrent(&session, &source.torrent, source.file_index, false).await {
                    Ok(next_active) => next_active,
                    Err(error) => {
                        eprintln!("The alternative source could not start: {error}");
                        continue;
                    }
                };
            let next_prepared = prepare_video(
                &session,
                data_dir,
                &next_active,
                &server,
                context.as_ref(),
                false,
            )
            .await?;
            if !next_prepared.stream_ready {
                let _ = session.remove(next_active.id, true).await;
                continue;
            }
            if let Err(error) = session.remove(active.id, true).await {
                eprintln!("Old source cleanup will finish when Flix exits: {error}");
            }
            active = next_active;
            prepared = next_prepared;
            break;
        }
    }
    if !prepared.stream_ready {
        return Err(anyhow!(
            "No listed source warmed in time. Select a different episode or try again later."
        ));
    }
    let mut current = launch_prepared(selected_player, prepared)?;

    loop {
        let has_next =
            active.has_next_video() || episode_queue.as_ref().is_some_and(EpisodeQueue::has_next);
        match wait_for_player(&mut current.player, has_next, catalog.is_switchable()).await? {
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
                        current.player.stop()?;
                        apply_next(&session, &mut active, &mut episode_queue, &next).await;
                        current = launch_prepared(selected_player, next.prepared)?;
                    }
                    Err(error) => {
                        eprintln!("Next episode preparation failed: {error}");
                        eprintln!("The current episode is still playing.");
                    }
                }
            }
            PlaybackEvent::SourceChangeRequested => {
                match prepare_source_change(
                    &session,
                    data_dir,
                    &server,
                    &mut episode_queue,
                    &catalog,
                )
                .await
                {
                    Ok(Some(switch)) => {
                        current.player.stop()?;
                        apply_source_change(&session, &mut active, switch.active).await;
                        current = launch_prepared(selected_player, switch.prepared)?;
                    }
                    Ok(None) => println!("The current source is still playing."),
                    Err(error) => {
                        eprintln!("The other source could not start: {error}");
                        eprintln!("The current source is still playing.");
                    }
                }
            }
            PlaybackEvent::QuitRequested => {
                current.player.stop()?;
                break;
            }
            PlaybackEvent::Exited => {
                current.player.wait()?;
                if current.started_at.elapsed() < QUICK_EXIT && catalog.is_switchable() {
                    println!(
                        "The player closed after a few seconds. This source may be broken. Press [s] to pick another source."
                    );
                }
                let next_player = loop {
                    let has_next = active.has_next_video()
                        || episode_queue.as_ref().is_some_and(EpisodeQueue::has_next);
                    match next_action(
                        active.position,
                        active.videos.len(),
                        has_next,
                        catalog.is_switchable(),
                    )? {
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
                        Action::ChangeSource => match prepare_source_change(
                            &session,
                            data_dir,
                            &server,
                            &mut episode_queue,
                            &catalog,
                        )
                        .await
                        {
                            Ok(Some(switch)) => {
                                apply_source_change(&session, &mut active, switch.active).await;
                                break Some(launch_prepared(selected_player, switch.prepared)?);
                            }
                            Ok(None) => {}
                            Err(error) => eprintln!("The other source could not start: {error}"),
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
                    Some(player) => current = player,
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
    selected_tracks: Option<playback::anime::SelectedTracks>,
    title: String,
    url: String,
    subtitle_files: Vec<String>,
    file_length: u64,
    stream_ready: bool,
}

struct PreparedNext {
    active: ActiveTorrent,
    prepared: PreparedPlayback,
    advance_episode_queue: bool,
    remove_torrent: Option<TorrentId>,
}

struct PreparedSwitch {
    active: ActiveTorrent,
    prepared: PreparedPlayback,
}

struct RunningPlayer {
    player: ManagedPlayer,
    started_at: Instant,
}

async fn load_torrent(
    session: &TorrentSession,
    torrent: &str,
    preferred_file_index: Option<usize>,
    allow_choice: bool,
) -> Result<ActiveTorrent> {
    println!("Adding torrent...");
    let source = Source::parse(torrent)?;
    // Select the file at add time when the catalog gives its index. That keeps
    // the torrent from a second file selection on a live torrent, which can
    // deadlock and which drops the peers that connect during the change.
    let (id, preselected) = match preferred_file_index {
        Some(file_index) => session.add_with_file(source, file_index).await?,
        None => (session.add(source).await?, false),
    };
    let files = session.files(id)?;
    let videos: Vec<_> = media::ordered_videos(&files).into_iter().cloned().collect();
    if videos.is_empty() {
        return Err(anyhow!("Torrent contains no supported video file"));
    }
    if !preselected {
        // Nothing is selected yet, so stop a download of the complete torrent
        // while the user chooses a video.
        session.select_files(id, &[]).await?;
    }
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
    let target = videos[position].index;
    if crate::session::needs_file_selection(preferred_file_index, preselected, target) {
        session.select_files(id, &[target]).await?;
    }
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
    let mut subtitle_files = preparation.cached_subtitles.clone();
    if subtitle_files.is_empty() {
        let selected = if prompt_for_subtitle {
            choose_subtitles(&preparation.subtitle_results)?
        } else {
            preparation
                .subtitle_results
                .first()
                .into_iter()
                .collect::<Vec<_>>()
        };
        subtitle_files =
            download_subtitles(session, data_dir, active.id, video.index, &selected).await;
    }
    report_subtitles(&subtitle_files);
    let mut stream_ready = preparation.stream_ready;
    let selected_tracks = if subtitle_context.is_some() && stream_ready {
        let (reader, _) = session.open_stream(active.id, video.index)?;
        match tokio::time::timeout(
            Duration::from_secs(3),
            playback::anime::inspect(reader, !subtitle_files.is_empty()),
        )
        .await
        {
            Ok(Ok(tracks)) => Some(tracks),
            result => {
                eprintln!("Anime language check failed: {result:?}");
                stream_ready = false;
                None
            }
        }
    } else {
        None
    };
    Ok(PreparedPlayback {
        selected_tracks,
        title: video.name.clone(),
        url: stream_server::stream_url(server, active.id, video.index),
        subtitle_files: subtitle_files
            .iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect(),
        file_length: video.length,
        stream_ready,
    })
}

/// Downloads every chosen subtitle before playback starts, so that the player
/// lists all of them as selectable tracks.
async fn download_subtitles(
    session: &TorrentSession,
    data_dir: &Path,
    id: TorrentId,
    file_index: usize,
    selected: &[&subtitles::SubtitleResult],
) -> Vec<PathBuf> {
    if selected.is_empty() {
        return Vec::new();
    }
    let info_hash = match session.info_hash(id) {
        Ok(hash) => hash,
        Err(error) => {
            eprintln!("English subtitle download failed: {error}");
            return Vec::new();
        }
    };
    let mut paths = Vec::new();
    for result in selected.iter().take(MAX_SUBTITLE_DOWNLOADS) {
        println!("Downloading English subtitle: {}", subtitle_label(result));
        match tokio::time::timeout(
            SUBTITLE_TIMEOUT,
            subtitles::download_english_subtitle(data_dir, &info_hash, file_index, result),
        )
        .await
        {
            Ok(Ok(path)) => {
                if !paths.contains(&path) {
                    paths.push(path);
                }
            }
            Ok(Err(error)) => eprintln!("English subtitle download failed: {error}"),
            Err(_) => eprintln!("English subtitle download timed out."),
        }
    }
    paths
}

fn report_subtitles(subtitle_files: &[PathBuf]) {
    match subtitle_files.len() {
        0 => {}
        1 => println!("1 English subtitle track is ready in the player."),
        count => println!("{count} English subtitle tracks are ready in the player."),
    }
}

fn launch_prepared(
    selected_player: &player::Player,
    prepared: PreparedPlayback,
) -> Result<RunningPlayer> {
    if !prepared.stream_ready {
        return Err(anyhow!(
            "The source is not ready for playback. Select another source."
        ));
    }
    println!("Launching player at {}", selected_player.path.display());
    let options = PlaybackOptions::new(prepared.title, prepared.file_length)
        .with_selected_tracks(prepared.selected_tracks)
        .with_subtitles(prepared.subtitle_files)
        .with_show_output(std::env::var_os("FLIX_PLAYER_LOGS").is_some());
    let player = player::launch(selected_player, &prepared.url, &options)?;
    Ok(RunningPlayer {
        player,
        started_at: Instant::now(),
    })
}

/// Asks the user for another source of the same title and prepares it. Returns
/// `Ok(None)` when the user cancels.
async fn prepare_source_change(
    session: &TorrentSession,
    data_dir: &Path,
    server: &stream_server::ServerHandle,
    episode_queue: &mut Option<EpisodeQueue>,
    catalog: &SourceCatalog,
) -> Result<Option<PreparedSwitch>> {
    let Some(source) = choose_source(&catalog.sources)? else {
        return Ok(None);
    };
    println!("Switching to: {}", source.label);
    let context = current_subtitle_context(episode_queue.as_ref());
    for candidate in sources_for_change(catalog, source) {
        if candidate.torrent != source.torrent {
            println!(
                "The selected source did not warm. Trying: {}",
                candidate.label
            );
        }

        let next_active =
            match load_torrent(session, &candidate.torrent, candidate.file_index, true).await {
                Ok(next_active) => next_active,
                Err(error) => {
                    eprintln!("The alternative source could not start: {error}");
                    continue;
                }
            };
        let prepared = match prepare_video(
            session,
            data_dir,
            &next_active,
            server,
            context.as_ref(),
            false,
        )
        .await
        {
            Ok(prepared) => prepared,
            Err(error) => {
                eprintln!("The alternative source could not prepare: {error}");
                let _ = session.remove(next_active.id, true).await;
                continue;
            }
        };
        if !prepared.stream_ready {
            let _ = session.remove(next_active.id, true).await;
            continue;
        }

        sync_queue_with_video(episode_queue, next_active.current());
        return Ok(Some(PreparedSwitch {
            active: next_active,
            prepared,
        }));
    }

    Err(anyhow!(
        "No selected source warmed in time. Select another source."
    ))
}

fn sources_for_change<'a>(
    catalog: &'a SourceCatalog,
    selected: &'a PlaybackSource,
) -> Vec<&'a PlaybackSource> {
    let mut candidates = vec![selected];
    candidates.extend(
        catalog
            .sources
            .iter()
            .filter(|candidate| candidate.torrent != selected.torrent),
    );
    candidates
}

async fn apply_source_change(
    session: &TorrentSession,
    active: &mut ActiveTorrent,
    next_active: ActiveTorrent,
) {
    if next_active.id != active.id {
        if let Err(error) = session.remove(active.id, true).await {
            eprintln!("Old source cleanup will finish when Flix exits: {error}");
        }
    }
    *active = next_active;
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
    let streams = client
        .streams_for("series", &episode.stream_id, subtitle_context.is_some())
        .await?;
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
        .map(PlaybackSource::from_stream)
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

async fn wait_for_player(
    player: &mut ManagedPlayer,
    has_next: bool,
    can_change_source: bool,
) -> Result<PlaybackEvent> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        while !player.has_exited()? {
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        return Ok(PlaybackEvent::Exited);
    }
    let mut keys = Vec::new();
    if has_next {
        keys.push("[n] next");
    }
    if can_change_source {
        keys.push("[s] change source");
    }
    keys.push("[q] stop");
    println!(
        "Playback controls: focus this terminal and press {}.",
        keys.join(", ")
    );
    let raw_mode = RawModeGuard::enter()?;
    let mut input_available = true;
    let mut poll = tokio::time::interval(Duration::from_millis(200));
    poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let event = loop {
        tokio::select! {
            _ = poll.tick() => {
                if player.has_exited()? {
                    break PlaybackEvent::Exited;
                }
                if input_available {
                    match event::poll(Duration::ZERO) {
                        Ok(true) => match event::read() {
                            Ok(Event::Key(key)) if key.kind == KeyEventKind::Press => {
                        if has_next && matches!(key.code, KeyCode::Char('n') | KeyCode::Char('N')) {
                            break PlaybackEvent::NextRequested;
                        }
                        if can_change_source
                            && matches!(key.code, KeyCode::Char('s') | KeyCode::Char('S'))
                        {
                            break PlaybackEvent::SourceChangeRequested;
                        }
                        if matches!(key.code, KeyCode::Char('q') | KeyCode::Char('Q'))
                            || (key.code == KeyCode::Char('c')
                                && key.modifiers.contains(KeyModifiers::CONTROL))
                        {
                            break PlaybackEvent::QuitRequested;
                        }
                    }
                            Ok(_) => {}
                            Err(error) => {
                                eprintln!("Playback controls are unavailable: {error}");
                                input_available = false;
                            }
                        },
                        Ok(false) => {}
                        Err(error) => {
                            eprintln!("Playback controls are unavailable: {error}");
                            input_available = false;
                        }
                    }
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

fn subtitle_label(result: &subtitles::SubtitleResult) -> &str {
    result
        .file_name
        .as_deref()
        .or(result.release.as_deref())
        .unwrap_or("Unnamed subtitle")
}

/// Reads a subtitle choice. The user can name several subtitles so that the
/// player receives more than one selectable track.
fn choose_subtitles(
    results: &[subtitles::SubtitleResult],
) -> Result<Vec<&subtitles::SubtitleResult>> {
    if results.is_empty() {
        println!("No English subtitle result was found.");
        return Ok(Vec::new());
    }
    if !io::stdin().is_terminal() {
        return Ok(results.iter().take(1).collect());
    }
    let count = results.len().min(20);
    println!("English subtitle results:");
    for (position, result) in results.iter().take(count).enumerate() {
        println!(
            "  {}. {} ({} downloads)",
            position + 1,
            subtitle_label(result),
            result.download_count
        );
    }
    println!(
        "Flix downloads at most {MAX_SUBTITLE_DOWNLOADS} subtitles and gives each one to the player as a track."
    );
    loop {
        let input =
            read_line("Select English subtitles [1], several as 1,2, [a] top results, [0] none: ")?;
        match parse_subtitle_selection(&input, count) {
            Some(indexes) => {
                return Ok(indexes.into_iter().filter_map(|i| results.get(i)).collect())
            }
            None => eprintln!("Enter numbers from 1 to {count}, or 0 for none."),
        }
    }
}

/// Turns a subtitle prompt answer into zero-based result positions. Returns
/// `None` when the answer is not valid.
pub fn parse_subtitle_selection(input: &str, count: usize) -> Option<Vec<usize>> {
    let input = input.trim();
    if count == 0 {
        return Some(Vec::new());
    }
    if input.is_empty() {
        return Some(vec![0]);
    }
    let lower = input.to_ascii_lowercase();
    if lower == "0" || lower == "n" || lower == "none" {
        return Some(Vec::new());
    }
    if lower == "a" || lower == "all" {
        return Some((0..count.min(MAX_SUBTITLE_DOWNLOADS)).collect());
    }
    let mut indexes = Vec::new();
    for part in lower.split([',', ' ']).filter(|part| !part.is_empty()) {
        let value = part.parse::<usize>().ok()?;
        if !(1..=count).contains(&value) {
            return None;
        }
        if !indexes.contains(&(value - 1)) {
            indexes.push(value - 1);
        }
    }
    (!indexes.is_empty()).then_some(indexes)
}

/// Shows every source found for the title and returns the chosen one, or
/// `None` when the user cancels.
fn choose_source(sources: &[PlaybackSource]) -> Result<Option<&PlaybackSource>> {
    if sources.is_empty() {
        println!("Flix kept no other source for this title.");
        return Ok(None);
    }
    if !io::stdin().is_terminal() {
        return Ok(None);
    }
    let count = sources.len().min(30);
    println!("Sources for this title:");
    for (position, source) in sources.iter().take(count).enumerate() {
        println!(
            "  {}. [{}] {} | {} | {} seeds",
            position + 1,
            source.quality,
            source.label,
            source.size.as_deref().unwrap_or("size unknown"),
            source
                .seeders
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unknown".to_string())
        );
    }
    loop {
        let input = read_line("Select another source, or [0] to cancel: ")?;
        if input.is_empty() || input == "0" {
            return Ok(None);
        }
        if let Ok(value) = input.parse::<usize>() {
            if (1..=count).contains(&value) {
                return Ok(sources.get(value - 1));
            }
        }
        eprintln!("Enter a number from 1 to {count}, or 0 to cancel.");
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

fn next_action(
    position: usize,
    count: usize,
    has_next: bool,
    can_change_source: bool,
) -> Result<Action> {
    if !io::stdin().is_terminal() || (count == 1 && !has_next && !can_change_source) {
        return Ok(Action::Quit);
    }
    loop {
        let mut choices = Vec::new();
        if has_next {
            choices.push("[Enter/n] next");
        }
        if can_change_source {
            choices.push("[s] source");
        }
        if count > 1 {
            choices.push("[l] list");
        }
        choices.push("[r] replay");
        choices.push(if has_next {
            "[q] quit"
        } else {
            "[q/Enter] quit"
        });
        let prompt = format!("Player exited. {}: ", choices.join(", "));
        match read_line(&prompt)?.to_ascii_lowercase().as_str() {
            "" | "n" if has_next => return Ok(Action::Next),
            "" | "q" => return Ok(Action::Quit),
            "s" if can_change_source => return Ok(Action::ChangeSource),
            "l" | "b" if count > 1 => return Ok(Action::Choose),
            "r" => return Ok(Action::Replay),
            _ => {
                let suffix = if position + 1 < count || has_next {
                    "next, source, list, replay, or quit"
                } else {
                    "source, list, replay, or quit"
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
    SourceChangeRequested,
    QuitRequested,
}

enum Action {
    Next,
    ChangeSource,
    Choose,
    Replay,
    Quit,
}

#[cfg(test)]
mod tests {
    use super::{sources_for_change, PlaybackSource, SourceCatalog};

    fn source(torrent: &str) -> PlaybackSource {
        PlaybackSource {
            torrent: torrent.to_string(),
            file_index: None,
            quality: "1080p".to_string(),
            label: torrent.to_string(),
            size: None,
            seeders: None,
        }
    }

    #[test]
    fn source_change_retries_other_known_sources() {
        let catalog = SourceCatalog {
            sources: vec![source("first"), source("selected"), source("last")],
            fallbacks: Vec::new(),
        };

        let candidates = sources_for_change(&catalog, &catalog.sources[1]);
        let torrents: Vec<_> = candidates
            .into_iter()
            .map(|candidate| candidate.torrent.as_str())
            .collect();

        assert_eq!(torrents, vec!["selected", "first", "last"]);
    }
}
