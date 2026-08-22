#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use anyhow::Result;
use clap::Parser;
use flix::config::Config;
use flix::desktop::{DesktopServer, DesktopService, SettingsAction, SettingsCommand};
use std::process::Command;

#[derive(Parser, Debug)]
#[command(name = "flix-desktop", version, about = "Flix private desktop client")]
struct Args {
    #[arg(long)]
    check: bool,

    #[arg(long)]
    no_open: bool,

    /// Bind the local web server to this port. Omit to use any free port.
    #[arg(long)]
    port: Option<u16>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging so pipeline errors are visible. RUST_LOG controls the
    // level; the default keeps warnings and errors. Without a subscriber every
    // tracing event is dropped silently.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("flix=info,warn")),
        )
        .init();

    // Remove playback caches left behind by a previous crash or force quit.
    flix::config::sweep_orphaned_playback_caches();

    let args = Args::parse();
    let config = Config::load()?;
    let service = DesktopService::new(config);

    if args.check {
        let settings = service.settings(&SettingsCommand {
            action: SettingsAction::Get,
        })?;
        println!("Flix Desktop core check passed.");
        println!("Download directory: {}", settings.download_dir);
        println!("Data directory: {}", settings.data_dir);
        println!("Available players: {}", settings.players.len());
        return Ok(());
    }

    let server = match args.port {
        Some(port) => DesktopServer::bind_on(service, port).await?,
        None => DesktopServer::bind(service).await?,
    };
    let launch_url = server.url();

    println!("Flix Desktop server bound to: {}", server.local_url());

    if !args.no_open {
        println!("Opening default browser at launch URL...");
        open_browser(&launch_url);
    } else {
        println!("Open this URL in your browser: {}", launch_url);
    }

    // Run server with Ctrl+C signal and internal /api/quit support
    let shutdown_tx = server.state.shutdown_tx.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            println!("\nReceived shutdown signal. Stopping Flix Desktop...");
            let _ = shutdown_tx.send(());
        }
    });

    server.run().await?;
    println!("Flix Desktop stopped.");
    Ok(())
}

fn open_browser(url: &str) {
    #[cfg(target_os = "macos")]
    let _ = Command::new("open").arg(url).spawn();

    #[cfg(target_os = "windows")]
    let _ = Command::new("cmd").args(["/C", "start", url]).spawn();

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    let _ = Command::new("xdg-open").arg(url).spawn();
}
