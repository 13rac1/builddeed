//! SHA-256 of files, and the folder check (the registered feature).

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex::encode(h.finalize()))
}

/// relative path -> SHA-256, for each file in a folder and its subfolders.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Snapshot {
    pub folder: PathBuf,
    pub taken_at_ms: u64,
    pub files: BTreeMap<String, String>,
}

pub fn snapshot(folder: &Path, now_ms: u64) -> std::io::Result<Snapshot> {
    let mut files = BTreeMap::new();
    let mut stack = vec![folder.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let ft = entry.file_type()?;
            let path = entry.path();
            if ft.is_dir() {
                stack.push(path);
            } else if ft.is_file() {
                let rel = path.strip_prefix(folder).unwrap_or(&path);
                let key = rel.to_string_lossy().replace('\\', "/");
                files.insert(key, sha256_file(&path)?);
            }
            // Symbolic links are not followed.
        }
    }
    Ok(Snapshot {
        folder: folder.to_path_buf(),
        taken_at_ms: now_ms,
        files,
    })
}

#[derive(Debug, Default, PartialEq)]
pub struct Changes {
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub changed: Vec<String>,
    pub same: usize,
}

pub fn compare(old: &Snapshot, new: &Snapshot) -> Changes {
    let mut c = Changes::default();
    for (path, hash) in &new.files {
        match old.files.get(path) {
            None => c.added.push(path.clone()),
            Some(h) if h != hash => c.changed.push(path.clone()),
            Some(_) => c.same += 1,
        }
    }
    for path in old.files.keys() {
        if !new.files.contains_key(path) {
            c.removed.push(path.clone());
        }
    }
    c
}

pub fn report(new: &Snapshot, c: &Changes) -> String {
    let mut s = format!(
        "BuildDeed Checksum folder report\nFolder: {}\nFiles: {}\nSame: {}  Changed: {}  Added: {}  Removed: {}\n",
        new.folder.display(),
        new.files.len(),
        c.same,
        c.changed.len(),
        c.added.len(),
        c.removed.len()
    );
    for (title, list) in [
        ("Changed", &c.changed),
        ("Added", &c.added),
        ("Removed", &c.removed),
    ] {
        if !list.is_empty() {
            s.push_str(&format!("\n{title}:\n"));
            for p in list {
                s.push_str(&format!("  {p}\n"));
            }
        }
    }
    s
}

/// The name of the snapshot file for a folder in the data folder.
pub fn snapshot_name(folder: &Path) -> String {
    let h = Sha256::digest(folder.to_string_lossy().as_bytes());
    format!("snapshot-{}.json", &hex::encode(h)[..16])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compare_finds_added_removed_and_changed() {
        let dir = std::env::temp_dir().join(format!("bd-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("a.txt"), "one").unwrap();
        std::fs::write(dir.join("sub/b.txt"), "two").unwrap();
        std::fs::write(dir.join("c.txt"), "three").unwrap();
        let old = snapshot(&dir, 1).unwrap();
        assert_eq!(old.files.len(), 3);
        assert_eq!(
            old.files["a.txt"],
            "7692c3ad3540bb803c020b3aee66cd8887123234ea0c6e7143c0add73ff431ed"
        );

        std::fs::write(dir.join("a.txt"), "ONE").unwrap();
        std::fs::remove_file(dir.join("c.txt")).unwrap();
        std::fs::write(dir.join("sub/d.txt"), "four").unwrap();
        let new = snapshot(&dir, 2).unwrap();
        let c = compare(&old, &new);
        assert_eq!(c.changed, vec!["a.txt"]);
        assert_eq!(c.removed, vec!["c.txt"]);
        assert_eq!(c.added, vec!["sub/d.txt"]);
        assert_eq!(c.same, 1);
        assert!(report(&new, &c).contains("Changed: 1"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
