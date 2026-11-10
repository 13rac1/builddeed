//! Files that the app keeps in its data folder.

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::path::PathBuf;

/// The last successful license check.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Registration {
    /// The owner of the license.
    pub account_id: String,
    pub token_id: String,
    pub updates_until_ms: u64,
    /// The time of the last check that reached the network.
    pub checked_at_ms: u64,
}

/// With no network, a stored registration stays valid for this time after
/// the last check that reached the network.
pub const OFFLINE_GRACE_MS: u64 = 14 * 24 * 60 * 60 * 1000;

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

/// The result of a start-up license check.
#[derive(Debug, PartialEq)]
pub enum Check {
    /// The network answered: a license (save it), or none (remove the file).
    Fresh(Option<Registration>),
    /// No network, and the stored check is less than OFFLINE_GRACE_MS old:
    /// use it, but do not save it again.
    Grace(Registration),
    /// No network, and no stored check that can be used.
    Failed(String),
}

/// Decides the start-up state from the network answer and the stored check.
/// A stored `checked_at_ms` in the future is not trusted: the clock went back,
/// or someone wrote the file.
pub fn resolve(
    fresh: Result<Option<Registration>, String>,
    stored: &Registration,
    now: u64,
) -> Check {
    match fresh {
        Ok(r) => Check::Fresh(r),
        Err(e) => {
            let age_ok =
                stored.checked_at_ms <= now && now - stored.checked_at_ms < OFFLINE_GRACE_MS;
            if age_ok {
                Check::Grace(stored.clone())
            } else {
                Check::Failed(e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reg(checked_at_ms: u64) -> Registration {
        Registration {
            account_id: "alice.testnet".into(),
            token_id: "1".into(),
            updates_until_ms: 9_999_999_999_999,
            checked_at_ms,
        }
    }

    const NOW: u64 = 1_800_000_000_000;

    #[test]
    fn a_network_answer_wins() {
        let fresh = reg(NOW);
        assert_eq!(
            resolve(Ok(Some(fresh.clone())), &reg(0), NOW),
            Check::Fresh(Some(fresh))
        );
        assert_eq!(
            resolve(Ok(None), &reg(NOW), NOW),
            Check::Fresh(None),
            "license sold"
        );
    }

    #[test]
    fn grace_lasts_less_than_14_days() {
        let err = || Err("no network".to_string());
        assert!(matches!(
            resolve(err(), &reg(NOW - 1), NOW),
            Check::Grace(_)
        ));
        assert!(matches!(
            resolve(err(), &reg(NOW - OFFLINE_GRACE_MS + 1), NOW),
            Check::Grace(_)
        ));
        assert_eq!(
            resolve(err(), &reg(NOW - OFFLINE_GRACE_MS), NOW),
            Check::Failed("no network".into())
        );
    }

    #[test]
    fn a_future_check_time_gives_no_grace() {
        let r = resolve(Err("no network".into()), &reg(NOW + 1), NOW);
        assert_eq!(r, Check::Failed("no network".into()));
        let r = resolve(Err("no network".into()), &reg(u64::MAX), NOW);
        assert_eq!(r, Check::Failed("no network".into()));
    }
}
