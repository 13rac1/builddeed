//! Files that the app keeps in its data folder.

use serde::{de::DeserializeOwned, Serialize};
use std::path::PathBuf;

/// The data folder of the app, in the user's home. None if the system gives
/// no home folder: the app then keeps nothing, and does not use a shared
/// folder such as /tmp.
pub fn data_dir() -> Option<PathBuf> {
    let base = if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
    };
    base.filter(|p| p.is_absolute())
        .map(|p| p.join("builddeed-checksum"))
}

/// Reads a JSON file of the data folder. A missing or corrupt file is None.
pub fn load<T: DeserializeOwned>(name: &str) -> Option<T> {
    let text = std::fs::read_to_string(data_dir()?.join(name)).ok()?;
    serde_json::from_str(&text).ok()
}

/// Writes a JSON file of the data folder. It writes a temporary file and
/// renames it, so that a crash does not leave half a file.
pub fn save<T: Serialize>(name: &str, value: &T) -> Result<PathBuf, String> {
    let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    save_text(name, &text)
}

pub fn save_text(name: &str, text: &str) -> Result<PathBuf, String> {
    let dir = data_dir().ok_or("No home folder: the app cannot save files")?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("Cannot make {}: {e}", dir.display()))?;
    let path = dir.join(name);
    let tmp = dir.join(format!("{name}.tmp"));
    std::fs::write(&tmp, text).map_err(|e| format!("Cannot write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("Cannot write {}: {e}", path.display()))?;
    Ok(path)
}

pub fn remove(name: &str) {
    if let Some(dir) = data_dir() {
        let _ = std::fs::remove_file(dir.join(name));
    }
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
