use anyhow::{Context, Result};
use keyring::Entry;

const KEYRING_SERVICE: &str = "flix_desktop_gateway";
const KEYRING_USER: &str = "device_token";

pub fn save_device_token(token: &str) -> Result<()> {
    let entry = Entry::new(KEYRING_SERVICE, KEYRING_USER)
        .context("Failed to open OS credential store entry")?;
    entry
        .set_password(token)
        .context("Failed to write device token to OS credential store")?;
    Ok(())
}

pub fn get_device_token() -> Result<Option<String>> {
    let entry = Entry::new(KEYRING_SERVICE, KEYRING_USER)
        .context("Failed to open OS credential store entry")?;
    match entry.get_password() {
        Ok(token) => Ok(Some(token)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(anyhow::anyhow!("Keyring get error: {:?}", e)),
    }
}

pub fn delete_device_token() -> Result<()> {
    let entry = Entry::new(KEYRING_SERVICE, KEYRING_USER)
        .context("Failed to open OS credential store entry")?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => {
            tracing::warn!("Could not delete from OS credential store: {:?}", e);
            Ok(())
        }
    }
}
