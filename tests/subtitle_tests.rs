use flix::player::{command, PlaybackOptions, Player, PlayerKind};
use flix::subtitles::{parse_search_response, SubtitleQuery};
use serde_json::json;
use std::path::PathBuf;

#[test]
fn builds_episode_subtitle_query() {
    let query = SubtitleQuery::for_file("Special.Ops.Lioness.S01E01.1080p.mkv", "en");

    assert_eq!(query.title, "Special Ops Lioness");
    assert_eq!(query.season, Some(1));
    assert_eq!(query.episode, Some(1));
    assert_eq!(query.language, "en");
}

#[test]
fn ignores_subtitle_results_without_a_file_id() {
    let response = json!({
        "data": [
            {"attributes": {"language": "en", "files": []}},
            {"attributes": {"language": "en", "release": "WEB-DL", "download_count": 500, "files": [{"file_id": 42, "file_name": "episode\u{001b}[31m.srt"}]}}
        ]
    });

    let results = parse_search_response(response).expect("valid response");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].file_id, 42);
    assert_eq!(results[0].release.as_deref(), Some("WEB-DL"));
    assert_eq!(results[0].download_count, 500);
    assert!(!results[0]
        .file_name
        .as_deref()
        .unwrap_or_default()
        .contains('\u{1b}'));
}

fn player_args(kind: PlayerKind, options: &PlaybackOptions) -> Vec<String> {
    let player = Player {
        kind,
        path: PathBuf::from("player"),
        version: None,
    };
    command(&player, "http://127.0.0.1/video", options)
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect()
}

#[test]
fn players_receive_the_subtitle_file() {
    let options = PlaybackOptions::new("Episode".to_string(), 1_000_000_000)
        .with_subtitles(vec!["/tmp/episode.srt".to_string()]);
    for kind in [PlayerKind::Mpv, PlayerKind::Vlc] {
        let args = player_args(kind, &options);
        assert!(args.iter().any(|arg| arg == "--sub-file=/tmp/episode.srt"));
    }
}

#[test]
fn players_prefer_english_audio_and_subtitles() {
    // FLIX_AUDIO_LANGUAGE can leak in from the environment and force an audio
    // language. Remove it so the test checks the real default.
    std::env::remove_var("FLIX_AUDIO_LANGUAGE");

    let options =
        PlaybackOptions::new("Episode".to_string(), 1_000_000_000).with_standard_languages();
    assert_eq!(
        options.audio_languages,
        vec!["eng", "en", "english", "original"]
    );
    assert_eq!(options.subtitle_languages, vec!["eng", "en", "english"]);

    let mpv = player_args(PlayerKind::Mpv, &options);
    assert!(mpv
        .iter()
        .any(|arg| arg == "--alang=eng,en,english,original"));
    assert!(mpv.iter().any(|arg| arg == "--slang=eng,en,english"));

    let vlc = player_args(PlayerKind::Vlc, &options);
    assert!(vlc
        .iter()
        .any(|arg| arg == "--audio-language=eng,en,english,original"));
    assert!(vlc.iter().any(|arg| arg == "--sub-language=eng,en,english"));
}

#[test]
fn every_subtitle_becomes_a_player_track() {
    let options = PlaybackOptions::new("Episode".to_string(), 1_000_000_000).with_subtitles(vec![
        "/tmp/one.srt".to_string(),
        "/tmp/two.srt".to_string(),
        "/tmp/name with space.srt".to_string(),
    ]);

    // mpv takes one `--sub-file` for each track.
    let mpv = player_args(PlayerKind::Mpv, &options);
    assert_eq!(
        mpv.iter()
            .filter(|arg| arg.starts_with("--sub-file="))
            .count(),
        3
    );

    // VLC takes the first track directly and the rest as input slaves.
    let vlc = player_args(PlayerKind::Vlc, &options);
    assert!(vlc.iter().any(|arg| arg == "--sub-file=/tmp/one.srt"));
    let slaves = vlc
        .iter()
        .find(|arg| arg.starts_with("--input-slave="))
        .expect("input slave argument");
    assert_eq!(
        slaves,
        "--input-slave=file:///tmp/two.srt#file:///tmp/name%20with%20space.srt"
    );
}

#[test]
fn a_single_subtitle_needs_no_input_slave() {
    let options = PlaybackOptions::new("Episode".to_string(), 1_000_000_000)
        .with_subtitles(vec!["/tmp/one.srt".to_string()]);

    let vlc = player_args(PlayerKind::Vlc, &options);
    assert!(!vlc.iter().any(|arg| arg.starts_with("--input-slave=")));
}

#[tokio::test]
async fn lists_every_cached_english_subtitle_for_one_video() {
    let directory = tempfile::tempdir().expect("temporary data directory");
    let hash = "0123456789012345678901234567890123456789";
    let subtitle_dir = directory.path().join("subtitles");
    std::fs::create_dir_all(&subtitle_dir).expect("subtitle directory");
    for name in [
        // OpenSubtitles download, Stremio download, and the legacy name.
        "0123456789012345678901234567890123456789.2.42.en.srt",
        "0123456789012345678901234567890123456789.2.stremio.en.srt",
        "0123456789012345678901234567890123456789.2.en.srt",
    ] {
        std::fs::write(subtitle_dir.join(name), b"1\n").expect("subtitle file");
    }
    // Another video of the same torrent, and an unfinished download.
    std::fs::write(
        subtitle_dir.join("0123456789012345678901234567890123456789.21.en.srt"),
        b"1\n",
    )
    .expect("other subtitle file");
    std::fs::write(
        subtitle_dir.join("0123456789012345678901234567890123456789.2.7.en.part"),
        b"1\n",
    )
    .expect("partial subtitle file");

    let cached = flix::subtitles::cached_english_subtitles(directory.path(), hash, 2)
        .await
        .expect("subtitle cache scan");

    assert_eq!(cached.len(), 3);
    assert!(cached
        .iter()
        .all(|path| path.to_string_lossy().contains(".2.")));
}

#[tokio::test]
async fn an_empty_subtitle_cache_returns_no_file() {
    let directory = tempfile::tempdir().expect("temporary data directory");
    let cached = flix::subtitles::cached_english_subtitles(
        directory.path(),
        "0123456789012345678901234567890123456789",
        0,
    )
    .await
    .expect("subtitle cache scan");

    assert!(cached.is_empty());
}

#[tokio::test]
#[ignore = "requires OpenSubtitles credentials and uses one subtitle download"]
async fn downloads_an_english_episode_subtitle() {
    let directory = tempfile::tempdir().expect("temporary subtitle directory");
    let subtitle = flix::subtitles::find_english_subtitle(
        directory.path(),
        "0123456789012345678901234567890123456789",
        0,
        "Special.Ops.Lioness.S01E01.1080p.mkv",
    )
    .await
    .expect("OpenSubtitles request");
    let path = subtitle.expect("English subtitle result");
    assert!(std::fs::metadata(path).expect("subtitle metadata").len() > 0);
}

#[tokio::test]
#[ignore = "requires an OpenSubtitles API key"]
async fn searches_english_subtitles_without_login() {
    let directory = tempfile::tempdir().expect("temporary subtitle directory");
    let options = flix::subtitles::english_subtitle_options(
        directory.path(),
        "0123456789012345678901234567890123456789",
        0,
        "Special.Ops.Lioness.S01E01.1080p.mkv",
    )
    .await
    .expect("OpenSubtitles search");

    assert!(!options.results.is_empty());
}
