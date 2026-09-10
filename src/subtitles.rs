use crate::playback::MediaRef;
use anyhow::{anyhow, Context, Result};
use reqwest::redirect::Policy;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

const API_BASE: &str = "https://api.opensubtitles.com/api/v1";
const JSON_LIMIT: usize = 2 * 1024 * 1024;
const SUBTITLE_LIMIT: usize = 10 * 1024 * 1024;
static LOGIN_REJECTED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubtitleQuery {
    pub title: String,
    pub season: Option<u32>,
    pub episode: Option<u32>,
    pub language: String,
}

impl SubtitleQuery {
    pub fn for_file(file_name: &str, language: &str) -> Self {
        let parsed = crate::meta::parse::parse(file_name);
        Self {
            title: parsed.title,
            season: parsed.season,
            episode: parsed.episode,
            language: language.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubtitleResult {
    pub file_id: u64,
    pub file_name: Option<String>,
    pub release: Option<String>,
    pub download_count: u64,
}

pub struct SubtitleOptions {
    /// Subtitle files already on disk for this video, most preferred first.
    pub cached: Vec<PathBuf>,
    pub results: Vec<SubtitleResult>,
}

#[derive(Deserialize)]
struct SearchEnvelope {
    #[serde(default)]
    data: Vec<SearchRecord>,
}

#[derive(Deserialize)]
struct SearchRecord {
    attributes: SearchAttributes,
}

#[derive(Deserialize)]
struct SearchAttributes {
    language: Option<String>,
    release: Option<String>,
    #[serde(default)]
    download_count: u64,
    #[serde(default)]
    files: Vec<SubtitleFile>,
}

#[derive(Deserialize)]
struct SubtitleFile {
    file_id: Option<u64>,
    file_name: Option<String>,
}

pub fn parse_search_response(value: Value) -> Result<Vec<SubtitleResult>> {
    let envelope: SearchEnvelope = serde_json::from_value(value)
        .context("OpenSubtitles returned an invalid search response")?;
    Ok(envelope
        .data
        .into_iter()
        .filter_map(|record| {
            if record.attributes.language.as_deref() != Some("en") {
                return None;
            }
            let file = record.attributes.files.into_iter().next()?;
            Some(SubtitleResult {
                file_id: file.file_id?,
                file_name: file.file_name.map(|value| sanitize_label(&value)),
                release: record
                    .attributes
                    .release
                    .map(|value| sanitize_label(&value)),
                download_count: record.attributes.download_count,
            })
        })
        .collect())
}

pub async fn english_subtitle_options(
    data_dir: &Path,
    info_hash: &str,
    file_index: usize,
    file_name: &str,
) -> Result<SubtitleOptions> {
    let cached = cached_english_subtitles(data_dir, info_hash, file_index).await?;
    if !cached.is_empty() {
        return Ok(SubtitleOptions {
            cached,
            results: Vec::new(),
        });
    }
    let Some(client) = OpenSubtitlesClient::from_env(data_dir)? else {
        return Ok(SubtitleOptions {
            cached: Vec::new(),
            results: Vec::new(),
        });
    };
    let query = SubtitleQuery::for_file(file_name, "en");
    Ok(SubtitleOptions {
        cached: Vec::new(),
        results: client.search(&query).await?,
    })
}

/// Lists every English subtitle already downloaded for one torrent video.
pub async fn cached_english_subtitles(
    data_dir: &Path,
    info_hash: &str,
    file_index: usize,
) -> Result<Vec<PathBuf>> {
    scan_cached_subtitles(&data_dir.join("subtitles"), info_hash, file_index).await
}

pub async fn download_english_subtitle(
    data_dir: &Path,
    info_hash: &str,
    file_index: usize,
    result: &SubtitleResult,
) -> Result<PathBuf> {
    let client =
        OpenSubtitlesClient::from_env(data_dir)?.context("OpenSubtitles is not configured")?;
    client.download(info_hash, file_index, result.file_id).await
}

pub async fn find_english_subtitle(
    data_dir: &Path,
    info_hash: &str,
    file_index: usize,
    file_name: &str,
) -> Result<Option<PathBuf>> {
    let Some(client) = OpenSubtitlesClient::from_env(data_dir)? else {
        return Ok(None);
    };
    client
        .find_and_download(info_hash, file_index, file_name)
        .await
}

pub fn is_configured() -> bool {
    gateway_is_configured()
        || std::env::var("OPENSUBTITLES_API_KEY").is_ok_and(|value| !value.trim().is_empty())
}

pub fn gateway_is_configured() -> bool {
    gateway_base_url().is_some()
        && crate::desktop::credentials::get_device_token()
            .ok()
            .flatten()
            .is_some()
}

pub async fn resolve_gateway_english_subtitle(
    data_dir: &Path,
    media: &MediaRef,
    info_hash: &str,
    file_index: usize,
) -> Result<Option<PathBuf>> {
    let mut lookup = GatewaySubtitleLookup::from_media(media);
    if lookup.is_none() {
        let catalog_id = match media {
            MediaRef::Movie { catalog_id, .. } | MediaRef::Episode { catalog_id, .. } => {
                catalog_id.as_str()
            }
        };
        if catalog_id.starts_with("kitsu:") {
            if let Some(imdb_id) = resolve_kitsu_imdb_id(catalog_id).await {
                let (season, episode) = match media {
                    MediaRef::Movie { .. } => (None, None),
                    MediaRef::Episode { season, episode, .. } => (Some(*season), Some(*episode)),
                };
                lookup = Some(GatewaySubtitleLookup {
                    imdb_id,
                    season,
                    episode,
                });
            }
        }
    }
    let Some(request) = lookup else {
        return Ok(None);
    };
    let Some(device_token) = crate::desktop::credentials::get_device_token()? else {
        return Ok(None);
    };
    let Some(base_url) = gateway_base_url() else {
        return Ok(None);
    };

    let client = reqwest::Client::builder()
        .use_rustls_tls()
        .min_tls_version(reqwest::tls::Version::TLS_1_2)
        .timeout(Duration::from_secs(15))
        .build()
        .context("Failed to create the subtitle gateway client")?;
    let response = client
        .post(format!("{base_url}/v1/subtitles/resolve"))
        .header("Authorization", format!("Bearer {device_token}"))
        .header("X-Device-Token", &device_token)
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await
        .context("Subtitle gateway request failed")?;

    if !response.status().is_success() {
        return Err(anyhow!(
            "Subtitle gateway request failed with status {}",
            response.status()
        ));
    }

    let resolved: GatewayResolveResponse = response
        .json()
        .await
        .context("Subtitle gateway returned an invalid response")?;

    let GatewayResolveResponse::Matched {
        file_name,
        content,
        format,
        ..
    } = resolved
    else {
        return Ok(None);
    };

    if format != "srt" {
        return Err(anyhow!(
            "Subtitle gateway returned an unsupported subtitle format"
        ));
    }

    let subtitle_dir = data_dir.join("subtitles");
    tokio::fs::create_dir_all(&subtitle_dir)
        .await
        .context("Failed to create the subtitle directory")?;
    let destination = gateway_subtitle_destination_from_dir(&subtitle_dir, info_hash, file_index)?;
    if existing_file(&destination).await? {
        return Ok(Some(destination));
    }

    let partial = destination.with_extension("part");
    let bytes = content.into_bytes();
    if bytes.is_empty() || bytes.len() > SUBTITLE_LIMIT {
        return Err(anyhow!(
            "Subtitle gateway returned an invalid subtitle file"
        ));
    }
    let result = write_subtitle(&partial, &destination, &bytes).await;
    if result.is_err() {
        let _ = tokio::fs::remove_file(&partial).await;
    }
    result?;

    if let Some(name) = file_name {
        tracing::debug!(
            "Downloaded subtitle from gateway: {}",
            sanitize_label(&name)
        );
    }

    Ok(Some(destination))
}

fn extract_imdb_id(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if let Some(pos) = trimmed.find("tt") {
        let candidate = &trimmed[pos..];
        let digits: String = candidate
            .chars()
            .skip(2)
            .take_while(|c| c.is_ascii_digit())
            .collect();
        if digits.len() >= 5 {
            return Some(format!("tt{digits}"));
        }
    }
    None
}

async fn resolve_kitsu_imdb_id(kitsu_id: &str) -> Option<String> {
    let client = reqwest::Client::builder()
        .use_rustls_tls()
        .timeout(Duration::from_secs(4))
        .build()
        .ok()?;
    for media_type in ["series", "anime", "movie"] {
        let url = format!("https://anime-kitsu.strem.fun/meta/{media_type}/{kitsu_id}.json");
        if let Ok(response) = client.get(&url).send().await {
            if response.status().is_success() {
                if let Ok(value) = response.json::<serde_json::Value>().await {
                    if let Some(id) = value
                        .get("meta")
                        .and_then(|m| m.get("imdb_id"))
                        .and_then(|i| i.as_str())
                    {
                        if let Some(extracted) = extract_imdb_id(id) {
                            return Some(extracted);
                        }
                    }
                }
            }
        }
    }
    None
}

#[derive(Serialize)]
struct GatewaySubtitleLookup {
    imdb_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    season: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    episode: Option<u32>,
}

impl GatewaySubtitleLookup {
    fn from_media(media: &MediaRef) -> Option<Self> {
        match media {
            MediaRef::Movie { catalog_id, .. } => {
                let imdb_id = extract_imdb_id(catalog_id)?;
                Some(Self {
                    imdb_id,
                    season: None,
                    episode: None,
                })
            }
            MediaRef::Episode {
                imdb_id,
                stream_id,
                season,
                episode,
                ..
            } => {
                let raw_id = imdb_id.as_deref().unwrap_or(stream_id);
                let imdb_id = extract_imdb_id(raw_id)?;
                Some(Self {
                    imdb_id,
                    season: Some(*season),
                    episode: Some(*episode),
                })
            }
        }
    }
}

#[derive(Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum GatewayResolveResponse {
    Matched {
        #[serde(rename = "file_id")]
        _file_id: u64,
        file_name: Option<String>,
        content: String,
        format: String,
    },
    NoMatch {
        #[serde(rename = "reason")]
        _reason: String,
    },
}

struct OpenSubtitlesClient {
    client: reqwest::Client,
    api_key: String,
    user_agent: String,
    subtitle_dir: PathBuf,
}

impl OpenSubtitlesClient {
    fn from_env(data_dir: &Path) -> Result<Option<Self>> {
        let Ok(api_key) = std::env::var("OPENSUBTITLES_API_KEY") else {
            return Ok(None);
        };
        if api_key.trim().is_empty() {
            return Ok(None);
        }
        let user_agent = std::env::var("OPENSUBTITLES_USER_AGENT")
            .unwrap_or_else(|_| format!("Flix v{}", env!("CARGO_PKG_VERSION")));
        let client = reqwest::Client::builder()
            .use_rustls_tls()
            .min_tls_version(reqwest::tls::Version::TLS_1_3)
            .timeout(Duration::from_secs(15))
            .redirect(Policy::custom(|attempt| {
                if attempt.previous().len() >= 3 {
                    return attempt.stop();
                }
                if is_approved_download_url(attempt.url()) {
                    attempt.follow()
                } else {
                    attempt.stop()
                }
            }))
            .build()
            .context("Failed to create the OpenSubtitles client")?;
        Ok(Some(Self {
            client,
            api_key,
            user_agent,
            subtitle_dir: data_dir.join("subtitles"),
        }))
    }

    async fn find_and_download(
        &self,
        info_hash: &str,
        file_index: usize,
        file_name: &str,
    ) -> Result<Option<PathBuf>> {
        let cached = scan_cached_subtitles(&self.subtitle_dir, info_hash, file_index).await?;
        if let Some(path) = cached.into_iter().next() {
            return Ok(Some(path));
        }
        let query = SubtitleQuery::for_file(file_name, "en");
        let Some(result) = self.search(&query).await?.into_iter().next() else {
            return Ok(None);
        };
        Ok(Some(
            self.download(info_hash, file_index, result.file_id).await?,
        ))
    }

    async fn download(&self, info_hash: &str, file_index: usize, file_id: u64) -> Result<PathBuf> {
        let destination =
            subtitle_destination_from_dir(&self.subtitle_dir, info_hash, file_index, file_id)?;
        if existing_file(&destination).await? {
            return Ok(destination);
        }
        let token = self.login().await?;
        let link = self.download_link(file_id, &token).await?;
        self.download_file(&link, &destination).await?;
        Ok(destination)
    }

    async fn search(&self, query: &SubtitleQuery) -> Result<Vec<SubtitleResult>> {
        let mut parameters = vec![
            ("query", query.title.clone()),
            ("languages", query.language.clone()),
            ("order_by", "download_count".to_string()),
            ("order_dir", "desc".to_string()),
        ];
        if let Some(season) = query.season {
            parameters.push(("season_number", season.to_string()));
        }
        if let Some(episode) = query.episode {
            parameters.push(("episode_number", episode.to_string()));
        }
        let response = self
            .headers(self.client.get(format!("{API_BASE}/subtitles")))
            .query(&parameters)
            .send()
            .await
            .context("OpenSubtitles search failed")?;
        let value = read_json(response, JSON_LIMIT, "search").await?;
        let mut results = parse_search_response(value)?;
        results.retain(|result| result.file_id > 0);
        Ok(results)
    }

    async fn login(&self) -> Result<String> {
        if LOGIN_REJECTED.load(Ordering::Relaxed) {
            return Err(anyhow!(
                "OpenSubtitles login is disabled for this run after a 401 response"
            ));
        }
        let username = required_env("OPENSUBTITLES_USERNAME")?;
        let password = required_env("OPENSUBTITLES_PASSWORD")?;
        let response = self
            .headers(self.client.post(format!("{API_BASE}/login")))
            .json(&LoginRequest {
                username: &username,
                password: &password,
            })
            .send()
            .await
            .context("OpenSubtitles login failed")?;
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            LOGIN_REJECTED.store(true, Ordering::Relaxed);
            return Err(anyhow!(
                "OpenSubtitles login failed with status 401 Unauthorized"
            ));
        }
        let value = read_json(response, JSON_LIMIT, "login").await?;
        let login: LoginResponse = serde_json::from_value(value)
            .context("OpenSubtitles returned an invalid login response")?;
        if login.token.is_empty() {
            return Err(anyhow!("OpenSubtitles login returned no token"));
        }
        Ok(login.token)
    }

    async fn download_link(&self, file_id: u64, token: &str) -> Result<reqwest::Url> {
        let response = self
            .headers(self.client.post(format!("{API_BASE}/download")))
            .bearer_auth(token)
            .json(&DownloadRequest { file_id })
            .send()
            .await
            .context("OpenSubtitles download request failed")?;
        let value = read_json(response, JSON_LIMIT, "download request").await?;
        let download: DownloadResponse = serde_json::from_value(value)
            .context("OpenSubtitles returned an invalid download response")?;
        let url = reqwest::Url::parse(&download.link)
            .context("OpenSubtitles returned an invalid download URL")?;
        if !is_approved_download_url(&url) {
            return Err(anyhow!(
                "OpenSubtitles returned an unapproved download host"
            ));
        }
        Ok(url)
    }

    async fn download_file(&self, url: &reqwest::Url, destination: &Path) -> Result<()> {
        let response = self
            .client
            .get(url.clone())
            .send()
            .await
            .context("Subtitle file download failed")?;
        let bytes = read_bytes(response, SUBTITLE_LIMIT, "subtitle file").await?;
        if bytes.is_empty() {
            return Err(anyhow!("OpenSubtitles returned an empty subtitle file"));
        }
        tokio::fs::create_dir_all(&self.subtitle_dir)
            .await
            .context("Failed to create the subtitle directory")?;
        let partial = destination.with_extension("part");
        let result = write_subtitle(&partial, destination, &bytes).await;
        if result.is_err() {
            let _ = tokio::fs::remove_file(&partial).await;
        }
        result
    }

    fn headers(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        request
            .header("Api-Key", &self.api_key)
            .header("User-Agent", &self.user_agent)
            .header("Accept", "application/json")
    }
}

fn subtitle_destination_from_dir(
    subtitle_dir: &Path,
    info_hash: &str,
    file_index: usize,
    file_id: u64,
) -> Result<PathBuf> {
    let prefix = subtitle_prefix(info_hash, file_index)?;
    Ok(subtitle_dir.join(format!("{prefix}{file_id}.en.srt")))
}

fn gateway_subtitle_destination_from_dir(
    subtitle_dir: &Path,
    info_hash: &str,
    file_index: usize,
) -> Result<PathBuf> {
    let prefix = subtitle_prefix(info_hash, file_index)?;
    Ok(subtitle_dir.join(format!("{prefix}gateway.en.srt")))
}

/// Builds the shared name prefix of every subtitle file for one torrent video.
fn subtitle_prefix(info_hash: &str, file_index: usize) -> Result<String> {
    if info_hash.len() != 40 || !info_hash.chars().all(|value| value.is_ascii_hexdigit()) {
        return Err(anyhow!("Torrent info hash is invalid"));
    }
    Ok(format!(
        "{}.{}.",
        info_hash.to_ascii_lowercase(),
        file_index
    ))
}

/// Finds every non-empty English subtitle file that belongs to one torrent
/// video. The names of subtitles from OpenSubtitles and from Stremio share the
/// same prefix, so one scan collects both.
async fn scan_cached_subtitles(
    subtitle_dir: &Path,
    info_hash: &str,
    file_index: usize,
) -> Result<Vec<PathBuf>> {
    let prefix = subtitle_prefix(info_hash, file_index)?;
    let mut entries = match tokio::fs::read_dir(subtitle_dir).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error).context("Failed to read the subtitle cache"),
    };
    let mut found = Vec::new();
    while let Some(entry) = entries
        .next_entry()
        .await
        .context("Failed to read a subtitle cache entry")?
    {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if !name.starts_with(&prefix) || !name.ends_with(".en.srt") {
            continue;
        }
        let path = entry.path();
        if existing_file(&path).await? {
            found.push(path);
        }
    }
    found.sort();
    Ok(found)
}

async fn write_subtitle(partial: &Path, destination: &Path, bytes: &[u8]) -> Result<()> {
    use tokio::io::AsyncWriteExt;
    let mut file = tokio::fs::File::create(partial)
        .await
        .context("Failed to create the temporary subtitle file")?;
    file.write_all(bytes)
        .await
        .context("Failed to write the subtitle file")?;
    file.sync_all()
        .await
        .context("Failed to sync the subtitle file")?;
    tokio::fs::rename(partial, destination)
        .await
        .context("Failed to save the subtitle file")
}

#[derive(Serialize)]
struct LoginRequest<'a> {
    username: &'a str,
    password: &'a str,
}

#[derive(Deserialize)]
struct LoginResponse {
    token: String,
}

#[derive(Serialize)]
struct DownloadRequest {
    file_id: u64,
}

#[derive(Deserialize)]
struct DownloadResponse {
    link: String,
}

fn required_env(name: &str) -> Result<String> {
    let value = std::env::var(name)
        .with_context(|| format!("{name} is required when OpenSubtitles is enabled"))?;
    if value.trim().is_empty() {
        return Err(anyhow!("{name} cannot be empty"));
    }
    Ok(value)
}

async fn existing_file(path: &Path) -> Result<bool> {
    match tokio::fs::metadata(path).await {
        Ok(metadata) => Ok(metadata.is_file() && metadata.len() > 0),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error).context("Failed to inspect the subtitle cache"),
    }
}

async fn read_json(response: reqwest::Response, limit: usize, operation: &str) -> Result<Value> {
    let bytes = read_bytes(response, limit, operation).await?;
    serde_json::from_slice(&bytes)
        .with_context(|| format!("OpenSubtitles returned invalid JSON for {operation}"))
}

async fn read_bytes(response: reqwest::Response, limit: usize, operation: &str) -> Result<Vec<u8>> {
    if !response.status().is_success() {
        return Err(anyhow!(
            "OpenSubtitles {operation} failed with status {}",
            response.status()
        ));
    }
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        return Err(anyhow!("OpenSubtitles {operation} response is too large"));
    }
    let bytes = response
        .bytes()
        .await
        .with_context(|| format!("Failed to read the OpenSubtitles {operation} response"))?;
    if bytes.len() > limit {
        return Err(anyhow!("OpenSubtitles {operation} response is too large"));
    }
    Ok(bytes.to_vec())
}

fn is_approved_download_url(url: &reqwest::Url) -> bool {
    if url.scheme() != "https" {
        return false;
    }
    url.host_str().is_some_and(|host| {
        host.eq_ignore_ascii_case("opensubtitles.com")
            || host.to_ascii_lowercase().ends_with(".opensubtitles.com")
    })
}

fn sanitize_label(value: &str) -> String {
    value
        .chars()
        .filter(|character| {
            !character.is_control()
                && !matches!(
                    *character,
                    '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'
                )
        })
        .take(300)
        .collect()
}

fn gateway_base_url() -> Option<String> {
    Some(crate::desktop::gateway::resolve_base_url())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_imdb_id_from_raw_identifiers() {
        assert_eq!(extract_imdb_id("tt7124066"), Some("tt7124066".to_string()));
        assert_eq!(extract_imdb_id("tt7124066:1:7"), Some("tt7124066".to_string()));
        assert_eq!(extract_imdb_id("tt0111161"), Some("tt0111161".to_string()));
        assert_eq!(extract_imdb_id("kitsu:13274"), None);
        assert_eq!(extract_imdb_id("invalid"), None);
    }

    #[test]
    fn gateway_lookup_extracts_clean_imdb_id_from_episode_media() {
        let media = MediaRef::Episode {
            catalog_id: "kitsu:13274".to_string(),
            stream_id: "tt7124066:1:7".to_string(),
            imdb_id: None,
            season: 1,
            episode: 7,
            title: Some("New & Old".to_string()),
        };
        let lookup = GatewaySubtitleLookup::from_media(&media).unwrap();
        assert_eq!(lookup.imdb_id, "tt7124066");
        assert_eq!(lookup.season, Some(1));
        assert_eq!(lookup.episode, Some(7));
    }
}
