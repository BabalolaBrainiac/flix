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

#[test]
fn players_receive_the_subtitle_file() {
    let options = PlaybackOptions {
        title: "Episode".to_string(),
        subtitle_url: Some("/tmp/episode.srt".to_string()),
        start_at: None,
        ipc_socket: None,
        show_output: false,
        file_length: 1_000_000_000,
    };
    for kind in [PlayerKind::Mpv, PlayerKind::Vlc] {
        let player = Player {
            kind,
            path: PathBuf::from("player"),
            version: None,
        };
        let args: Vec<String> = command(&player, "http://127.0.0.1/video", &options)
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert!(args.iter().any(|arg| arg == "--sub-file=/tmp/episode.srt"));
    }
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
