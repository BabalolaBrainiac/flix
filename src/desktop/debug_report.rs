use crate::playback::PlaybackSnapshot;
use serde::Serialize;
use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::{Seek, Write};
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use uuid::Uuid;

const MAX_EVENTS: usize = 256;

#[derive(Clone, Serialize)]
pub struct DebugEvent {
    elapsed_ms: u64,
    stage: &'static str,
    code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    player: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    exit: Option<crate::player::PlayerExit>,
}

#[derive(Serialize)]
pub struct DebugReport {
    schema_version: u32,
    version: &'static str,
    platform: &'static str,
    session_id: String,
    events: Vec<DebugEvent>,
}

struct Journal {
    events: VecDeque<DebugEvent>,
    file: Option<File>,
}

pub struct DebugLog {
    session_id: String,
    started: Instant,
    journal: Mutex<Journal>,
}

impl DebugLog {
    pub fn new(data_dir: &Path) -> Self {
        let session_id = Uuid::new_v4().to_string();
        let root = data_dir.join("logs");
        crate::storage::trim_cache(&root, 2 * 1024 * 1024, Duration::from_secs(7 * 86400));
        let file = open_log(&root, &session_id).ok();
        Self {
            session_id,
            started: Instant::now(),
            journal: Mutex::new(Journal {
                events: VecDeque::new(),
                file,
            }),
        }
    }

    pub fn record(&self, snapshot: &PlaybackSnapshot) {
        let (stage, code) = stage_and_code(snapshot);
        let event = DebugEvent {
            elapsed_ms: self.started.elapsed().as_millis() as u64,
            stage,
            code,
            player: match snapshot {
                PlaybackSnapshot::LaunchingPlayer { player_name, .. }
                | PlaybackSnapshot::Playing { player_name, .. } => match player_name.as_str() {
                    "VLC" => Some("VLC"),
                    "mpv" => Some("mpv"),
                    _ => None,
                },
                _ => None,
            },
            exit: None,
        };
        self.append(event);
    }

    pub fn record_player_exit(&self, player_name: &str, exit: crate::player::PlayerExit) {
        self.append(DebugEvent {
            elapsed_ms: self.started.elapsed().as_millis() as u64,
            stage: "player_exited",
            code: None,
            player: match player_name {
                "VLC" => Some("VLC"),
                "mpv" => Some("mpv"),
                _ => None,
            },
            exit: Some(exit),
        });
    }

    fn append(&self, event: DebugEvent) {
        let Ok(mut journal) = self.journal.lock() else {
            return;
        };
        journal.events.push_back(event);
        if journal.events.len() > MAX_EVENTS {
            journal.events.pop_front();
        }
        let report = self.report(&journal);
        if let Some(file) = &mut journal.file {
            let _ = write_report(file, &report);
        }
    }

    pub fn export(&self) -> DebugReport {
        match self.journal.lock() {
            Ok(journal) => self.report(&journal),
            Err(_) => self.report(&Journal {
                events: VecDeque::new(),
                file: None,
            }),
        }
    }

    fn report(&self, journal: &Journal) -> DebugReport {
        DebugReport {
            schema_version: 2,
            version: env!("CARGO_PKG_VERSION"),
            platform: std::env::consts::OS,
            session_id: self.session_id.clone(),
            events: journal.events.iter().cloned().collect(),
        }
    }
}

fn stage_and_code(snapshot: &PlaybackSnapshot) -> (&'static str, Option<String>) {
    let stage = match snapshot {
        PlaybackSnapshot::Idle => "idle",
        PlaybackSnapshot::ResolvingSource { .. } => "resolving_source",
        PlaybackSnapshot::LoadingTorrent { .. } => "loading_torrent",
        PlaybackSnapshot::Buffering { .. } => "buffering",
        PlaybackSnapshot::FindingSubtitle { .. } => "finding_subtitle",
        PlaybackSnapshot::DownloadingSubtitle { .. } => "downloading_subtitle",
        PlaybackSnapshot::LaunchingPlayer { .. } => "launching_player",
        PlaybackSnapshot::Playing { .. } => "playing",
        PlaybackSnapshot::Stopping => "stopping",
        PlaybackSnapshot::Failed { error } => return ("failed", Some(error.code.clone())),
    };
    (stage, None)
}

fn open_log(root: &Path, session_id: &str) -> std::io::Result<File> {
    std::fs::create_dir_all(root)?;
    let mut options = OpenOptions::new();
    options.read(true).write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(root.join(format!("{session_id}.json")))
}

fn write_report(file: &mut File, report: &DebugReport) -> std::io::Result<()> {
    file.rewind()?;
    let bytes = serde_json::to_vec(report)?;
    file.write_all(&bytes)?;
    file.set_len(bytes.len() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::playback::MediaRef;

    #[test]
    fn exports_bounded_events_without_media_or_local_paths() {
        let directory = tempfile::tempdir().unwrap();
        let log = DebugLog::new(directory.path());
        for _ in 0..300 {
            log.record(&PlaybackSnapshot::LoadingTorrent {
                media: MediaRef::Movie {
                    catalog_id: "private-id".into(),
                    title: "private-title".into(),
                },
                quality: "1080p".into(),
            });
        }
        let report = log.export();
        assert_eq!(report.events.len(), MAX_EVENTS);
        let text = serde_json::to_string(&report).unwrap();
        assert!(!text.contains("private"));
        assert!(!text.contains(&directory.path().display().to_string()));
        assert!(text.len() < 65536);
    }

    #[test]
    fn exports_player_exit_details_without_an_executable_path() {
        let directory = tempfile::tempdir().unwrap();
        let log = DebugLog::new(directory.path());
        log.record_player_exit(
            "/private/player",
            crate::player::PlayerExit {
                success: false,
                exit_code: Some(1),
                signal: None,
                runtime_ms: 50,
            },
        );
        let value = serde_json::to_value(log.export()).unwrap();
        assert_eq!(value["schema_version"], 2);
        assert_eq!(value["events"][0]["exit"]["exit_code"], 1);
        assert_eq!(value["events"][0]["exit"]["runtime_ms"], 50);
        assert!(!value.to_string().contains("private"));
    }
}
