//! Append-only history of macOS Focus state transitions.
//!
//! Polls `focus_mode::focus_status()` on a long interval (60s by default), classifies the
//! delta against the previously-observed state, and writes one timestamped line per real
//! transition to `~/.config/pet/focus_history.log`. The consolidate path or LLM tools
//! can later read this file to derive long-term insights ("you've been in Work Focus an
//! average of 4h/day this week"). Non-macOS hosts produce no transitions.
//!
//! The format is intentionally simple — one line per event:
//!     2026-05-02T11:55:00+08:00 on:work
//!     2026-05-02T12:30:00+08:00 off
//!     2026-05-02T13:00:00+08:00 switch:personal
//! so anyone can `grep` / `awk` the file without a parser.

use std::path::{Path, PathBuf};
use std::time::Duration;

use tauri::AppHandle;
use tokio::io::AsyncWriteExt;

use crate::focus_mode::{focus_status, FocusStatus};

const POLL_INTERVAL_SECS: u64 = 60;
/// Roll the log over to `.1` once the active file passes this many bytes. ~1 MB ≈ 30k
/// transitions worth of lines; comfortably more than a year at typical use.
const MAX_LOG_BYTES: u64 = 1_000_000;

fn history_path() -> Option<PathBuf> {
    // Mirrors commands::memory's config dir layout so all pet state lives in one place.
    Some(dirs::config_dir()?.join("pet").join("focus_history.log"))
}

/// Pure transition classifier. Returns `Some(event_str)` when the state change is worth
/// logging, `None` otherwise (no change, or "we just started up and Focus was already off
/// — boring"). `event_str` is the second column of the log line.
pub fn classify_transition(prev: Option<&FocusStatus>, curr: &FocusStatus) -> Option<String> {
    let curr_name = curr.name.as_deref().unwrap_or("");
    match prev {
        // First observation — only log if Focus is on at startup, since "off → off"
        // would otherwise produce a meaningless log entry every restart.
        None => {
            if curr.active {
                Some(format!("on:{}", curr_name))
            } else {
                None
            }
        }
        Some(p) => {
            if p.active && !curr.active {
                Some("off".to_string())
            } else if !p.active && curr.active {
                Some(format!("on:{}", curr_name))
            } else if p.active && curr.active && p.name != curr.name {
                // User changed focus mode without turning it off (e.g. Work → Personal).
                Some(format!("switch:{}", curr_name))
            } else {
                None
            }
        }
    }
}

/// Spawn the background poller. Wraps the pure classifier with the IO bits — focus_status
/// reads, append-to-log writes, and the polling sleep.
pub fn spawn(_app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        // Small startup delay so we don't race with the focus state file being initialized
        // by macOS during login.
        tokio::time::sleep(Duration::from_secs(15)).await;

        let mut last: Option<FocusStatus> = None;
        loop {
            // Status is None on non-macOS or unreadable file; treat that as "we don't know,
            // skip this tick" rather than synthesizing a transition.
            if let Some(curr) = focus_status().await {
                if let Some(event) = classify_transition(last.as_ref(), &curr) {
                    let _ = append_event(&event).await;
                }
                last = Some(curr);
            }
            tokio::time::sleep(Duration::from_secs(POLL_INTERVAL_SECS)).await;
        }
    });
}

async fn append_event(event: &str) -> Result<(), std::io::Error> {
    let Some(path) = history_path() else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    // Best-effort: a failure to rotate (e.g. permission issue) shouldn't block writes.
    let _ = rotate_if_needed(&path, MAX_LOG_BYTES).await;
    let ts = chrono::Local::now()
        .format("%Y-%m-%dT%H:%M:%S%:z")
        .to_string();
    let mut f = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .await?;
    f.write_all(format!("{} {}\n", ts, event).as_bytes()).await?;
    Ok(())
}

/// Roll `path` over to `<path>.1` when it has reached `max_bytes`. Returns `Ok(true)` if
/// the rotation happened, `Ok(false)` if the file is small enough or doesn't exist yet.
/// Any pre-existing `.1` is overwritten — we keep one generation only, since the LLM
/// reading this log cares about recent transitions, not deep history.
async fn rotate_if_needed(path: &Path, max_bytes: u64) -> std::io::Result<bool> {
    let meta = match tokio::fs::metadata(path).await {
        Ok(m) => m,
        Err(_) => return Ok(false), // file doesn't exist yet
    };
    if meta.len() < max_bytes {
        return Ok(false);
    }
    let rotated = rotated_path(path);
    tokio::fs::rename(path, &rotated).await?;
    Ok(true)
}

/// Append `.1` to a path, preserving the original filename. Pure helper, no IO.
fn rotated_path(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(".1");
    PathBuf::from(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fs(active: bool, name: Option<&str>) -> FocusStatus {
        FocusStatus {
            active,
            name: name.map(String::from),
        }
    }

    #[test]
    fn first_observation_inactive_logs_nothing() {
        assert!(classify_transition(None, &fs(false, None)).is_none());
    }

    #[test]
    fn first_observation_active_logs_on() {
        let ev = classify_transition(None, &fs(true, Some("work")));
        assert_eq!(ev.as_deref(), Some("on:work"));
    }

    #[test]
    fn off_to_on_logs_on() {
        let prev = fs(false, None);
        let ev = classify_transition(Some(&prev), &fs(true, Some("personal")));
        assert_eq!(ev.as_deref(), Some("on:personal"));
    }

    #[test]
    fn on_to_off_logs_off() {
        let prev = fs(true, Some("work"));
        let ev = classify_transition(Some(&prev), &fs(false, None));
        assert_eq!(ev.as_deref(), Some("off"));
    }

    #[test]
    fn name_change_while_active_logs_switch() {
        let prev = fs(true, Some("work"));
        let ev = classify_transition(Some(&prev), &fs(true, Some("personal")));
        assert_eq!(ev.as_deref(), Some("switch:personal"));
    }

    #[test]
    fn no_change_returns_none() {
        let prev = fs(true, Some("work"));
        assert!(classify_transition(Some(&prev), &fs(true, Some("work"))).is_none());
        let prev = fs(false, None);
        assert!(classify_transition(Some(&prev), &fs(false, None)).is_none());
    }

    #[test]
    fn name_missing_uses_empty_string() {
        let ev = classify_transition(None, &fs(true, None));
        assert_eq!(ev.as_deref(), Some("on:"));
    }

    // ---- rotation ----

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

    /// Build a fresh per-test temp dir under the system temp root. Caller is responsible
    /// for cleanup; we use a unique nanos-based name to avoid collisions across parallel
    /// test runs without pulling in tempfile as a dev-dep.
    fn fresh_temp_dir(label: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("pet-test-{}-{}", label, nanos));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[tokio::test]
    async fn rotates_when_oversized() {
        let dir = fresh_temp_dir("rot");
        let log = dir.join("focus_history.log");
        tokio::fs::write(&log, b"0123456789").await.unwrap();

        let did_rotate = rotate_if_needed(&log, 5).await.unwrap();
        assert!(did_rotate);
        assert!(!log.exists(), "active log should have been moved");
        let rotated = dir.join("focus_history.log.1");
        assert!(rotated.exists(), "rotated copy should appear");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn does_not_rotate_when_under_limit() {
        let dir = fresh_temp_dir("norot");
        let log = dir.join("focus_history.log");
        tokio::fs::write(&log, b"abc").await.unwrap();

        let did_rotate = rotate_if_needed(&log, 1024).await.unwrap();
        assert!(!did_rotate);
        assert!(log.exists(), "active log untouched");
        assert!(!dir.join("focus_history.log.1").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn rotation_overwrites_existing_dot_one() {
        // Only one generation is kept. An old .1 should be replaced silently.
        let dir = fresh_temp_dir("overwrite");
        let log = dir.join("focus_history.log");
        let prior = dir.join("focus_history.log.1");
        tokio::fs::write(&log, b"NEWNEWNEWNEW").await.unwrap();
        tokio::fs::write(&prior, b"OLD").await.unwrap();

        rotate_if_needed(&log, 5).await.unwrap();
        let rotated_contents = tokio::fs::read(&prior).await.unwrap();
        assert_eq!(rotated_contents, b"NEWNEWNEWNEW", "the new one should now be .1");

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
