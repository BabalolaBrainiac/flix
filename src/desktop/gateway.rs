//! Resolves and stores the subtitle gateway base URL.
//!
//! A packaged build must reach the shared gateway without any environment
//! setup. Activation can point the build at a different gateway, and the
//! chosen URL must survive a restart. The resolution order is:
//!
//! 1. The `FLIX_GATEWAY_URL` environment variable. This is a development
//!    override.
//! 2. The URL that the last successful activation saved.
//! 3. The packaged default gateway.

use anyhow::{anyhow, Context, Result};
use std::path::PathBuf;

/// Gateway that packaged builds use when nothing else is configured.
pub const PACKAGED_GATEWAY_URL: &str = "https://gateway.babalola.dev";

/// Name of the file that holds the activated gateway URL.
const GATEWAY_URL_FILE: &str = "gateway_url";

/// Environment variable that overrides the gateway URL during development.
const GATEWAY_URL_ENV: &str = "FLIX_GATEWAY_URL";

/// Returns the gateway base URL that requests must use.
///
/// The result never ends with a slash.
pub fn resolve_base_url() -> String {
    if let Some(url) = env_override() {
        return url;
    }
    if let Some(url) = saved_base_url() {
        return url;
    }
    PACKAGED_GATEWAY_URL.to_string()
}

/// Returns the gateway URL from the environment override, if it is valid.
fn env_override() -> Option<String> {
    let raw = std::env::var(GATEWAY_URL_ENV).ok()?;
    match normalize_gateway_url(&raw) {
        Ok(url) => Some(url),
        Err(error) => {
            tracing::warn!("Ignoring invalid {GATEWAY_URL_ENV}: {error}");
            None
        }
    }
}

/// Returns the gateway URL that activation saved, if one is valid.
pub fn saved_base_url() -> Option<String> {
    let path = gateway_url_path().ok()?;
    let raw = std::fs::read_to_string(path).ok()?;
    normalize_gateway_url(&raw).ok()
}

/// Saves the gateway URL that a successful activation used.
pub fn save_base_url(url: &str) -> Result<()> {
    let normalized = normalize_gateway_url(url)?;
    let path = gateway_url_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .context("Failed to create the Flix data directory for the gateway URL")?;
    }
    std::fs::write(&path, normalized.as_bytes())
        .context("Failed to save the subtitle gateway URL")?;
    Ok(())
}

/// Removes the saved gateway URL.
pub fn clear_base_url() -> Result<()> {
    let path = gateway_url_path()?;
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).context("Failed to remove the saved subtitle gateway URL"),
    }
}

/// Returns the path of the file that holds the gateway URL.
fn gateway_url_path() -> Result<PathBuf> {
    Ok(crate::config::data_dir()?.join(GATEWAY_URL_FILE))
}

/// Validates a gateway URL and returns it without a trailing slash.
///
/// The URL must be `https`. Plain `http` is allowed only for a loopback host,
/// because a developer runs the gateway locally. The URL must not carry
/// credentials, a query, or a fragment.
pub fn normalize_gateway_url(raw: &str) -> Result<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("The subtitle gateway URL is empty"));
    }
    if trimmed.len() > 255 {
        return Err(anyhow!("The subtitle gateway URL is too long"));
    }

    let parsed = reqwest::Url::parse(trimmed)
        .map_err(|_| anyhow!("The subtitle gateway URL is not a valid URL"))?;

    let host = parsed
        .host_str()
        .ok_or_else(|| anyhow!("The subtitle gateway URL has no host"))?
        .to_ascii_lowercase();
    let is_loopback =
        host == "localhost" || host == "127.0.0.1" || host == "[::1]" || host == "::1";

    match parsed.scheme() {
        "https" => {}
        "http" if is_loopback => {}
        "http" => {
            return Err(anyhow!(
                "The subtitle gateway URL must use https for a remote host"
            ))
        }
        _ => return Err(anyhow!("The subtitle gateway URL must use http or https")),
    }

    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(anyhow!(
            "The subtitle gateway URL must not carry credentials"
        ));
    }
    if parsed.query().is_some() || parsed.fragment().is_some() {
        return Err(anyhow!(
            "The subtitle gateway URL must not carry a query or a fragment"
        ));
    }

    Ok(trimmed.trim_end_matches('/').to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_https_url_and_removes_trailing_slash() {
        let url = normalize_gateway_url("https://gateway.babalola.dev/").unwrap();
        assert_eq!(url, "https://gateway.babalola.dev");
    }

    #[test]
    fn accepts_loopback_http_url() {
        let url = normalize_gateway_url(" http://127.0.0.1:8787 ").unwrap();
        assert_eq!(url, "http://127.0.0.1:8787");
    }

    #[test]
    fn rejects_remote_http_url() {
        assert!(normalize_gateway_url("http://gateway.babalola.dev").is_err());
    }

    #[test]
    fn rejects_unsupported_scheme_and_credentials() {
        assert!(normalize_gateway_url("file:///etc/passwd").is_err());
        assert!(normalize_gateway_url("https://user:pass@gateway.babalola.dev").is_err());
    }

    #[test]
    fn packaged_default_is_a_remote_https_gateway() {
        // A packaged build must not fall back to a developer loopback gateway.
        let url = normalize_gateway_url(PACKAGED_GATEWAY_URL).unwrap();
        assert_eq!(url, PACKAGED_GATEWAY_URL);
        assert!(url.starts_with("https://"));
        assert!(!url.contains("127.0.0.1"));
        assert!(!url.contains("localhost"));
    }

    #[test]
    fn rejects_empty_and_malformed_url() {
        assert!(normalize_gateway_url("   ").is_err());
        assert!(normalize_gateway_url("gateway.babalola.dev").is_err());
    }
}
