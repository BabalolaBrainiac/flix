use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use flix::config::{Config, PlaybackCache};
use flix::library::{Entry, Library, SourceSerde};
use flix::media;
use flix::session::{Source, TorrentFile, TorrentId, TorrentSession};
use flix::{cli_download, cli_playback, cli_search, meta, player, stream_server, tui};
use std::io::{self, Write};
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::Mutex;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    Add {
        torrent: String,
    },
    Serve {
        torrent: String,
    },
    Play {
        torrent: String,
    },
    Search {
        query: String,
        #[arg(long)]
        anime: bool,
        #[arg(long, conflicts_with = "magnet")]
        download: bool,
        #[arg(long)]
        magnet: bool,
    },
    InstallPlayer,
    Tui,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Remove playback caches left behind by a previous crash or force quit.
    flix::config::sweep_orphaned_playback_caches();

    let cli = Cli::parse();
    let config = Config::load()?;
    if matches!(&cli.command, Some(Commands::InstallPlayer)) {
        return install_player();
    }

    if let Some(Commands::Play { torrent }) = cli.command.as_ref() {
        return run_temporary_play(
            &config,
            torrent,
            None,
            None,
            flix::cli_playback::SourceCatalog::default(),
        )
        .await;
    }

    if let Some(Commands::Search {
        query,
        anime,
        download,
        magnet,
    }) = cli.command.as_ref()
    {
        let action_override = if *download {
            Some(cli_search::SearchAction::Download)
        } else if *magnet {
            Some(cli_search::SearchAction::Magnet)
        } else {
            None
        };
        let Some(selection) = cli_search::select(query, *anime, action_override).await? else {
            return Ok(());
        };
        return match selection.action {
            cli_search::SearchAction::Play => {
                run_temporary_play(
                    &config,
                    &selection.magnet,
                    selection.file_index,
                    selection.episode_queue,
                    selection.catalog,
                )
                .await
            }
            cli_search::SearchAction::Download => {
                let session = Arc::new(TorrentSession::new(&config.download_dir).await?);
                cli_download::run(
                    session,
                    &config.download_dir,
                    &selection.magnet,
                    selection.file_index,
                )
                .await
            }
            cli_search::SearchAction::Magnet => {
                println!("{}", selection.magnet);
                Ok(())
            }
        };
    }

    let session = Arc::new(TorrentSession::new(&config.download_dir).await?);
    let library = Arc::new(Mutex::new(Library::load(&config.data_dir)?));

    match cli.command {
        Some(Commands::Add { torrent }) => {
            let source = Source::parse(&torrent)?;
            let id = session.add(source.clone()).await?;
            println!("Added torrent with ID: {}", id);

            let files = session.files(id)?;
            for file in &files {
                println!("{}: {} ({} bytes)", file.index, file.name, file.length);
            }

            let entry = build_entry(&session, id, source, &torrent, &files).await?;
            let mut lib = library.lock().await;
            lib.upsert(entry);
            lib.save()?;
        }
        Some(Commands::Serve { torrent }) => {
            let source = Source::parse(&torrent)?;
            let id = session.add(source).await?;
            println!("Added torrent with ID: {}", id);
            let handle = stream_server::serve(session.clone()).await?;

            let files = session.files(id)?;
            for file in files {
                let url = stream_server::stream_url(&handle, id, file.index);
                println!(
                    "{}: {} ({} bytes) -> {}",
                    file.index, file.name, file.length, url
                );
            }

            println!("Server running. Test with curl, e.g.: curl -i -r 0-1023 <URL>");

            tokio::signal::ctrl_c().await?;
        }
        Some(Commands::Play { .. }) => unreachable!(),
        Some(Commands::Search { .. }) => unreachable!(),
        Some(Commands::Tui) | None => {
            let mut terminal = tui::setup_terminal()?;
            let mut app =
                tui::app::App::new(session.clone(), library.clone(), config.data_dir.clone());

            let res = app.run(&mut terminal).await;

            tui::restore_terminal()?;

            res?;
        }
        Some(Commands::InstallPlayer) => unreachable!(),
    }

    Ok(())
}

async fn run_temporary_play(
    config: &Config,
    torrent: &str,
    preferred_file_index: Option<usize>,
    episode_queue: Option<flix::episode_queue::EpisodeQueue>,
    catalog: flix::cli_playback::SourceCatalog,
) -> Result<()> {
    let playback_cache = PlaybackCache::new()?;
    let session = Arc::new(TorrentSession::new(playback_cache.path()).await?);
    let playback_result = cli_playback::run_with_file_index(
        session.clone(),
        &config.data_dir,
        torrent,
        preferred_file_index,
        episode_queue,
        catalog,
    )
    .await;
    drop(session);
    let cleanup_result = playback_cache.close();
    playback_result?;
    cleanup_result
}

async fn build_entry(
    session: &TorrentSession,
    id: TorrentId,
    source: Source,
    input: &str,
    files: &[TorrentFile],
) -> Result<Entry> {
    let metadata = meta::lookup(input).await;
    let display_name = media::select_default_video(files)
        .map(|file| file.name.clone())
        .unwrap_or_else(|| metadata.title.clone());
    Ok(Entry {
        info_hash: session.info_hash(id)?,
        display_name,
        source: SourceSerde::from(&source),
        added_at: SystemTime::now(),
        last_played: None,
        meta: Some(metadata),
        output_name: flix::library::output_name_from_files(files),
    })
}

fn install_player() -> Result<()> {
    let manager = player::install::detect_package_manager().context(
        "No supported package manager found. Install mpv from https://mpv.io/installation/",
    )?;
    let command = player::install::install_command(&manager);
    println!("Flix will run this command:");
    println!("  {command}");
    print!("Continue? [y/N] ");
    io::stdout().flush()?;
    let mut input = io::stdin().lock();
    if !player::install::confirm(&mut input)? {
        println!("Install cancelled.");
        return Ok(());
    }
    player::install::run_package_install(manager)?;
    println!("mpv installation completed.");
    Ok(())
}
