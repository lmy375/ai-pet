//! The one place that knows the on-disk layout and how a file is written.
//!
//! Error rules for every caller:
//! - read: a missing file is `Ok(None)` (callers fall back to a default);
//!   any other IO or parse failure is `Err`;
//! - write: always `Result`; a caller either propagates it or reports it
//!   explicitly. Only log files may swallow a write failure.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::de::DeserializeOwned;
use serde::Serialize;

/// `<config>/<sub>/`, created on demand.
pub fn data_dir(sub: &str) -> Result<PathBuf, String> {
    let dir = crate::common::config_dir()?.join(sub);
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create {}: {e}", dir.display()))?;
    Ok(dir)
}

/// Write `bytes` to `path` through a sibling temp file: write, fsync, rename.
/// A crash or a full disk therefore leaves the previous file intact rather than
/// a truncated one. The temp name is unique per write, so concurrent writers
/// can't rename each other's half-written file into place.
pub fn write_atomic(path: &Path, bytes: impl AsRef<[u8]>) -> io::Result<()> {
    static SEQ: AtomicU64 = AtomicU64::new(0);

    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let tmp = parent.join(format!(
        ".{name}.{}.{}.tmp",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));

    let mut file = File::create(&tmp)?;
    file.write_all(bytes.as_ref())?;
    file.sync_all()?;
    drop(file);

    match fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(e) => {
            // A leftover temp file would be picked up as a half-written record
            // by directory listings (LLM log compaction sweeps them).
            let _ = fs::remove_file(&tmp);
            Err(e)
        }
    }
}

/// Pretty JSON, atomically written. This is the shape every record file uses.
pub fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let json = serde_json::to_vec_pretty(value)
        .map_err(|e| format!("Failed to serialize {}: {e}", path.display()))?;
    write_atomic(path, json).map_err(|e| format!("Failed to write {}: {e}", path.display()))
}

/// `Ok(None)` when the file does not exist; IO and parse failures are `Err`.
pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<Option<T>, String> {
    let Some(text) = read_text(path)? else {
        return Ok(None);
    };
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|e| format!("Failed to parse {}: {e}", path.display()))
}

/// `Ok(None)` when the file does not exist; other IO failures are `Err`.
pub fn read_text(path: &Path) -> Result<Option<String>, String> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("Failed to read {}: {e}", path.display())),
    }
}

/// Atomically write text (YAML, Markdown, …), creating parent dirs first.
pub fn write_text(path: &Path, text: &str) -> Result<(), String> {
    write_atomic(path, text).map_err(|e| format!("Failed to write {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pet-store-{name}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn missing_json_is_none_not_an_error() {
        let dir = temp_dir("missing");
        let path = dir.join("nope.json");
        assert!(read_json::<Vec<u32>>(&path).unwrap().is_none());
        assert!(read_text(&path).unwrap().is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_json_is_an_error() {
        let dir = temp_dir("corrupt");
        let path = dir.join("bad.json");
        fs::write(&path, "{ not json").unwrap();
        assert!(read_json::<Vec<u32>>(&path).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn atomic_write_replaces_content_and_leaves_no_temp() {
        let dir = temp_dir("atomic");
        let path = dir.join("nested").join("data.json");
        write_json(&path, &vec![1, 2, 3]).unwrap();
        write_json(&path, &vec![4, 5]).unwrap();
        assert_eq!(read_json::<Vec<u32>>(&path).unwrap(), Some(vec![4, 5]));
        let leftovers: Vec<_> = fs::read_dir(path.parent().unwrap())
            .unwrap()
            .flatten()
            .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("tmp"))
            .collect();
        assert!(leftovers.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_text_creates_parents_and_reads_back() {
        let dir = temp_dir("text");
        let path = dir.join("a").join("b.md");
        write_text(&path, "hi").unwrap();
        assert_eq!(read_text(&path).unwrap().as_deref(), Some("hi"));
        let _ = fs::remove_dir_all(&dir);
    }
}