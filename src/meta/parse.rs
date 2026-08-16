use super::ParsedName;
use regex::Regex;
use std::path::Path;

pub fn parse(release_name: &str) -> ParsedName {
    let candidate = candidate_name(release_name);
    let year_re = Regex::new(r"(19|20)\d{2}").expect("valid year expression");
    let show_re = Regex::new(r"(?i)S(\d{1,2})E(\d{1,2})").expect("valid episode expression");
    let quality_re = Regex::new(r"(?i)(480|720|1080|2160)p|WEB[ ._-]?DL|BluRay|HDRip|DVDRip")
        .expect("valid quality expression");

    let year = year_re
        .find(&candidate)
        .and_then(|matched| matched.as_str().parse().ok());

    let mut season = None;
    let mut episode = None;

    if let Some(captures) = show_re.captures(&candidate) {
        season = captures
            .get(1)
            .and_then(|value| value.as_str().parse().ok());
        episode = captures
            .get(2)
            .and_then(|value| value.as_str().parse().ok());
    }

    let mut end_index = candidate.len();
    if let Some(matched) = year_re.find(&candidate) {
        end_index = end_index.min(matched.start());
    }
    if let Some(matched) = show_re.find(&candidate) {
        end_index = end_index.min(matched.start());
    }
    if let Some(matched) = quality_re.find(&candidate) {
        end_index = end_index.min(matched.start());
    }

    let title = candidate[..end_index]
        .replace(['.', '_', '-'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let title = if title.is_empty() {
        "Unknown".to_string()
    } else {
        title
    };

    ParsedName {
        title,
        year,
        season,
        episode,
    }
}

fn candidate_name(input: &str) -> String {
    if let Ok(url) = reqwest::Url::parse(input) {
        if url.scheme() == "magnet" {
            return url
                .query_pairs()
                .find_map(|(key, value)| (key == "dn").then(|| value.into_owned()))
                .unwrap_or_else(|| "Unknown".to_string());
        }
        if matches!(url.scheme(), "http" | "https") {
            return url
                .path_segments()
                .and_then(|mut segments| segments.next_back())
                .map(strip_extensions)
                .unwrap_or_else(|| "Unknown".to_string());
        }
    }
    Path::new(input)
        .file_name()
        .and_then(|name| name.to_str())
        .map(strip_extensions)
        .unwrap_or_else(|| input.to_string())
}

fn strip_extensions(name: &str) -> String {
    let mut value = name.to_string();
    for extension in ["torrent", "mp4", "mkv", "avi", "mov", "webm"] {
        let suffix = format!(".{extension}");
        if value.to_ascii_lowercase().ends_with(&suffix) {
            value.truncate(value.len() - suffix.len());
        }
    }
    value
}
