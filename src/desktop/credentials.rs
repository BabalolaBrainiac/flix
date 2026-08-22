use anyhow::{Context, Result};
use keyring::Entry;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const KEYRING_SERVICE: &str = "flix_desktop_gateway";
const KEYRING_USER: &str = "device_token";
const TOKEN_FILE: &str = "device_token";

// The token lives in a file in the data directory, not the OS keychain. The
// keychain binds access to the app's code signature, so an unsigned build with
// a new signature could not read a token an earlier build wrote, which forced
// re-activation after every update. A data-directory file survives an update
// and any build can read it. It holds a revocable, rate-limited gateway token
// and is written owner-only.
fn token_path() -> Result<PathBuf> {
    Ok(crate::config::data_dir()?.join(TOKEN_FILE))
}

fn set_owner_only(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}

/// In-memory cache of the device token for this process run.
///
/// The outer `Option` records whether the keychain has been read yet. The inner
/// `Option` is the token, or `None` when no token is stored. macOS prompts for
/// keychain access on each read from an app whose code signature it cannot
/// match. Reading once per launch and reusing the value keeps that prompt to at
/// most one per launch, instead of one on every activation check and every
/// subtitle request.
static TOKEN_CACHE: Mutex<Option<Option<String>>> = Mutex::new(None);

pub fn save_device_token(token: &str) -> Result<()> {
    let path = token_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .context("Failed to create the data directory for the device token")?;
    }
    std::fs::write(&path, token.as_bytes()).context("Failed to write the device token")?;
    set_owner_only(&path);

    *TOKEN_CACHE.lock().unwrap() = Some(Some(token.to_string()));
    Ok(())
}

pub fn get_device_token() -> Result<Option<String>> {
    let mut cache = TOKEN_CACHE.lock().unwrap();
    if let Some(cached) = cache.as_ref() {
        return Ok(cached.clone());
    }

    let value = match std::fs::read_to_string(token_path()?) {
        Ok(contents) => {
            let trimmed = contents.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error).context("Failed to read the device token"),
    };
    *cache = Some(value.clone());
    Ok(value)
}

pub fn delete_device_token() -> Result<()> {
    match std::fs::remove_file(token_path()?) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => tracing::warn!("Could not remove the device token file: {error}"),
    }
    // Also clear a token an older keychain-based build may have left behind.
    if let Ok(entry) = Entry::new(KEYRING_SERVICE, KEYRING_USER) {
        let _ = entry.delete_credential();
    }

    *TOKEN_CACHE.lock().unwrap() = Some(None);
    Ok(())
}
