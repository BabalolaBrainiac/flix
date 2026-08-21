use anyhow::{Context, Result};
use keyring::Entry;
use std::sync::Mutex;

const KEYRING_SERVICE: &str = "flix_desktop_gateway";
const KEYRING_USER: &str = "device_token";

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
    let entry = Entry::new(KEYRING_SERVICE, KEYRING_USER)
        .context("Failed to open OS credential store entry")?;
    entry
        .set_password(token)
        .context("Failed to write device token to OS credential store")?;
    // Seed the cache so the next read does not touch the keychain again.
    *TOKEN_CACHE.lock().unwrap() = Some(Some(token.to_string()));
    Ok(())
}

pub fn get_device_token() -> Result<Option<String>> {
    // Hold the lock across the read so two callers cannot both trigger a
    // keychain prompt at once. The second caller waits, then sees the cache.
    let mut cache = TOKEN_CACHE.lock().unwrap();
    if let Some(cached) = cache.as_ref() {
        return Ok(cached.clone());
    }

    let entry = Entry::new(KEYRING_SERVICE, KEYRING_USER)
        .context("Failed to open OS credential store entry")?;
    let value = match entry.get_password() {
        Ok(token) => Some(token),
        Err(keyring::Error::NoEntry) => None,
        Err(e) => return Err(anyhow::anyhow!("Keyring get error: {:?}", e)),
    };
    *cache = Some(value.clone());
    Ok(value)
}

pub fn delete_device_token() -> Result<()> {
    let entry = Entry::new(KEYRING_SERVICE, KEYRING_USER)
        .context("Failed to open OS credential store entry")?;
    let result = match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => {
            tracing::warn!("Could not delete from OS credential store: {:?}", e);
            Ok(())
        }
    };
    // Record that no token is stored, so a later read does not hit the keychain.
    *TOKEN_CACHE.lock().unwrap() = Some(None);
    result
}
