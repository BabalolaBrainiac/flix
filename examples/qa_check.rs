//! Post-release QA check. Drives a packaged flix-desktop binary through its
//! local HTTP API: activate with a QA invite code, then play a movie, an
//! anime title, and a TV show, using local synthetic fixtures instead of a
//! live catalog or a public tracker. Exits non-zero on the first regression
//! it finds.
//!
//! Every check drives the browser-preview playback path over plain HTTP, so
//! no media player (VLC, mpv, or otherwise) is installed or launched.
//! Playback is redirected to a fixed, isolated directory via
//! FLIX_PLAYBACK_DIR (see src/config.rs), so this never touches a real
//! user's data and can run safely on any machine, not just CI.
//!
//! Usage: qa_check --app <path-to-flix-desktop> [--version <expected-version>]

use anyhow::{bail, Context, Result};
use librqbit::{create_torrent, CreateTorrentOptions};
use reqwest::{Method, StatusCode};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::{Command as StdCommand, Stdio};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::time::{sleep, timeout, Instant};

const PORT: u16 = 8998;
const BOOTSTRAP_LINE: &str = "Open this URL in your browser: ";
const CURRENT_BOOTSTRAP_LINE: &str = "Launch URL: ";
const STATE_TIMEOUT: Duration = Duration::from_secs(45);

struct Fixtures {
    movie: PathBuf,
    anime: PathBuf,
    show_dir: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    if let Err(error) = run().await {
        eprintln!("Post-release QA failed: {error:#}");
        std::process::exit(1);
    }
    Ok(())
}

async fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let app_path =
        PathBuf::from(arg_value(&args, "--app").context(
            "Usage: qa_check --app <path-to-flix-desktop> [--version <expected-version>]",
        )?);
    let expected_version = arg_value(&args, "--version").map(str::to_string);
    let invite_code = std::env::var("QA_INVITE_CODE").context("QA_INVITE_CODE must be set")?;

    let work_dir = std::env::current_dir()?.join("target").join("qa-run");
    let fixtures_dir = work_dir.join("fixtures");
    let torrents_dir = work_dir.join("torrents");
    // Every real user leaves FLIX_PLAYBACK_DIR unset, so playback writes into
    // a fresh, auto-cleaned-up temp directory it never shares with anything
    // else (see src/config.rs, PlaybackCache). Setting it here points every
    // /api/play in this run at one fixed directory this process fully owns,
    // so pre-placed fixtures land exactly where librqbit will look for them.
    let playback_dir = work_dir.join("playback");
    std::fs::create_dir_all(&fixtures_dir)?;
    std::fs::create_dir_all(&torrents_dir)?;
    std::fs::create_dir_all(&playback_dir)?;

    println!("Generating local test fixtures...");
    let fixtures = generate_fixtures(&fixtures_dir)?;

    println!("Packaging fixture torrents...");
    let movie_torrent = build_torrent(&fixtures.movie, &torrents_dir.join("movie.torrent")).await?;
    let anime_torrent = build_torrent(&fixtures.anime, &torrents_dir.join("anime.torrent")).await?;
    let show_torrent =
        build_torrent(&fixtures.show_dir, &torrents_dir.join("show.torrent")).await?;

    copy_file_into(&fixtures.movie, &playback_dir)?;
    copy_file_into(&fixtures.anime, &playback_dir)?;
    copy_dir_into(&fixtures.show_dir, &playback_dir)?;
    log_placed_fixtures(&playback_dir)?;

    println!("Starting flix-desktop...");
    let (mut child, credential) = start_app(&app_path, &playback_dir).await?;
    let client = reqwest::Client::new();

    let outcome = drive_checks(
        &client,
        &credential,
        &invite_code,
        expected_version.as_deref(),
        &movie_torrent,
        &anime_torrent,
        &show_torrent,
    )
    .await;

    let _ = api(&client, &credential, Method::POST, "/api/quit", None).await;
    if timeout(Duration::from_secs(5), child.wait()).await.is_err() {
        let _ = child.start_kill();
    }

    outcome
}

fn arg_value<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

// Builds every fixture with ffmpeg. Each file is a few seconds of synthetic
// video, so there is no external content dependency and no copyright
// question from generating and playing it in CI.
fn generate_fixtures(out_dir: &Path) -> Result<Fixtures> {
    let base = ["-hide_banner", "-loglevel", "error", "-y"];

    let movie = out_dir.join("movie.mp4");
    run_ffmpeg(
        &base,
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=640x360:rate=24",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000",
            "-t",
            "6",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            "-movflags",
            "+faststart",
        ],
        &movie,
    )?;

    let subtitle_path = out_dir.join("anime.srt");
    std::fs::write(
        &subtitle_path,
        "1\n00:00:00,000 --> 00:00:06,000\nQA fixture dialogue line.\n",
    )?;
    let anime = out_dir.join("anime.mkv");
    run_ffmpeg(
        &base,
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=640x360:rate=24",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=660:sample_rate=48000",
            "-i",
            subtitle_path.to_str().context("non-UTF8 fixture path")?,
            "-t",
            "6",
            "-map",
            "0:v",
            "-map",
            "1:a",
            "-map",
            "2:a",
            "-map",
            "3:s",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            "-c:s",
            "srt",
            "-metadata:s:a:0",
            "language=eng",
            "-metadata:s:a:0",
            "title=English dub",
            "-metadata:s:a:1",
            "language=jpn",
            "-metadata:s:a:1",
            "title=Japanese original",
            "-metadata:s:s:0",
            "language=eng",
            "-metadata:s:s:0",
            "title=Full dialogue",
            "-disposition:s:0",
            "default",
        ],
        &anime,
    )?;

    let show_dir = out_dir.join("show");
    std::fs::create_dir_all(&show_dir)?;
    for (season, episode, frequency) in [(1, 1, 370), (1, 2, 410)] {
        let target = show_dir.join(format!("Flix.QA.Show.S{season:02}E{episode:02}.mp4"));
        let audio_source = format!("sine=frequency={frequency}:sample_rate=48000");
        run_ffmpeg(
            &base,
            &[
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=640x360:rate=24",
                "-f",
                "lavfi",
                "-i",
                audio_source.as_str(),
                "-t",
                "4",
                "-c:v",
                "libx264",
                "-preset",
                "ultrafast",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
                "-movflags",
                "+faststart",
            ],
            &target,
        )?;
    }

    Ok(Fixtures {
        movie,
        anime,
        show_dir,
    })
}

fn run_ffmpeg(base: &[&str], extra: &[&str], output: &Path) -> Result<()> {
    let status = StdCommand::new("ffmpeg")
        .args(base)
        .args(extra)
        .arg(output)
        .status()
        .context("Failed to run ffmpeg. Is it installed?")?;
    if !status.success() {
        bail!(
            "ffmpeg exited with status {status} while building {}",
            output.display()
        );
    }
    Ok(())
}

// Builds a `.torrent` file with the same call Flix's own session code uses,
// so the file layout it expects under the playback directory matches
// exactly: a single input file downloads to `<playback_dir>/<basename>`, and
// an input directory downloads to `<playback_dir>/<dir basename>/...`.
async fn build_torrent(input: &Path, output: &Path) -> Result<PathBuf> {
    let torrent = create_torrent(input, CreateTorrentOptions::default())
        .await
        .with_context(|| format!("Failed to build a torrent for {}", input.display()))?;
    let bytes = torrent.as_bytes()?;
    std::fs::write(output, bytes)?;
    Ok(output.to_path_buf())
}

fn copy_file_into(file: &Path, playback_dir: &Path) -> Result<()> {
    let name = file.file_name().context("fixture file has no name")?;
    std::fs::copy(file, playback_dir.join(name))?;
    Ok(())
}

fn copy_dir_into(dir: &Path, playback_dir: &Path) -> Result<()> {
    let name = dir.file_name().context("fixture directory has no name")?;
    let target = playback_dir.join(name);
    std::fs::create_dir_all(&target)?;
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        std::fs::copy(entry.path(), target.join(entry.file_name()))?;
    }
    Ok(())
}

// Confirms exactly what landed on disk, with sizes, right before
// flix-desktop starts.
fn log_placed_fixtures(playback_dir: &Path) -> Result<()> {
    fn walk(dir: &Path, depth: usize) -> Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let meta = entry.metadata()?;
            println!(
                "{}{} ({} bytes)",
                "  ".repeat(depth),
                entry.path().display(),
                meta.len()
            );
            if meta.is_dir() {
                walk(&entry.path(), depth + 1)?;
            }
        }
        Ok(())
    }
    println!("Fixtures placed under {}:", playback_dir.display());
    walk(playback_dir, 1)
}

fn bootstrap_token(line: &str) -> Option<String> {
    let url_text = [CURRENT_BOOTSTRAP_LINE, BOOTSTRAP_LINE]
        .into_iter()
        .find_map(|prefix| line.strip_prefix(prefix))?;
    let url = reqwest::Url::parse(url_text.trim()).ok()?;
    url.query_pairs()
        .find(|(key, _)| key == "token")
        .map(|(_, token)| token.into_owned())
}

// The app prints its local bootstrap URL once on startup. A background task
// keeps draining stdout after that so the child never blocks on a full pipe.
async fn start_app(app_path: &Path, playback_dir: &Path) -> Result<(Child, String)> {
    let mut child = Command::new(app_path)
        .args(["--no-open", "--port", &PORT.to_string()])
        // Makes every /api/play in this run write into playback_dir instead
        // of a fresh temp directory, so the fixtures placed there ahead of
        // time are exactly where librqbit looks (see src/config.rs).
        .env("FLIX_PLAYBACK_DIR", playback_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("Failed to start {}", app_path.display()))?;
    let stdout = child
        .stdout
        .take()
        .context("flix-desktop stdout was not piped")?;
    let mut lines = BufReader::new(stdout).lines();
    let (tx, rx) = tokio::sync::oneshot::channel();

    tokio::spawn(async move {
        let mut tx = Some(tx);
        while let Ok(Some(line)) = lines.next_line().await {
            println!("[flix-desktop] {line}");
            if let Some(token) = bootstrap_token(&line) {
                if let Some(sender) = tx.take() {
                    let _ = sender.send(token);
                }
            }
        }
    });

    let credential = timeout(Duration::from_secs(20), rx)
        .await
        .context("Timed out waiting for the flix-desktop bootstrap line")?
        .context("flix-desktop exited before printing its bootstrap line")?;
    Ok((child, credential))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_current_bootstrap_line() {
        let token = bootstrap_token("Launch URL: http://127.0.0.1:8998/?token=current-token");
        assert_eq!(token.as_deref(), Some("current-token"));
    }

    #[test]
    fn retains_support_for_the_legacy_bootstrap_line() {
        let token = bootstrap_token(
            "Open this URL in your browser: http://127.0.0.1:8998/?token=legacy-token",
        );
        assert_eq!(token.as_deref(), Some("legacy-token"));
    }

    #[test]
    fn parses_optional_version_argument() {
        let args = vec![
            "qa_check".to_string(),
            "--app".to_string(),
            "/path/to/app".to_string(),
            "--version".to_string(),
            "0.4.1".to_string(),
        ];
        assert_eq!(arg_value(&args, "--app"), Some("/path/to/app"));
        assert_eq!(arg_value(&args, "--version"), Some("0.4.1"));
        assert_eq!(arg_value(&args, "--missing"), None);
    }
}

async fn api(
    client: &reqwest::Client,
    credential: &str,
    method: Method,
    endpoint: &str,
    body: Option<Value>,
) -> Result<Value> {
    let url = format!("http://127.0.0.1:{PORT}{endpoint}");
    let mut request = client.request(method.clone(), &url).bearer_auth(credential);
    if let Some(body) = body {
        request = request.json(&body);
    }
    let response = request
        .send()
        .await
        .with_context(|| format!("{method} {endpoint} failed"))?;
    let status = response.status();
    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        bail!("{method} {endpoint} returned {status}: {text}");
    }
    if status == StatusCode::NO_CONTENT {
        return Ok(Value::Null);
    }
    response
        .json::<Value>()
        .await
        .with_context(|| format!("{method} {endpoint} returned invalid JSON"))
}

async fn wait_for_health(
    client: &reqwest::Client,
    credential: &str,
    expected_version: Option<&str>,
) -> Result<()> {
    for _ in 0..50 {
        if let Ok(health) = api(client, credential, Method::GET, "/api/health", None).await {
            let version = health["version"]
                .as_str()
                .context("Health response is missing the version field")?;
            if let Some(expected) = expected_version {
                if version != expected {
                    bail!("Expected flix-desktop version {expected}, got {version}");
                }
            }
            return Ok(());
        }
        sleep(Duration::from_millis(200)).await;
    }
    bail!("flix-desktop never became healthy");
}

async fn poll_until(
    client: &reqwest::Client,
    credential: &str,
    is_terminal: impl Fn(&Value) -> bool,
    label: &str,
) -> Result<Value> {
    let deadline = Instant::now() + STATE_TIMEOUT;
    let mut last = Value::Null;
    while Instant::now() < deadline {
        let status = api(client, credential, Method::GET, "/api/status", None).await?;
        last = status["state"].clone();
        if is_terminal(&last) {
            return Ok(last);
        }
        sleep(Duration::from_millis(500)).await;
    }
    bail!("{label}: timed out waiting for a terminal state. Last state: {last}");
}

fn is_browser_or_failed(state: &Value) -> bool {
    matches!(state["state"].as_str(), Some("browser") | Some("failed"))
}

// Confirms the browser-preview stream URL actually serves real bytes, not
// just that the app produced a URL string. No player and no browser needed:
// the URL carries its own one-time access token in the query string.
async fn assert_stream_serves_bytes(
    client: &reqwest::Client,
    stream_url: &str,
    label: &str,
) -> Result<()> {
    let response = client
        .get(stream_url)
        .header("Range", "bytes=0-65535")
        .send()
        .await
        .with_context(|| format!("{label}: could not fetch the stream URL"))?;
    if !matches!(
        response.status(),
        StatusCode::OK | StatusCode::PARTIAL_CONTENT
    ) {
        bail!("{label}: stream URL returned HTTP {}", response.status());
    }
    let bytes = response.bytes().await?;
    if bytes.is_empty() {
        bail!("{label}: stream URL returned no bytes");
    }
    Ok(())
}

async fn check_update_discovery(
    client: &reqwest::Client,
    credential: &str,
    expected_version: Option<&str>,
) -> Result<()> {
    let update = api(client, credential, Method::GET, "/api/update", None).await?;
    let current_version = update["current_version"]
        .as_str()
        .context("Missing current_version in update response")?;
    let latest_version = update["latest_version"]
        .as_str()
        .context("Missing latest_version in update response")?;
    if let Some(expected) = expected_version {
        if current_version != expected {
            bail!("Expected current_version {expected}, got {current_version}");
        }
    }
    println!("Update discovery confirmed: current={current_version}, latest={latest_version}, available={}", update["available"]);
    Ok(())
}

async fn drive_checks(
    client: &reqwest::Client,
    credential: &str,
    invite_code: &str,
    expected_version: Option<&str>,
    movie_torrent: &Path,
    anime_torrent: &Path,
    show_torrent: &Path,
) -> Result<()> {
    wait_for_health(client, credential, expected_version).await?;

    println!("Checking update discovery against published release metadata...");
    check_update_discovery(client, credential, expected_version).await?;

    println!("Activating with the QA invite code...");
    api(
        client,
        credential,
        Method::POST,
        "/api/activation/redeem",
        Some(json!({ "invite_code": invite_code })),
    )
    .await?;

    println!("Checking movie playback...");
    check_movie(client, credential, movie_torrent).await?;

    println!("Checking anime playback (original Japanese audio, English subtitles)...");
    check_anime(client, credential, anime_torrent).await?;

    println!("Checking TV show playback and the next-episode action...");
    check_tv_show(client, credential, show_torrent).await?;

    println!("All post-release QA checks passed.");
    Ok(())
}

async fn check_movie(client: &reqwest::Client, credential: &str, torrent: &Path) -> Result<()> {
    api(
        client,
        credential,
        Method::POST,
        "/api/play",
        Some(json!({
            "target": "browser",
            "media_ref": { "type": "movie", "catalog_id": "qa:movie", "title": "QA Movie Fixture" },
            "magnet": torrent.to_string_lossy(),
        })),
    )
    .await?;
    let state = poll_until(client, credential, is_browser_or_failed, "movie").await?;
    if state["state"] != "browser" {
        bail!(
            "movie: expected browser playback to start, got a failure - {}",
            state["error"]
        );
    }
    assert_stream_serves_bytes(
        client,
        state["stream_url"].as_str().context("missing stream_url")?,
        "movie",
    )
    .await?;
    api(client, credential, Method::POST, "/api/stop", None).await?;
    Ok(())
}

// Anime cannot play through the browser preview by design - Flix rejects it
// there to preserve the verified-track guarantee (see src/playback/browser.rs).
// But the strict Japanese-audio/English-subtitle track selection still runs
// in full before that rejection (see src/playback/coordinator.rs,
// launch_active_session). So playing the anime fixture with target "browser"
// is the right way to test that selection logic without any external
// player: reaching "failed" with error code BROWSER_UNSUPPORTED means the
// track selection succeeded and only the deliberate browser guard stopped
// it - that is the passing outcome here. Any other outcome (playing at all,
// or a different failure code such as ANIME_LANGUAGE_UNAVAILABLE) is a real
// regression.
async fn check_anime(client: &reqwest::Client, credential: &str, torrent: &Path) -> Result<()> {
    api(
        client,
        credential,
        Method::POST,
        "/api/play",
        Some(json!({
            "target": "browser",
            "media_ref": { "type": "movie", "catalog_id": "kitsu:qa-anime", "title": "QA Anime Fixture" },
            "magnet": torrent.to_string_lossy(),
        })),
    )
    .await?;
    let state = poll_until(client, credential, is_browser_or_failed, "anime").await?;
    let code = state["error"]["code"].as_str().unwrap_or_default();
    if state["state"] != "failed" || code != "BROWSER_UNSUPPORTED" {
        bail!(
            "anime: expected the Japanese audio and English subtitle tracks to be selected, then hit \
             the browser-anime guard (BROWSER_UNSUPPORTED). Got {state}"
        );
    }
    Ok(())
}

async fn check_tv_show(client: &reqwest::Client, credential: &str, torrent: &Path) -> Result<()> {
    api(
        client,
        credential,
        Method::POST,
        "/api/play",
        Some(json!({
            "target": "browser",
            "media_ref": {
                "type": "episode",
                "catalog_id": "qa:show",
                "stream_id": "qa-show-s01e01",
                "season": 1,
                "episode": 1,
                "title": "QA Show Fixture",
            },
            // Without a queue_seed the coordinator has no episode queue to
            // advance, so /api/next moves to the next file in the torrent
            // but the reported `media` field (sourced from the queue, not
            // the file) never updates. A real client always supplies one
            // for a series; matching that is what makes this check
            // faithful to production instead of just the file-level move.
            "queue_seed": {
                "catalog_item": {
                    "id": "qa:show",
                    "name": "QA Show Fixture",
                    "media_type": "series",
                    "release_info": null,
                    "poster": null,
                    "is_anime": false,
                },
                "episodes": [
                    {"id": "ep1", "stream_id": "qa-show-s01e01", "title": "Episode 1", "season": 1, "episode": 1},
                    {"id": "ep2", "stream_id": "qa-show-s01e02", "title": "Episode 2", "season": 1, "episode": 2},
                ],
                "current_episode_id": "ep1",
            },
            "magnet": torrent.to_string_lossy(),
        })),
    )
    .await?;
    let first = poll_until(
        client,
        credential,
        is_browser_or_failed,
        "tv show episode 1",
    )
    .await?;
    if first["state"] != "browser" {
        bail!(
            "tv show: expected episode 1 to start, got a failure - {}",
            first["error"]
        );
    }
    if first["media"]["season"] != 1 || first["media"]["episode"] != 1 {
        bail!(
            "tv show: expected season 1 episode 1, got {}",
            first["media"]
        );
    }
    if first["has_next"] != true {
        bail!("tv show: episode 1 did not report a next episode in the season pack");
    }
    assert_stream_serves_bytes(
        client,
        first["stream_url"].as_str().context("missing stream_url")?,
        "tv show episode 1",
    )
    .await?;

    api(client, credential, Method::POST, "/api/next", None).await?;
    let second = poll_until(
        client,
        credential,
        is_browser_or_failed,
        "tv show episode 2",
    )
    .await?;
    if second["state"] != "browser" {
        bail!(
            "tv show: expected episode 2 to start after next, got a failure - {}",
            second["error"]
        );
    }
    if second["media"]["season"] != 1 || second["media"]["episode"] != 2 {
        bail!(
            "tv show: expected the next action to reach season 1 episode 2, got {}",
            second["media"]
        );
    }
    assert_stream_serves_bytes(
        client,
        second["stream_url"]
            .as_str()
            .context("missing stream_url")?,
        "tv show episode 2",
    )
    .await?;
    api(client, credential, Method::POST, "/api/stop", None).await?;
    Ok(())
}
