use crate::playback::LanguagePolicy;
use anyhow::{anyhow, Result};
use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

pub mod detect;
pub mod install;
pub mod ipc;

// Files at or above 2 GiB get larger player caches for 4K content.
const LARGE_FILE_THRESHOLD: u64 = 2 * 1_073_741_824;

// Track language preference, most preferred first. A player matches the first
// tag that a track declares, so this list holds ISO codes and the plain name.
const DEFAULT_LANGUAGES: &[&str] = &["eng", "en", "english"];

// Characters that must not stay literal inside a file URI. `#` separates the
// entries of the VLC `--input-slave` list, so this set always encodes it.
const URI_PATH_SET: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'%')
    .add(b'<')
    .add(b'>')
    .add(b'?')
    .add(b'[')
    .add(b']')
    .add(b'^')
    .add(b'`')
    .add(b'{')
    .add(b'|')
    .add(b'}');

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
    pub selected_tracks: Option<crate::playback::anime::SelectedTracks>,
    pub title: String,
    /// Subtitle files to attach, most preferred first. Every file becomes a
    /// selectable subtitle track in the player.
    pub subtitle_files: Vec<String>,
    /// Audio language preference, most preferred first.
    pub audio_languages: Vec<String>,
    /// Subtitle language preference, most preferred first.
    pub subtitle_languages: Vec<String>,
    pub start_at: Option<f64>,
    pub ipc_socket: Option<PathBuf>,
    pub show_output: bool,
    pub file_length: u64,
}

impl PlaybackOptions {
    pub fn new(title: String, file_length: u64) -> Self {
        let language_policy = LanguagePolicy::standard();
        Self {
            selected_tracks: None,
            title,
            subtitle_files: Vec::new(),
            audio_languages: language_policy.audio_languages,
            subtitle_languages: language_policy.subtitle_languages,
            start_at: None,
            ipc_socket: None,
            show_output: false,
            file_length,
        }
    }

    /// Applies the standard policy: original audio track, English subtitles.
    pub fn with_standard_languages(mut self) -> Self {
        let language_policy = LanguagePolicy::standard();
        self.audio_languages = language_policy.audio_languages;
        self.subtitle_languages = language_policy.subtitle_languages;
        self
    }

    pub fn with_language_policy(mut self, policy: LanguagePolicy) -> Self {
        self.audio_languages = policy.audio_languages;
        self.subtitle_languages = policy.subtitle_languages;
        self
    }

    pub fn with_subtitles(mut self, subtitle_files: Vec<String>) -> Self {
        self.subtitle_files = subtitle_files;
        self
    }

    pub fn with_selected_tracks(
        mut self,
        tracks: Option<crate::playback::anime::SelectedTracks>,
    ) -> Self {
        if tracks.is_some() {
            self = self.with_language_policy(LanguagePolicy::anime());
        }
        self.selected_tracks = tracks;
        self
    }

    pub fn with_show_output(mut self, show_output: bool) -> Self {
        self.show_output = show_output;
        self
    }

    pub fn with_ipc_socket(mut self, ipc_socket: Option<PathBuf>) -> Self {
        self.ipc_socket = ipc_socket;
        self
    }
}

/// Reads the language preference from `FLIX_AUDIO_LANGUAGE`, or returns the
/// English default. The variable holds a comma separated list, most preferred
/// first, for example `jpn,ja,japanese`.
pub fn preferred_languages() -> Vec<String> {
    let Ok(value) = std::env::var("FLIX_AUDIO_LANGUAGE") else {
        return default_languages();
    };
    let languages: Vec<String> = value
        .split(',')
        .map(|item| item.trim().to_ascii_lowercase())
        .filter(|item| !item.is_empty() && item.chars().all(|c| c.is_ascii_alphanumeric()))
        .collect();
    if languages.is_empty() {
        return default_languages();
    }
    languages
}

fn default_languages() -> Vec<String> {
    DEFAULT_LANGUAGES
        .iter()
        .map(|value| value.to_string())
        .collect()
}

pub struct ManagedPlayer {
    child: Option<Child>,
    started: Instant,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct PlayerExit {
    pub success: bool,
    pub exit_code: Option<i32>,
    pub signal: Option<i32>,
    pub runtime_ms: u64,
}

#[derive(Debug, thiserror::Error)]
#[error("The player closed during startup")]
pub struct PlayerStartupError(pub PlayerExit);

const STARTUP_CHECK: Duration = Duration::from_millis(500);

impl PlayerKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Mpv => "mpv",
            Self::Vlc => "VLC",
        }
    }
}

impl ManagedPlayer {
    pub fn new(child: Child) -> Self {
        Self {
            child: Some(child),
            started: Instant::now(),
        }
    }

    pub fn poll_exit(&mut self) -> Result<Option<PlayerExit>> {
        let Some(child) = self.child.as_mut() else {
            return Ok(None);
        };
        Ok(child.try_wait()?.map(|status| {
            #[cfg(unix)]
            let signal = {
                use std::os::unix::process::ExitStatusExt;
                status.signal()
            };
            #[cfg(not(unix))]
            let signal = None;
            PlayerExit {
                success: status.success(),
                exit_code: status.code(),
                signal,
                runtime_ms: self.started.elapsed().as_millis() as u64,
            }
        }))
    }

    /// Detects immediate process failures. This does not confirm video decoding.
    pub async fn wait_for_startup(&mut self) -> Result<()> {
        loop {
            if let Some(exit) = self.poll_exit()? {
                return Err(PlayerStartupError(exit).into());
            }
            if self.started.elapsed() >= STARTUP_CHECK {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
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
    let audio_languages = opts.audio_languages.join(",");
    let subtitle_languages = opts.subtitle_languages.join(",");

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

            if !audio_languages.is_empty() {
                cmd.arg(format!("--alang={audio_languages}"));
            }
            if !subtitle_languages.is_empty() {
                cmd.arg(format!("--slang={subtitle_languages}"));
            }
            cmd.arg("--subs-fallback=no");
            if let Some(tracks) = &opts.selected_tracks {
                cmd.arg(format!("--aid={}", tracks.audio + 1));
                if let Some(subtitle) = tracks.subtitle {
                    cmd.arg(format!("--sid={}", subtitle + 1));
                }
                cmd.arg("--subs-fallback-forced=no");
            }

            // mpv accepts one `--sub-file` for each extra subtitle track.
            for subtitle in &opts.subtitle_files {
                cmd.arg(format!("--sub-file={subtitle}"));
            }
            if let Some(start) = opts.start_at {
                cmd.arg(format!("--start={}", start));
            }
        }
        PlayerKind::Vlc => {
            // macOS VLC does not support these options. Direct launches create separate processes.
            #[cfg(not(target_os = "macos"))]
            {
                cmd.arg("--no-one-instance");
                cmd.arg("--no-one-instance-when-started-from-file");
            }
            // Do not use --play-and-exit. VLC exits on HTTP stream disconnects during seeks.
            // ManagedPlayer monitors process lifetime directly when the user closes the window.
            cmd.arg(url);
            cmd.arg(format!("--meta-title={}", opts.title));
            if opts.file_length >= LARGE_FILE_THRESHOLD {
                cmd.arg("--network-caching=15000");
            } else {
                cmd.arg("--network-caching=5000");
            }

            if !audio_languages.is_empty() {
                cmd.arg(format!("--audio-language={audio_languages}"));
            }
            if !subtitle_languages.is_empty() {
                cmd.arg(format!("--sub-language={subtitle_languages}"));
            }
            if let Some(tracks) = &opts.selected_tracks {
                cmd.arg(format!("--audio-track={}", tracks.audio));
                if let Some(subtitle) = tracks.subtitle {
                    cmd.arg(format!("--sub-track={subtitle}"));
                }
            }

            // VLC reads one `--sub-file`. Flix attaches every other subtitle as
            // an input slave, so the VLC subtitle menu lists it as a track.
            if let Some(primary) = opts.subtitle_files.first() {
                cmd.arg(format!("--sub-file={primary}"));
            }
            if let Some(slaves) = input_slave_list(&opts.subtitle_files) {
                cmd.arg(format!("--input-slave={slaves}"));
            }

            if let Some(start) = opts.start_at {
                cmd.arg(format!("--start-time={}", start));
            }
        }
    }

    cmd
}

/// Joins every subtitle after the first into a VLC `--input-slave` value.
fn input_slave_list(subtitle_files: &[String]) -> Option<String> {
    let slaves: Vec<String> = subtitle_files
        .iter()
        .skip(1)
        .map(|path| file_uri(Path::new(path)))
        .collect();
    (!slaves.is_empty()).then(|| slaves.join("#"))
}

/// Converts a local path into a `file://` URI that VLC can read.
pub fn file_uri(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let text = if text.starts_with('/') {
        text
    } else {
        format!("/{text}")
    };
    format!("file://{}", utf8_percent_encode(&text, URI_PATH_SET))
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
