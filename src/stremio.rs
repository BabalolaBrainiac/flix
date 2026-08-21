use anyhow::{anyhow, Context, Result};
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use regex::Regex;
use reqwest::redirect::Policy;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

const CINEMETA_BASE: &str = "https://v3-cinemeta.strem.io/";
const ANIME_KITSU_BASE: &str = "https://anime-kitsu.strem.fun/";
const TORRENTIO_BASE: &str = "https://torrentio.strem.fun/";
const JSON_LIMIT: usize = 4 * 1024 * 1024;
const SUBTITLE_LIMIT: usize = 10 * 1024 * 1024;
const MAX_AUTOMATIC_4K_ATTEMPTS: usize = 3;
const PUBLIC_TRACKERS: &[&str] = &[
    "udp://tracker.opentrackr.org:1337/announce",
    "https://tracker.opentrackr.org:443/announce",
    "udp://open.stealth.si:80/announce",
    "udp://tracker.torrent.eu.org:451/announce",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogKind {
    Cinemeta,
    AnimeKitsu,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogItem {
    pub id: String,
    pub media_type: String,
    pub name: String,
    pub release_info: Option<String>,
    pub poster: Option<String>,
    pub kind: CatalogKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Episode {
    pub id: String,
    pub stream_id: String,
    pub imdb_id: Option<String>,
    pub title: Option<String>,
    pub season: u32,
    pub episode: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TorrentStream {
    pub info_hash: String,
    pub file_index: Option<usize>,
    pub file_name: Option<String>,
    pub name: String,
    pub title: String,
    pub seeders: Option<u64>,
    pub size: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SubtitleContext {
    pub series_id: String,
    pub selected_episode: u32,
}

#[derive(Debug, Clone)]
pub struct StremioSubtitle {
    pub id: String,
    pub url: reqwest::Url,
}

#[derive(Default)]
pub struct SearchResults {
    pub items: Vec<CatalogItem>,
    pub notes: Vec<String>,
}

#[derive(Deserialize)]
struct CatalogEnvelope {
    #[serde(default)]
    metas: Vec<CatalogRecord>,
}

#[derive(Deserialize)]
struct CatalogRecord {
    id: String,
    #[serde(rename = "type")]
    media_type: String,
    name: String,
    #[serde(rename = "releaseInfo")]
    release_info: Option<String>,
    poster: Option<String>,
}

#[derive(Deserialize)]
struct MetaEnvelope {
    meta: MetaRecord,
}

#[derive(Deserialize)]
struct MetaRecord {
    #[serde(default)]
    videos: Vec<EpisodeRecord>,
}

#[derive(Deserialize)]
struct EpisodeRecord {
    id: String,
    title: Option<String>,
    season: Option<u32>,
    episode: Option<u32>,
    imdb_id: Option<String>,
    #[serde(rename = "imdbSeason")]
    imdb_season: Option<u32>,
    #[serde(rename = "imdbEpisode")]
    imdb_episode: Option<u32>,
}

#[derive(Deserialize)]
struct StreamEnvelope {
    #[serde(default)]
    streams: Vec<StreamRecord>,
}

#[derive(Deserialize)]
struct StreamRecord {
    #[serde(rename = "infoHash")]
    info_hash: Option<String>,
    #[serde(rename = "fileIdx")]
    file_index: Option<usize>,
    #[serde(default)]
    name: String,
    #[serde(default)]
    title: String,
    #[serde(rename = "behaviorHints", default)]
    behavior_hints: StreamBehaviorHints,
}

#[derive(Default, Deserialize)]
struct StreamBehaviorHints {
    filename: Option<String>,
}

#[derive(Deserialize)]
struct SubtitleEnvelope {
    #[serde(default)]
    subtitles: Vec<SubtitleRecord>,
}

#[derive(Deserialize)]
struct SubtitleRecord {
    id: String,
    lang: String,
    url: String,
}

pub fn parse_catalog_response(value: Value, kind: CatalogKind) -> Result<Vec<CatalogItem>> {
    let envelope: CatalogEnvelope =
        serde_json::from_value(value).context("Stremio returned an invalid catalog response")?;
    Ok(envelope
        .metas
        .into_iter()
        .filter(|record| {
            valid_content_id(&record.id)
                && matches!(record.media_type.as_str(), "movie" | "series" | "anime")
                && !record.name.trim().is_empty()
        })
        .map(|record| CatalogItem {
            id: record.id,
            media_type: record.media_type,
            name: sanitize_text(&record.name, 200),
            release_info: record.release_info.map(|value| sanitize_text(&value, 80)),
            poster: record.poster.map(|value| sanitize_text(&value, 500)),
            kind,
        })
        .collect())
}

pub fn parse_meta_response(value: Value) -> Result<Vec<Episode>> {
    let envelope: MetaEnvelope =
        serde_json::from_value(value).context("Stremio returned an invalid metadata response")?;
    let mut episodes: Vec<_> = envelope
        .meta
        .videos
        .into_iter()
        .filter_map(|record| {
            let stream_id = imdb_episode_stream_id(&record);
            let id = valid_content_id(&record.id).then_some(record.id)?;
            let stream_id = stream_id.unwrap_or_else(|| id.clone());
            Some(Episode {
                id,
                stream_id,
                imdb_id: record
                    .imdb_id
                    .as_deref()
                    .filter(|value| valid_content_id(value))
                    .map(|value| sanitize_text(value, 32)),
                title: record.title.map(|value| sanitize_text(&value, 200)),
                season: record.season?,
                episode: record.episode?,
            })
        })
        .collect();
    episodes.sort_by_key(|episode| (episode.season, episode.episode));
    Ok(episodes)
}

pub fn parse_stream_response(value: Value) -> Result<Vec<TorrentStream>> {
    let envelope: StreamEnvelope =
        serde_json::from_value(value).context("Stremio returned an invalid stream response")?;
    let seeder_pattern = Regex::new(r"(?i)(?:\x{1F464}|users?)\s*(\d+)")?;
    let size_pattern = Regex::new(r"(?i)(?:\x{1F4BE}|size)\s*([0-9.]+\s*(?:KB|MB|GB|TB))")?;
    let mut streams: Vec<_> = envelope
        .streams
        .into_iter()
        .filter_map(|record| {
            let info_hash = record.info_hash?;
            if !valid_info_hash(&info_hash) {
                return None;
            }
            let seeders = seeder_pattern
                .captures(&record.title)
                .and_then(|capture| capture.get(1))
                .and_then(|value| value.as_str().parse().ok());
            let size = size_pattern
                .captures(&record.title)
                .and_then(|capture| capture.get(1))
                .map(|value| value.as_str().to_string());
            Some(TorrentStream {
                info_hash: info_hash.to_ascii_lowercase(),
                file_index: record.file_index,
                file_name: record
                    .behavior_hints
                    .filename
                    .map(|value| sanitize_text(&value, 300)),
                name: sanitize_text(&record.name, 200),
                title: sanitize_text(&record.title, 500),
                seeders,
                size,
            })
        })
        .collect();
    streams.sort_by(|left, right| {
        stream_quality(right)
            .cmp(&stream_quality(left))
            .then_with(|| right.seeders.cmp(&left.seeders))
    });
    Ok(streams)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum StreamQuality {
    Other,
    Upscaled4k,
    FullHd,
    Native4k,
}

pub fn stream_quality_label(stream: &TorrentStream) -> &'static str {
    match stream_quality(stream) {
        StreamQuality::Native4k => "4K",
        StreamQuality::FullHd => "1080p",
        StreamQuality::Upscaled4k => "4K upscale",
        StreamQuality::Other => "other quality",
    }
}

pub fn automatic_1080p_fallback(streams: &[TorrentStream]) -> Option<&TorrentStream> {
    (streams
        .first()
        .is_some_and(|stream| stream_quality(stream) == StreamQuality::Native4k))
    .then(|| {
        streams
            .iter()
            .find(|stream| stream_quality(stream) == StreamQuality::FullHd)
    })
    .flatten()
}

pub fn automatic_playback_candidates(streams: &[TorrentStream]) -> Vec<&TorrentStream> {
    let Some(first) = streams.first() else {
        return Vec::new();
    };
    if stream_quality(first) != StreamQuality::Native4k {
        return vec![first];
    }

    let mut seen = HashSet::new();
    let mut candidates = Vec::new();
    for stream in streams
        .iter()
        .filter(|stream| stream_quality(stream) == StreamQuality::Native4k)
    {
        let key = (stream.info_hash.as_str(), stream.file_index);
        if seen.insert(key) {
            candidates.push(stream);
        }
        if candidates.len() == MAX_AUTOMATIC_4K_ATTEMPTS {
            break;
        }
    }
    if let Some(fallback) = automatic_1080p_fallback(streams) {
        candidates.push(fallback);
    }
    candidates
}

fn stream_quality(stream: &TorrentStream) -> StreamQuality {
    let description = format!(
        "{} {} {}",
        stream.name,
        stream.title,
        stream.file_name.as_deref().unwrap_or_default()
    )
    .to_ascii_lowercase();
    let is_4k = description.contains("2160p") || description.contains("4k");
    let is_upscale = description.contains("upscale") || description.contains("upscaled");
    if is_4k && !is_upscale {
        StreamQuality::Native4k
    } else if description.contains("1080p") {
        StreamQuality::FullHd
    } else if is_4k {
        StreamQuality::Upscaled4k
    } else {
        StreamQuality::Other
    }
}

pub fn build_magnet(info_hash: &str, display_name: Option<&str>) -> Result<String> {
    if !valid_info_hash(info_hash) {
        return Err(anyhow!("Stremio returned an invalid torrent info hash"));
    }
    let mut url = reqwest::Url::parse("magnet:?").context("Failed to build the magnet URL")?;
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("xt", &format!("urn:btih:{info_hash}"));
        if let Some(name) = display_name.filter(|name| !name.trim().is_empty()) {
            query.append_pair("dn", name);
        }
        for tracker in PUBLIC_TRACKERS {
            query.append_pair("tr", tracker);
        }
    }
    Ok(url.into())
}

pub fn parse_subtitle_response(value: Value) -> Result<Vec<StremioSubtitle>> {
    let envelope: SubtitleEnvelope =
        serde_json::from_value(value).context("Stremio returned an invalid subtitle response")?;
    Ok(envelope
        .subtitles
        .into_iter()
        .filter(|record| matches!(record.lang.as_str(), "en" | "eng"))
        .filter_map(|record| {
            let url = reqwest::Url::parse(&record.url).ok()?;
            approved_subtitle_url(&url).then_some(StremioSubtitle {
                id: sanitize_text(&record.id, 100),
                url,
            })
        })
        .collect())
}

pub async fn find_anime_subtitle(
    data_dir: &Path,
    info_hash: &str,
    file_index: usize,
    file_name: &str,
    context: &SubtitleContext,
) -> Result<Option<PathBuf>> {
    if !valid_content_id(&context.series_id) {
        return Err(anyhow!("The anime subtitle identifier is invalid"));
    }
    let episode = crate::meta::parse::parse(file_name)
        .episode
        .unwrap_or(context.selected_episode);
    let id = format!("{}:{episode}", context.series_id);
    let client = StremioClient::from_env()?;
    let Some(subtitle) = client.anime_subtitles(&id).await?.into_iter().next() else {
        return Ok(None);
    };
    Ok(Some(
        client
            .download_subtitle(data_dir, info_hash, file_index, &subtitle.url)
            .await?,
    ))
}

pub struct StremioClient {
    client: reqwest::Client,
    cinemeta_base: reqwest::Url,
    anime_base: reqwest::Url,
    stream_base: reqwest::Url,
}

impl StremioClient {
    pub fn from_env() -> Result<Self> {
        let client = reqwest::Client::builder()
            .use_rustls_tls()
            .min_tls_version(reqwest::tls::Version::TLS_1_2)
            .timeout(Duration::from_secs(15))
            .redirect(Policy::custom(|attempt| {
                if attempt.previous().len() >= 3 {
                    return attempt.stop();
                }
                if approved_redirect_url(attempt.url()) {
                    attempt.follow()
                } else {
                    attempt.stop()
                }
            }))
            .build()
            .context("Failed to create the Stremio client")?;
        Ok(Self {
            client,
            cinemeta_base: addon_base("FLIX_CINEMETA_URL", CINEMETA_BASE)?,
            anime_base: addon_base("FLIX_ANIME_KITSU_URL", ANIME_KITSU_BASE)?,
            stream_base: addon_base("FLIX_TORRENT_STREAM_URL", TORRENTIO_BASE)?,
        })
    }

    pub async fn search(&self, query: &str, anime_only: bool) -> SearchResults {
        let anime = self.catalog(
            &self.anime_base,
            "anime",
            "kitsu-anime-list",
            query,
            CatalogKind::AnimeKitsu,
        );
        if anime_only {
            return result_from_requests(vec![("Anime Kitsu", anime.await)]);
        }
        let movie = self.catalog(
            &self.cinemeta_base,
            "movie",
            "top",
            query,
            CatalogKind::Cinemeta,
        );
        let series = self.catalog(
            &self.cinemeta_base,
            "series",
            "top",
            query,
            CatalogKind::Cinemeta,
        );
        let (movie, series, anime) = tokio::join!(movie, series, anime);
        result_from_requests(vec![
            ("Cinemeta movie", movie),
            ("Cinemeta series", series),
            ("Anime Kitsu", anime),
        ])
    }

    pub async fn episodes(&self, item: &CatalogItem) -> Result<Vec<Episode>> {
        let base = match item.kind {
            CatalogKind::Cinemeta => &self.cinemeta_base,
            CatalogKind::AnimeKitsu => &self.anime_base,
        };
        let media_type = if item.kind == CatalogKind::AnimeKitsu {
            "series"
        } else {
            item.media_type.as_str()
        };
        let url = resource_url(base, "meta", media_type, &item.id)?;
        parse_meta_response(self.get_json(url, "metadata").await?)
    }

    pub async fn streams(&self, media_type: &str, id: &str) -> Result<Vec<TorrentStream>> {
        let stream_type = if id.starts_with("kitsu:") || id.contains(':') {
            "series"
        } else {
            media_type
        };
        let url = resource_url(&self.stream_base, "stream", stream_type, id)?;
        let mut last_error = None;
        for attempt in 0..3 {
            match self.get_json(url.clone(), "stream").await {
                Ok(value) => return parse_stream_response(value),
                Err(error) if attempt < 2 && is_transient_stream_error(&error) => {
                    last_error = Some(error);
                    tokio::time::sleep(Duration::from_millis(250)).await;
                }
                Err(error) => return Err(error),
            }
        }
        Err(last_error.expect("stream retry recorded an error"))
    }

    async fn anime_subtitles(&self, id: &str) -> Result<Vec<StremioSubtitle>> {
        let url = resource_url(&self.anime_base, "subtitles", "series", id)?;
        parse_subtitle_response(self.get_json(url, "subtitle").await?)
    }

    async fn download_subtitle(
        &self,
        data_dir: &Path,
        info_hash: &str,
        file_index: usize,
        url: &reqwest::Url,
    ) -> Result<PathBuf> {
        if !valid_info_hash(info_hash) || !approved_subtitle_url(url) {
            return Err(anyhow!("Stremio returned an invalid subtitle download"));
        }
        let subtitle_dir = data_dir.join("subtitles");
        let destination = subtitle_dir.join(format!(
            "{}.{}.stremio.en.srt",
            info_hash.to_ascii_lowercase(),
            file_index
        ));
        if tokio::fs::metadata(&destination)
            .await
            .is_ok_and(|metadata| metadata.is_file() && metadata.len() > 0)
        {
            return Ok(destination);
        }
        let response = self
            .client
            .get(url.clone())
            .send()
            .await
            .context("Stremio subtitle download failed")?;
        let bytes = read_response(response, SUBTITLE_LIMIT, "subtitle file").await?;
        if bytes.is_empty() {
            return Err(anyhow!("Stremio returned an empty subtitle file"));
        }
        tokio::fs::create_dir_all(&subtitle_dir)
            .await
            .context("Failed to create the subtitle directory")?;
        let partial = destination.with_extension("part");
        let result = write_file(&partial, &destination, &bytes).await;
        if result.is_err() {
            let _ = tokio::fs::remove_file(&partial).await;
        }
        result?;
        Ok(destination)
    }

    async fn catalog(
        &self,
        base: &reqwest::Url,
        media_type: &str,
        id: &str,
        query: &str,
        kind: CatalogKind,
    ) -> Result<Vec<CatalogItem>> {
        let encoded = utf8_percent_encode(query, NON_ALPHANUMERIC).to_string();
        let url = reqwest::Url::parse(&format!(
            "{}catalog/{media_type}/{id}/search={encoded}.json",
            base.as_str()
        ))
        .context("Failed to build the Stremio catalog URL")?;
        parse_catalog_response(self.get_json(url, "catalog").await?, kind)
    }

    async fn get_json(&self, url: reqwest::Url, operation: &str) -> Result<Value> {
        let response = self
            .client
            .get(url)
            .header("Accept", "application/json")
            .send()
            .await
            .with_context(|| format!("Stremio {operation} request failed"))?;
        let bytes = read_response(response, JSON_LIMIT, operation).await?;
        serde_json::from_slice(&bytes)
            .with_context(|| format!("Stremio returned invalid JSON for {operation}"))
    }
}

async fn read_response(
    mut response: reqwest::Response,
    limit: usize,
    operation: &str,
) -> Result<Vec<u8>> {
    if !response.status().is_success() {
        return Err(anyhow!(
            "Stremio {operation} failed with status {}",
            response.status()
        ));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .with_context(|| format!("Failed to read the Stremio {operation} response"))?
    {
        if bytes.len().saturating_add(chunk.len()) > limit {
            return Err(anyhow!("Stremio {operation} response is too large"));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

async fn write_file(partial: &Path, destination: &Path, bytes: &[u8]) -> Result<()> {
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

fn addon_base(name: &str, default: &str) -> Result<reqwest::Url> {
    let value = std::env::var(name).unwrap_or_else(|_| default.to_string());
    let mut url =
        reqwest::Url::parse(&value).with_context(|| format!("{name} is not a valid URL"))?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(anyhow!("{name} must be a plain HTTPS base URL"));
    }
    let path = url.path().trim_end_matches('/').to_string();
    url.set_path(&format!("{path}/"));
    Ok(url)
}

fn resource_url(
    base: &reqwest::Url,
    resource: &str,
    media_type: &str,
    id: &str,
) -> Result<reqwest::Url> {
    if !valid_content_id(id) || !matches!(media_type, "movie" | "series" | "anime") {
        return Err(anyhow!("Stremio returned an invalid content identifier"));
    }
    reqwest::Url::parse(&format!(
        "{}{resource}/{media_type}/{id}.json",
        base.as_str()
    ))
    .context("Failed to build the Stremio resource URL")
}

fn imdb_episode_stream_id(record: &EpisodeRecord) -> Option<String> {
    let imdb_id = record.imdb_id.as_deref()?;
    if !valid_content_id(imdb_id) || !imdb_id.starts_with("tt") {
        return None;
    }
    Some(format!(
        "{}:{}:{}",
        imdb_id, record.imdb_season?, record.imdb_episode?
    ))
}

fn is_transient_stream_error(error: &anyhow::Error) -> bool {
    let message = error.to_string();
    message.contains("status 500")
        || message.contains("status 502")
        || message.contains("status 503")
        || message.contains("status 504")
}

fn result_from_requests(requests: Vec<(&str, Result<Vec<CatalogItem>>)>) -> SearchResults {
    let mut result = SearchResults::default();
    for (name, request) in requests {
        match request {
            Ok(mut items) => result.items.append(&mut items),
            Err(error) => result.notes.push(format!("{name} search failed: {error}")),
        }
    }
    result
}

fn valid_content_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, ':' | '_' | '-')
        })
}

fn valid_info_hash(value: &str) -> bool {
    (value.len() == 40 && value.chars().all(|character| character.is_ascii_hexdigit()))
        || (value.len() == 32
            && value
                .chars()
                .all(|character| character.is_ascii_alphanumeric()))
}

fn approved_subtitle_url(url: &reqwest::Url) -> bool {
    url.scheme() == "https"
        && url.host_str().is_some_and(|host| {
            host.eq_ignore_ascii_case("strem.io")
                || host.to_ascii_lowercase().ends_with(".strem.io")
        })
}

fn approved_redirect_url(url: &reqwest::Url) -> bool {
    url.scheme() == "https"
        && url.host_str().is_some_and(|host| {
            let host = host.to_ascii_lowercase();
            host == "strem.io"
                || host.ends_with(".strem.io")
                || host == "strem.fun"
                || host.ends_with(".strem.fun")
        })
}

fn sanitize_text(value: &str, limit: usize) -> String {
    value
        .chars()
        .filter(|character| {
            !character.is_control()
                && !matches!(
                    *character,
                    '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'
                )
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(limit)
        .collect()
}
