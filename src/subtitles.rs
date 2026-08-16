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
    pub cached: Option<PathBuf>,
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
    let destination = subtitle_destination(data_dir, info_hash, file_index)?;
    if existing_file(&destination).await? {
        return Ok(SubtitleOptions {
            cached: Some(destination),
            results: Vec::new(),
        });
    }
    let Some(client) = OpenSubtitlesClient::from_env(data_dir)? else {
        return Ok(SubtitleOptions {
            cached: None,
            results: Vec::new(),
        });
    };
    let query = SubtitleQuery::for_file(file_name, "en");
    Ok(SubtitleOptions {
        cached: None,
        results: client.search(&query).await?,
    })
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
    std::env::var("OPENSUBTITLES_API_KEY").is_ok_and(|value| !value.trim().is_empty())
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
        let destination = subtitle_destination_from_dir(&self.subtitle_dir, info_hash, file_index)?;
        if existing_file(&destination).await? {
            return Ok(Some(destination));
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
        let destination = subtitle_destination_from_dir(&self.subtitle_dir, info_hash, file_index)?;
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

fn subtitle_destination(data_dir: &Path, info_hash: &str, file_index: usize) -> Result<PathBuf> {
    subtitle_destination_from_dir(&data_dir.join("subtitles"), info_hash, file_index)
}

fn subtitle_destination_from_dir(
    subtitle_dir: &Path,
    info_hash: &str,
    file_index: usize,
) -> Result<PathBuf> {
    if info_hash.len() != 40 || !info_hash.chars().all(|value| value.is_ascii_hexdigit()) {
        return Err(anyhow!("Torrent info hash is invalid"));
    }
    Ok(subtitle_dir.join(format!(
        "{}.{}.en.srt",
        info_hash.to_ascii_lowercase(),
        file_index
    )))
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
