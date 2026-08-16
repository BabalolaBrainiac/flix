use anyhow::{anyhow, Result};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

pub mod detect;
pub mod install;
pub mod ipc;

// Files at or above 2 GiB get larger player caches for 4K content.
const LARGE_FILE_THRESHOLD: u64 = 2 * 1_073_741_824;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerKind {
    Mpv,
    Vlc,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Player {
    pub kind: PlayerKind,
    pub path: PathBuf,
    pub version: Option<String>,
}

pub struct PlaybackOptions {
    pub title: String,
    pub subtitle_url: Option<String>,
    pub start_at: Option<f64>,
    pub ipc_socket: Option<PathBuf>,
    pub show_output: bool,
    pub file_length: u64,
}

pub struct ManagedPlayer {
    child: Option<Child>,
}

impl ManagedPlayer {
    pub fn new(child: Child) -> Self {
        Self { child: Some(child) }
    }

    pub fn has_exited(&mut self) -> Result<bool> {
        match self.child.as_mut() {
            Some(child) => Ok(child.try_wait()?.is_some()),
            None => Ok(true),
        }
    }

    pub fn wait(&mut self) -> Result<()> {
        if let Some(mut child) = self.child.take() {
            child.wait()?;
        }
        Ok(())
    }

    pub fn stop(&mut self) -> Result<()> {
        let Some(mut child) = self.child.take() else {
            return Ok(());
        };
        if child.try_wait()?.is_none() {
            child.kill()?;
        }
        child.wait()?;
        Ok(())
    }
}

impl Drop for ManagedPlayer {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

pub fn detect() -> Vec<Player> {
    detect::detect()
}

pub fn command(p: &Player, url: &str, opts: &PlaybackOptions) -> Command {
    let mut cmd = Command::new(&p.path);

    match p.kind {
        PlayerKind::Mpv => {
            cmd.arg(url);
            cmd.arg(format!("--force-media-title={}", opts.title));

            if let Some(ref socket) = opts.ipc_socket {
                cmd.arg(format!("--input-ipc-server={}", socket.to_string_lossy()));
            }

            cmd.arg("--cache=yes");
            if opts.file_length >= LARGE_FILE_THRESHOLD {
                cmd.arg("--demuxer-max-bytes=512MiB");
                cmd.arg("--cache-secs=30");
            } else {
                cmd.arg("--demuxer-max-bytes=256MiB");
            }

            if let Some(ref sub) = opts.subtitle_url {
                cmd.arg(format!("--sub-file={}", sub));
            }
            if let Some(start) = opts.start_at {
                cmd.arg(format!("--start={}", start));
            }
        }
        PlayerKind::Vlc => {
            cmd.arg(url);
            cmd.arg(format!("--meta-title={}", opts.title));
            if opts.file_length >= LARGE_FILE_THRESHOLD {
                cmd.arg("--network-caching=15000");
            } else {
                cmd.arg("--network-caching=5000");
            }

            if let Some(ref sub) = opts.subtitle_url {
                cmd.arg(format!("--sub-file={}", sub));
            }

            if let Some(start) = opts.start_at {
                cmd.arg(format!("--start-time={}", start));
            }
        }
    }

    cmd
}

pub fn launch(p: &Player, url: &str, opts: &PlaybackOptions) -> Result<ManagedPlayer> {
    let mut command = command(p, url, opts);
    if !opts.show_output {
        command.stdout(Stdio::null()).stderr(Stdio::null());
    }
    let child = command
        .spawn()
        .map_err(|e| anyhow!("Failed to spawn player: {}", e))?;
    Ok(ManagedPlayer::new(child))
}
