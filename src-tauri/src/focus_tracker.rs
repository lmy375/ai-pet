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

use std::path::PathBuf;
use std::time::Duration;

use tauri::AppHandle;
use tokio::io::AsyncWriteExt;

use crate::focus_mode::{focus_status, FocusStatus};
use crate::log_rotation::rotate_if_needed;

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

// Rotation lives in crate::log_rotation now — focus_tracker uses the shared helper so any
// future log-bearing module can pick it up without copying again.

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

    // Rotation tests live in crate::log_rotation now — same logic, single source of
    // truth. focus_tracker just calls rotate_if_needed at append time.
}
