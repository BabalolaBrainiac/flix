use std::path::PathBuf;

pub fn detect() -> Vec<super::Player> {
    let mut players = Vec::new();
    if let Ok(path) = which::which("mpv") {
        players.push(player(super::PlayerKind::Mpv, path));
    } else if let Some(path) = known_mpv_paths().into_iter().find(|path| path.is_file()) {
        players.push(player(super::PlayerKind::Mpv, path));
    }

    if let Ok(path) = which::which("vlc") {
        players.push(player(super::PlayerKind::Vlc, path));
    } else if let Some(path) = known_vlc_paths().into_iter().find(|path| path.is_file()) {
        players.push(player(super::PlayerKind::Vlc, path));
    }

    players
}

fn player(kind: super::PlayerKind, path: PathBuf) -> super::Player {
    super::Player {
        kind,
        path,
        version: None,
    }
}

#[cfg(target_os = "macos")]
fn known_mpv_paths() -> Vec<PathBuf> {
    [
        "/Applications/mpv.app/Contents/MacOS/mpv",
        "/opt/homebrew/bin/mpv",
        "/usr/local/bin/mpv",
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect()
}

#[cfg(target_os = "macos")]
fn known_vlc_paths() -> Vec<PathBuf> {
    vec![PathBuf::from("/Applications/VLC.app/Contents/MacOS/VLC")]
}

#[cfg(target_os = "linux")]
fn known_mpv_paths() -> Vec<PathBuf> {
    vec![
        PathBuf::from("/usr/bin/mpv"),
        PathBuf::from("/usr/local/bin/mpv"),
    ]
}

#[cfg(target_os = "linux")]
fn known_vlc_paths() -> Vec<PathBuf> {
    vec![
        PathBuf::from("/usr/bin/vlc"),
        PathBuf::from("/usr/local/bin/vlc"),
    ]
}

#[cfg(target_os = "windows")]
fn known_mpv_paths() -> Vec<PathBuf> {
    vec![PathBuf::from(r"C:\Program Files\mpv\mpv.exe")]
}

#[cfg(target_os = "windows")]
fn known_vlc_paths() -> Vec<PathBuf> {
    vec![PathBuf::from(r"C:\Program Files\VideoLAN\VLC\vlc.exe")]
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn known_mpv_paths() -> Vec<PathBuf> {
    Vec::new()
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn known_vlc_paths() -> Vec<PathBuf> {
    Vec::new()
}
