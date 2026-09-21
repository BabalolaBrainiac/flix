//! Poster images through a local, signed, cached proxy.
//!
//! Lists and status boards show many posters at once. Each poster came from a
//! third-party host on every view. The proxy loads a poster once, keeps it on
//! disk, and serves it from `/api/poster`. Lists then work offline and make
//! no repeat requests to the poster hosts.
//!
//! Only a fixed set of image hosts is allowed. Without that limit, the proxy
//! would let any local page make this computer fetch any address.

use crate::desktop::signed_url::UrlSigner;
use anyhow::{anyhow, Context, Result};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::sync::Semaphore;

/// A host is allowed when it equals one of these names or ends with `.` and
/// one of them.
const ALLOWED_HOSTS: &[&str] = &[
    "metahub.space",
    "kitsu.app",
    "kitsu.io",
    "media-amazon.com",
    "image.tmdb.org",
    "fanart.tv",
];

const MAX_POSTER_BYTES: usize = 2 * 1024 * 1024;
const FETCH_TIMEOUT: Duration = Duration::from_secs(8);
const MAX_REDIRECTS: usize = 3;
// Poster loads must never crowd out playback traffic.
const MAX_CONCURRENT_FETCHES: usize = 4;
const TRIM_EVERY_WRITES: usize = 50;
const POSTER_PATH: &str = "/api/poster";

/// A signed poster URL stays the same inside one window, so the browser can
/// reuse its own cache. It stays valid for between one and two windows.
const SIGNATURE_WINDOW_SECONDS: u64 = 6 * 60 * 60;

fn host_is_allowed(host: &str) -> bool {
    ALLOWED_HOSTS
        .iter()
        .any(|allowed| host == *allowed || host.ends_with(&format!(".{allowed}")))
}

/// True for an `https` address on an allowed host, with no login, no port.
pub fn is_allowed_poster_url(raw: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(raw) else {
        return false;
    };
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && url.host_str().is_some_and(host_is_allowed)
}

fn scope(url: &str) -> String {
    format!("poster|{url}")
}

fn expiry_for(now: u64) -> u64 {
    (now / SIGNATURE_WINDOW_SECONDS + 2) * SIGNATURE_WINDOW_SECONDS
}

fn encode(value: &str) -> String {
    percent_encoding::utf8_percent_encode(value, percent_encoding::NON_ALPHANUMERIC).to_string()
}

/// The signed local address for a poster. An address that is not on an
/// allowed host stays as it is, so the image still loads from its own host.
pub fn proxied_poster_url(signer: &UrlSigner, upstream: &str, now: u64) -> String {
    if !is_allowed_poster_url(upstream) {
        return upstream.to_string();
    }
    let expires_at = expiry_for(now);
    let signature = signer.sign(&scope(upstream), expires_at);
    format!(
        "{POSTER_PATH}?u={}&exp={expires_at}&sig={signature}",
        encode(upstream)
    )
}

/// True when `signature` is valid for this poster address and time.
pub fn verify_poster_url(
    signer: &UrlSigner,
    upstream: &str,
    expires_at: u64,
    signature: &str,
    now: u64,
) -> bool {
    signer.verify(&scope(upstream), expires_at, signature, now)
}

/// Turns a proxied address back into the upstream address. Clients send back
/// the poster they received. Storing the signed address would break it after
/// a restart. Any other value comes back unchanged.
pub fn original_poster_url(value: &str) -> String {
    if !value.starts_with(POSTER_PATH) {
        return value.to_string();
    }
    reqwest::Url::parse(&format!("http://local{value}"))
        .ok()
        .and_then(|url| {
            url.query_pairs()
                .find(|(key, _)| key == "u")
                .map(|(_, upstream)| upstream.into_owned())
        })
        .unwrap_or_else(|| value.to_string())
}

/// The image type from the first bytes, or `None` when the data is not an
/// image that a page can show. It stops the cache from storing a web page.
pub fn sniff_image_mime(bytes: &[u8]) -> Option<&'static str> {
    match bytes {
        [0xff, 0xd8, 0xff, ..] => Some("image/jpeg"),
        [0x89, b'P', b'N', b'G', ..] => Some("image/png"),
        [b'G', b'I', b'F', b'8', ..] => Some("image/gif"),
        _ if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" => {
            Some("image/webp")
        }
        _ => None,
    }
}

pub struct PosterCache {
    dir: PathBuf,
    client: reqwest::Client,
    permits: Semaphore,
    writes: AtomicUsize,
}

impl PosterCache {
    pub fn new(data_dir: &Path) -> Result<Self> {
        let redirects = reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= MAX_REDIRECTS {
                attempt.error("Too many redirects")
            } else if is_allowed_poster_url(attempt.url().as_str()) {
                attempt.follow()
            } else {
                attempt.stop()
            }
        });
        let client = reqwest::Client::builder()
            .use_rustls_tls()
            .min_tls_version(reqwest::tls::Version::TLS_1_2)
            .timeout(FETCH_TIMEOUT)
            .redirect(redirects)
            .build()
            .context("Failed to create the poster client")?;
        Ok(Self {
            dir: data_dir.join("posters"),
            client,
            permits: Semaphore::new(MAX_CONCURRENT_FETCHES),
            writes: AtomicUsize::new(0),
        })
    }

    fn path_for(&self, url: &str) -> PathBuf {
        let digest = Sha256::digest(url.as_bytes());
        let name: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
        self.dir.join(name)
    }

    async fn read_cached(&self, url: &str) -> Option<(Vec<u8>, &'static str)> {
        let bytes = tokio::fs::read(self.path_for(url)).await.ok()?;
        let mime = sniff_image_mime(&bytes)?;
        Some((bytes, mime))
    }

    /// The poster image and its type. It comes from disk when stored, and
    /// from the poster host on the first request.
    pub async fn get(&self, url: &str) -> Result<(Vec<u8>, &'static str)> {
        if !is_allowed_poster_url(url) {
            return Err(anyhow!("Poster host is not allowed"));
        }
        if let Some(hit) = self.read_cached(url).await {
            return Ok(hit);
        }
        let _permit = self.permits.acquire().await?;
        // Another request for the same poster may have finished while this one waited.
        if let Some(hit) = self.read_cached(url).await {
            return Ok(hit);
        }
        let bytes = self.download(url).await?;
        let mime = sniff_image_mime(&bytes).ok_or_else(|| anyhow!("Poster is not an image"))?;
        self.store(url, &bytes).await?;
        Ok((bytes, mime))
    }

    async fn download(&self, url: &str) -> Result<Vec<u8>> {
        let mut response = self.client.get(url).send().await?;
        if !response.status().is_success() {
            return Err(anyhow!("Poster host returned {}", response.status()));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            if bytes.len() + chunk.len() > MAX_POSTER_BYTES {
                return Err(anyhow!("Poster is too large"));
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    }

    async fn store(&self, url: &str, bytes: &[u8]) -> Result<()> {
        tokio::fs::create_dir_all(&self.dir).await?;
        let path = self.path_for(url);
        let partial = path.with_extension("partial");
        tokio::fs::write(&partial, bytes).await?;
        tokio::fs::rename(&partial, &path).await?;
        if self.writes.fetch_add(1, Ordering::Relaxed) % TRIM_EVERY_WRITES == TRIM_EVERY_WRITES - 1
        {
            let dir = self.dir.clone();
            tokio::task::spawn_blocking(move || {
                crate::storage::trim_cache(
                    &dir,
                    crate::storage::POSTER_CACHE_LIMIT,
                    crate::storage::CACHE_AGE,
                );
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_only_https_addresses_on_the_listed_hosts() {
        assert!(is_allowed_poster_url(
            "https://images.metahub.space/poster/small/tt1/img"
        ));
        assert!(is_allowed_poster_url(
            "https://media.kitsu.app/anime/1/poster.jpg"
        ));
        assert!(is_allowed_poster_url("https://metahub.space/a.jpg"));
    }

    #[test]
    fn refuses_other_hosts_schemes_logins_and_ports() {
        for url in [
            "http://images.metahub.space/a.jpg",
            "https://evilmetahub.space/a.jpg",
            "https://metahub.space.evil.example/a.jpg",
            "https://user:pass@images.metahub.space/a.jpg",
            "https://images.metahub.space:8443/a.jpg",
            "https://127.0.0.1/a.jpg",
            "https://localhost/a.jpg",
            "file:///etc/passwd",
            "not a url",
        ] {
            assert!(!is_allowed_poster_url(url), "{url} must be refused");
        }
    }

    #[test]
    fn recognises_the_common_image_types_and_nothing_else() {
        assert_eq!(
            sniff_image_mime(&[0xff, 0xd8, 0xff, 0xe0]),
            Some("image/jpeg")
        );
        assert_eq!(sniff_image_mime(b"\x89PNG\r\n\x1a\n"), Some("image/png"));
        assert_eq!(sniff_image_mime(b"GIF89a"), Some("image/gif"));
        assert_eq!(
            sniff_image_mime(b"RIFF\0\0\0\0WEBPVP8 "),
            Some("image/webp")
        );
        assert_eq!(sniff_image_mime(b"<html>not an image</html>"), None);
        assert_eq!(sniff_image_mime(b""), None);
    }

    #[test]
    fn a_proxied_address_verifies_and_round_trips_to_the_original() {
        let signer = UrlSigner::new();
        let upstream = "https://images.metahub.space/poster/small/tt1/img?x=1&y=2";
        let now = 1_000_000;
        let proxied = proxied_poster_url(&signer, upstream, now);
        assert!(proxied.starts_with("/api/poster?u="));
        assert_eq!(original_poster_url(&proxied), upstream);

        let parsed = reqwest::Url::parse(&format!("http://local{proxied}")).unwrap();
        let pair = |name: &str| {
            parsed
                .query_pairs()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.into_owned())
                .unwrap()
        };
        let (expires_at, signature) = (pair("exp").parse::<u64>().unwrap(), pair("sig"));
        assert!(verify_poster_url(
            &signer, upstream, expires_at, &signature, now
        ));
        assert!(!verify_poster_url(
            &signer,
            "https://kitsu.app/other.jpg",
            expires_at,
            &signature,
            now
        ));
        assert!(!verify_poster_url(
            &signer,
            upstream,
            expires_at,
            &signature,
            expires_at + 1
        ));
    }

    #[test]
    fn the_signed_address_is_stable_inside_one_window() {
        let signer = UrlSigner::new();
        let upstream = "https://images.metahub.space/a.jpg";
        let a = proxied_poster_url(&signer, upstream, 1_000);
        let b = proxied_poster_url(&signer, upstream, 2_000);
        assert_eq!(a, b);
        let later = proxied_poster_url(&signer, upstream, SIGNATURE_WINDOW_SECONDS + 5);
        assert_ne!(a, later);
    }

    #[test]
    fn an_address_on_an_unlisted_host_is_left_alone() {
        let signer = UrlSigner::new();
        let other = "https://cdn.example.org/a.jpg";
        assert_eq!(proxied_poster_url(&signer, other, 1), other);
        assert_eq!(original_poster_url(other), other);
    }

    #[tokio::test]
    async fn serves_a_stored_poster_without_any_network_request() {
        let dir = tempfile::tempdir().unwrap();
        let cache = PosterCache::new(dir.path()).unwrap();
        let url = "https://images.metahub.space/poster/small/tt1/img";
        cache.store(url, b"\x89PNG\r\n\x1a\nrest").await.unwrap();
        let (bytes, mime) = cache.get(url).await.unwrap();
        assert_eq!(mime, "image/png");
        assert!(bytes.starts_with(b"\x89PNG"));
    }

    #[tokio::test]
    async fn refuses_a_host_that_is_not_allowed_before_any_request() {
        let dir = tempfile::tempdir().unwrap();
        let cache = PosterCache::new(dir.path()).unwrap();
        assert!(cache.get("https://internal.example/a.jpg").await.is_err());
        assert!(cache.get("https://127.0.0.1/a.jpg").await.is_err());
    }
}
