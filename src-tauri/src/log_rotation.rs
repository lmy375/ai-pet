//! Tiny utility for size-based log rotation. Append-only on-disk logs use this to keep
//! their files bounded without growing forever.
//!
//! Two operations:
//! - [`rotated_path`] — pure path tweak (`foo.log` → `foo.log.1`)
//! - [`rotate_if_needed`] — async stat + rename when over `max_bytes`.
//!
//! Only one generation is retained. Callers that want richer retention (date-based
//! archives, multiple `.1`/`.2`/`.3` slots) should write their own thing.

use std::path::{Path, PathBuf};

/// Append `.1` to a path's filename, preserving the original. Use over `with_extension`
/// because that helper *replaces* the existing extension — `focus_history.log` ends up as
/// `focus_history.1` instead of `focus_history.log.1`. Direct OsString concat is dumb but
/// correct.
pub fn rotated_path(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(".1");
    PathBuf::from(s)
}

/// Roll `path` over to `<path>.1` when it has reached `max_bytes`. Returns `Ok(true)` if
/// the rotation happened, `Ok(false)` if the file is small enough or doesn't exist yet.
/// Any pre-existing `.1` is overwritten — we keep one generation only.
pub async fn rotate_if_needed(path: &Path, max_bytes: u64) -> std::io::Result<bool> {
    let meta = match tokio::fs::metadata(path).await {
        Ok(m) => m,
        Err(_) => return Ok(false),
    };
    if meta.len() < max_bytes {
        return Ok(false);
    }
    let rotated = rotated_path(path);
    tokio::fs::rename(path, &rotated).await?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_temp_dir(label: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("pet-test-{}-{}", label, nanos));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn rotated_path_appends_dot_one() {
        let p = PathBuf::from("/some/dir/focus_history.log");
        assert_eq!(rotated_path(&p), PathBuf::from("/some/dir/focus_history.log.1"));
    }

    #[test]
    fn rotated_path_handles_no_extension() {
        let p = PathBuf::from("/tmp/raw");
        assert_eq!(rotated_path(&p), PathBuf::from("/tmp/raw.1"));
    }

    #[tokio::test]
    async fn rotates_when_oversized() {
        let dir = fresh_temp_dir("rot");
        let log = dir.join("foo.log");
        tokio::fs::write(&log, b"0123456789").await.unwrap();

        let did_rotate = rotate_if_needed(&log, 5).await.unwrap();
        assert!(did_rotate);
        assert!(!log.exists());
        assert!(dir.join("foo.log.1").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn does_not_rotate_when_under_limit() {
        let dir = fresh_temp_dir("norot");
        let log = dir.join("foo.log");
        tokio::fs::write(&log, b"abc").await.unwrap();

        let did_rotate = rotate_if_needed(&log, 1024).await.unwrap();
        assert!(!did_rotate);
        assert!(log.exists());
        assert!(!dir.join("foo.log.1").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn rotation_overwrites_existing_dot_one() {
        let dir = fresh_temp_dir("overwrite");
        let log = dir.join("foo.log");
        let prior = dir.join("foo.log.1");
        tokio::fs::write(&log, b"NEWNEWNEWNEW").await.unwrap();
        tokio::fs::write(&prior, b"OLD").await.unwrap();

        rotate_if_needed(&log, 5).await.unwrap();
        let rotated_contents = tokio::fs::read(&prior).await.unwrap();
        assert_eq!(rotated_contents, b"NEWNEWNEWNEW", "newest replaces .1");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn missing_file_is_no_op() {
        let dir = fresh_temp_dir("missing");
        let log = dir.join("nope.log");
        let did_rotate = rotate_if_needed(&log, 1).await.unwrap();
        assert!(!did_rotate);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
