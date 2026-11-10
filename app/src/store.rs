//! Files that the app keeps in its data folder.

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::path::PathBuf;

/// The last successful license check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Registration {
    pub account_id: String,
    pub public_key: String,
    pub token_id: String,
    pub updates_until_ms: u64,
    /// The time of the last check that reached the network.
    pub checked_at_ms: u64,
}

/// With no network, a stored registration stays valid for this time.
pub const OFFLINE_GRACE_MS: u64 = 14 * 24 * 60 * 60 * 1000;

pub fn data_dir() -> PathBuf {
    let base = if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
    };
    base.unwrap_or_else(std::env::temp_dir).join("builddeed-checksum")
}

pub fn load<T: DeserializeOwned>(name: &str) -> Option<T> {
    let text = std::fs::read_to_string(data_dir().join(name)).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn save<T: Serialize>(name: &str, value: &T) -> Result<PathBuf, String> {
    let dir = data_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("Cannot make {}: {e}", dir.display()))?;
    let path = dir.join(name);
    let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| format!("Cannot write {}: {e}", path.display()))?;
    Ok(path)
}

pub fn remove(name: &str) {
    let _ = std::fs::remove_file(data_dir().join(name));
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
