use super::{Chapter, PageSet, Publication, ReaderSource};
use anyhow::{anyhow, Context, Result};
use reqwest::Client;
use serde::Deserialize;
use std::collections::HashMap;
use std::time::Duration;

const API_BASE: &str = "https://api.mangadex.org";
const USER_AGENT: &str = concat!(
    "flix/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/BabalolaBrainiac/flix)"
);
const JSON_LIMIT: usize = 4 * 1024 * 1024;
const PAGE_LIMIT: usize = 20 * 1024 * 1024; // 20 MiB per page
#[allow(dead_code)]
const CHAPTER_LIMIT: usize = 400 * 1024 * 1024; // 400 MiB per chapter

pub struct MangaDexClient {
    client: Client,
}

impl MangaDexClient {
    pub fn new() -> Result<Self> {
        let client = Client::builder()
            .use_rustls_tls()
            .min_tls_version(reqwest::tls::Version::TLS_1_3)
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(15))
            .user_agent(USER_AGENT)
            .build()
            .context("Failed to create the MangaDex client")?;
        Ok(Self { client })
    }

    /// Search for manga by title.
    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<Publication>> {
        let url = format!(
            "{API_BASE}/manga?title={}&limit={}&includes[]=cover_art",
            urlencoding(query),
            limit
        );
        let response = self
            .client
            .get(&url)
            .header("Accept", "application/json")
            .send()
            .await
            .context("MangaDex search request failed")?;
        let bytes = read_response(response, JSON_LIMIT, "search").await?;
        let list: MangaListResponse =
            serde_json::from_slice(&bytes).context("MangaDex returned invalid search JSON")?;
        Ok(list
            .data
            .into_iter()
            .map(|manga| {
                let title = manga
                    .attributes
                    .title
                    .get("en")
                    .or_else(|| manga.attributes.title.values().next())
                    .cloned()
                    .unwrap_or_default();
                let description = manga
                    .attributes
                    .description
                    .get("en")
                    .or_else(|| manga.attributes.description.values().next())
                    .cloned();
                let tags = manga
                    .attributes
                    .tags
                    .iter()
                    .filter_map(|tag| tag.attributes.name.get("en").cloned())
                    .collect();
                let cover_url = manga
                    .relationships
                    .iter()
                    .find(|rel| rel.rel_type == "cover_art")
                    .and_then(|rel| rel.attributes.as_ref())
                    .and_then(|attrs| attrs.file_name.as_ref())
                    .map(|file_name| {
                        format!(
                            "https://uploads.mangadex.org/covers/{}/{}",
                            manga.id, file_name
                        )
                    });
                Publication {
                    id: manga.id,
                    title: sanitize_text(&title, 300),
                    description: description.map(|d| sanitize_text(&d, 1000)),
                    year: manga.attributes.year,
                    status: manga.attributes.status.map(|s| sanitize_text(&s, 50)),
                    tags,
                    cover_url,
                    source: ReaderSource::MangaDex,
                }
            })
            .collect())
    }

    /// Get chapters for a manga, filtered by language.
    /// Returns (chapters, available_languages).
    pub async fn chapters(
        &self,
        manga_id: &str,
        language: &str,
    ) -> Result<(Vec<Chapter>, Vec<String>)> {
        uuid::Uuid::parse_str(manga_id).context("Invalid manga ID")?;
        // Fetch all chapters to determine available languages
        let url = format!(
            "{API_BASE}/manga/{manga_id}/feed?limit=500&order[volume]=asc&order[chapter]=asc"
        );
        let response = self
            .client
            .get(&url)
            .header("Accept", "application/json")
            .send()
            .await
            .context("MangaDex chapter feed request failed")?;
        let bytes = read_response(response, JSON_LIMIT, "chapter feed").await?;
        let feed: ChapterFeedResponse = serde_json::from_slice(&bytes)
            .context("MangaDex returned invalid chapter feed JSON")?;

        let mut available_languages: Vec<String> = feed
            .data
            .iter()
            .filter_map(|ch| ch.attributes.translated_language.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        available_languages.sort();

        let mut chapters: Vec<Chapter> = feed
            .data
            .into_iter()
            .filter(|ch| ch.attributes.external_url.is_none())
            .filter(|ch| {
                ch.attributes
                    .translated_language
                    .as_deref()
                    .is_some_and(|lang| lang == language)
            })
            .map(|ch| Chapter {
                id: ch.id,
                chapter: ch.attributes.chapter,
                volume: ch.attributes.volume,
                title: ch.attributes.title.map(|t| sanitize_text(&t, 300)),
                language: ch.attributes.translated_language.unwrap_or_default(),
                pages: ch.attributes.pages.unwrap_or(0),
                external_url: None,
            })
            .collect();

        // Sort by volume (numeric), then chapter (numeric)
        chapters.sort_by(|a, b| {
            let vol_a = a.volume.as_deref().and_then(|v| v.parse::<f64>().ok());
            let vol_b = b.volume.as_deref().and_then(|v| v.parse::<f64>().ok());
            vol_a
                .partial_cmp(&vol_b)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    let ch_a = a.chapter.as_deref().and_then(|c| c.parse::<f64>().ok());
                    let ch_b = b.chapter.as_deref().and_then(|c| c.parse::<f64>().ok());
                    ch_a.partial_cmp(&ch_b).unwrap_or(std::cmp::Ordering::Equal)
                })
        });

        Ok((chapters, available_languages))
    }

    /// Get page URLs for a chapter.
    pub async fn pages(&self, chapter_id: &str) -> Result<PageSet> {
        uuid::Uuid::parse_str(chapter_id).context("Invalid chapter ID")?;
        let url = format!("{API_BASE}/at-home/server/{chapter_id}");
        let response = self
            .client
            .get(&url)
            .header("Accept", "application/json")
            .send()
            .await
            .context("MangaDex at-home request failed")?;
        let bytes = read_response(response, JSON_LIMIT, "at-home").await?;
        let at_home: AtHomeResponse =
            serde_json::from_slice(&bytes).context("MangaDex returned invalid at-home JSON")?;

        let base_url = reqwest::Url::parse(&at_home.base_url)
            .context("MangaDex returned an invalid base URL")?;
        if !approved_mangadex_url(&base_url) {
            return Err(anyhow!("MangaDex returned a disallowed base URL"));
        }

        let page_urls = at_home
            .chapter
            .data
            .iter()
            .map(|file| {
                format!(
                    "{}/data/{}/{}",
                    at_home.base_url, at_home.chapter.hash, file
                )
            })
            .collect();

        Ok(PageSet {
            chapter_id: chapter_id.to_string(),
            page_urls,
        })
    }

    /// Download a page image. Returns the image bytes.
    pub async fn download_page(&self, url: &str) -> Result<Vec<u8>> {
        let parsed = reqwest::Url::parse(url).context("Invalid page URL")?;
        if !approved_mangadex_url(&parsed) {
            return Err(anyhow!("Page URL host is not in the MangaDex allowlist"));
        }

        let response = self
            .client
            .get(url)
            .send()
            .await
            .context("MangaDex page download failed")?;

        if !response.status().is_success() {
            return Err(anyhow!(
                "MangaDex page download failed with status {}",
                response.status()
            ));
        }

        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string();
        if !content_type.starts_with("image/") {
            return Err(anyhow!(
                "MangaDex page returned non-image content type: {content_type}"
            ));
        }

        read_response(response, PAGE_LIMIT, "page download").await
    }
}

/// Check that a URL points to an allowed MangaDex host.
pub fn approved_mangadex_url(url: &reqwest::Url) -> bool {
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port_or_known_default() == Some(443)
        && url.host_str().is_some_and(|host| {
            let host = host.to_ascii_lowercase();
            host == "api.mangadex.org"
                || host == "uploads.mangadex.org"
                || host.ends_with(".mangadex.network")
                || host.ends_with(".mangadex.org")
        })
}

/// Sanitize text by stripping control characters and bidi overrides.
pub fn sanitize_text(value: &str, limit: usize) -> String {
    value
        .chars()
        .filter(|c| {
            !c.is_control() && !matches!(*c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(limit)
        .collect()
}

fn urlencoding(value: &str) -> String {
    percent_encoding::utf8_percent_encode(value, percent_encoding::NON_ALPHANUMERIC).to_string()
}

async fn read_response(
    mut response: reqwest::Response,
    limit: usize,
    operation: &str,
) -> Result<Vec<u8>> {
    if !response.status().is_success() {
        return Err(anyhow!(
            "MangaDex {operation} failed with status {}",
            response.status()
        ));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .with_context(|| format!("Failed to read the MangaDex {operation} response"))?
    {
        if bytes.len().saturating_add(chunk.len()) > limit {
            return Err(anyhow!("MangaDex {operation} response is too large"));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

#[derive(Deserialize)]
struct MangaListResponse {
    data: Vec<MangaData>,
}

#[derive(Deserialize)]
struct MangaData {
    id: String,
    attributes: MangaAttributes,
    #[serde(default)]
    relationships: Vec<Relationship>,
}

#[derive(Deserialize)]
struct MangaAttributes {
    title: HashMap<String, String>,
    #[serde(default)]
    description: HashMap<String, String>,
    year: Option<u32>,
    status: Option<String>,
    #[serde(default)]
    tags: Vec<Tag>,
}

#[derive(Deserialize)]
struct Tag {
    attributes: TagAttributes,
}

#[derive(Deserialize)]
struct TagAttributes {
    name: HashMap<String, String>,
}

#[derive(Deserialize)]
struct Relationship {
    #[serde(rename = "type")]
    rel_type: String,
    #[allow(dead_code)]
    id: String,
    attributes: Option<RelationshipAttributes>,
}

#[derive(Deserialize)]
struct RelationshipAttributes {
    #[serde(rename = "fileName")]
    file_name: Option<String>,
}

#[derive(Deserialize)]
struct ChapterFeedResponse {
    data: Vec<ChapterData>,
}

#[derive(Deserialize)]
struct ChapterData {
    id: String,
    attributes: ChapterAttributes,
}

#[derive(Deserialize)]
struct ChapterAttributes {
    chapter: Option<String>,
    volume: Option<String>,
    title: Option<String>,
    #[serde(rename = "translatedLanguage")]
    translated_language: Option<String>,
    pages: Option<usize>,
    #[serde(rename = "externalUrl")]
    external_url: Option<String>,
}

#[derive(Deserialize)]
struct AtHomeResponse {
    #[serde(rename = "baseUrl")]
    base_url: String,
    chapter: AtHomeChapter,
}

#[derive(Deserialize)]
struct AtHomeChapter {
    hash: String,
    data: Vec<String>,
}
