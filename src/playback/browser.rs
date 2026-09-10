use anyhow::{ensure, Result};
use std::fmt;
use tokio::io::AsyncReadExt;

const SUBTITLE_LIMIT: u64 = 2 * 1024 * 1024;

#[derive(Debug)]
pub struct BrowserUnsupported(&'static str);

impl fmt::Display for BrowserUnsupported {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl std::error::Error for BrowserUnsupported {}

pub fn mime_type(name: &str, is_anime: bool) -> Result<&'static str> {
    if is_anime {
        return Err(BrowserUnsupported("Browser anime playback needs verified track selection. Use an external player for now.").into());
    }
    match std::path::Path::new(name).extension().and_then(|value| value.to_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("mp4" | "m4v") => Ok("video/mp4"),
        Some("webm") => Ok("video/webm"),
        _ => Err(BrowserUnsupported("Browser playback currently supports MP4 and WebM. Use an external player for this source.").into()),
    }
}

pub async fn prepare_subtitle(paths: &[String]) -> Option<Vec<u8>> {
    for path in paths.iter().take(3) {
        if let Ok(bytes) = read_subtitle(path).await {
            return Some(bytes);
        }
    }
    None
}

async fn read_subtitle(path: &str) -> Result<Vec<u8>> {
    let file = tokio::fs::File::open(path).await?;
    let mut bytes = Vec::new();
    file.take(SUBTITLE_LIMIT + 1)
        .read_to_end(&mut bytes)
        .await?;
    ensure!(
        bytes.len() as u64 <= SUBTITLE_LIMIT,
        "Subtitle exceeds the size limit"
    );
    to_webvtt(std::str::from_utf8(&bytes)?)
}

fn timestamp(value: &str) -> Option<u64> {
    let normalized = value.trim().replace(',', ".");
    let (clock, millis) = normalized.split_once('.')?;
    let fields = clock.split(':').collect::<Vec<_>>();
    if fields.len() != 3 || millis.len() != 3 {
        return None;
    }
    let hours = fields[0].parse::<u64>().ok()?;
    let minutes = fields[1].parse::<u64>().ok()?;
    let seconds = fields[2].parse::<u64>().ok()?;
    let millis = millis.parse::<u64>().ok()?;
    (hours <= 999 && minutes < 60 && seconds < 60)
        .then_some(((hours * 60 + minutes) * 60 + seconds) * 1000 + millis)
}

fn to_webvtt(input: &str) -> Result<Vec<u8>> {
    let normalized = input.trim_start_matches('\u{feff}').replace("\r\n", "\n");
    let mut output = String::from("WEBVTT\n\n");
    let mut count = 0;
    for block in normalized.split("\n\n") {
        let mut lines = block.lines();
        let Some(timing) = lines.find(|line| line.contains(" --> ")) else {
            continue;
        };
        let Some((start, end)) = timing.split_once(" --> ") else {
            continue;
        };
        let (Some(start_ms), Some(end_ms)) = (timestamp(start), timestamp(end)) else {
            continue;
        };
        if end_ms <= start_ms {
            continue;
        }
        output.push_str(&timing.replace(',', "."));
        output.push('\n');
        for line in lines {
            output.push_str(
                &line
                    .replace('&', "&amp;")
                    .replace('<', "&lt;")
                    .replace('>', "&gt;"),
            );
            output.push('\n');
        }
        output.push('\n');
        count += 1;
    }
    ensure!(count > 0, "Subtitle contains no supported cues");
    Ok(output.into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_anime_and_unsupported_containers() {
        assert!(mime_type("episode.mp4", true).is_err());
        assert!(mime_type("episode.mkv", false).is_err());
        assert_eq!(mime_type("movie.MP4", false).unwrap(), "video/mp4");
        assert_eq!(mime_type("movie.webm", false).unwrap(), "video/webm");
    }

    #[test]
    fn converts_subtitles_and_rejects_invalid_timing() {
        let output = to_webvtt("1\r\n00:00:01,000 --> 00:00:03,000\r\nHello <world>\r\n").unwrap();
        assert_eq!(
            String::from_utf8(output).unwrap(),
            "WEBVTT\n\n00:00:01.000 --> 00:00:03.000\nHello &lt;world&gt;\n\n"
        );
        assert!(to_webvtt("00:00:03,000 --> 00:00:01,000\nInvalid").is_err());
        assert!(to_webvtt("00:61:03,000 --> 00:62:01,000\nInvalid").is_err());
    }
}
