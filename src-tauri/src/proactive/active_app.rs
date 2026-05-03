//! Iter R15: track time-on-current-app between proactive ticks so the
//! prompt can register "user has been in {app} for {N} minutes" hints.
//!
//! Pure compute_active_duration handles the state-transition math; a thin
//! wrapper reads/writes the static `LAST_ACTIVE_APP` and applies redaction
//! before producing the final hint string. format_active_app_hint is the
//! pure formatter — empty string for short durations or empty app name so
//! the caller can `push_if_nonempty`.
//!
//! Granularity is whatever the caller's polling rate is — production reads
//! at the start of each `run_proactive_turn` (typically every 5 minutes
//! per `ProactiveConfig.interval_seconds`). Coarse but enough to register
//! "long focused session" patterns; finer-grain tracking would warrant a
//! separate background loop.

use std::time::Instant;

/// Minutes a user has to spend in the same app before the hint fires. Below
/// this, the prompt stays quiet — short hops between apps shouldn't surface
/// as "stuck on X". 15 minutes is roughly "actually focused on it" without
/// being obnoxiously soon.
pub const MIN_DURATION_MINUTES: u64 = 15;

/// In-memory snapshot of the foreground app at the time we first observed
/// it. `since` is monotonic, so the elapsed minutes survive system clock
/// adjustments. App-name string is raw (un-redacted) — redaction happens
/// at hint-format time so the comparison with the next tick's app name
/// stays stable.
#[derive(Clone, Debug)]
pub struct ActiveAppSnapshot {
    pub app: String,
    pub since: Instant,
}

/// Process-wide singleton: the last-observed active-app snapshot. None on
/// fresh process start (and on non-macOS builds where we can't read the
/// foreground app). Updated by `update_active_app_snapshot`.
pub static LAST_ACTIVE_APP: std::sync::Mutex<Option<ActiveAppSnapshot>> =
    std::sync::Mutex::new(None);

/// Pure transition-state machine. Given the previous snapshot, the current
/// app name, and "now", returns:
/// - the new snapshot to write back (carries forward `since` if the app
///   didn't change; resets to `now` if it did)
/// - `Some(minutes)` if the app is unchanged from the previous snapshot
///   (i.e. user has been in this app for that many minutes); `None` if the
///   app changed or there was no prior snapshot
///
/// Pure / testable so the wrapper can read/write the static and the unit
/// tests can drive every state branch deterministically.
pub fn compute_active_duration(
    prev: Option<&ActiveAppSnapshot>,
    current_app: &str,
    now: Instant,
) -> (ActiveAppSnapshot, Option<u64>) {
    match prev {
        Some(p) if p.app == current_app => {
            let secs = now.saturating_duration_since(p.since).as_secs();
            (
                ActiveAppSnapshot {
                    app: p.app.clone(),
                    since: p.since,
                },
                Some(secs / 60),
            )
        }
        _ => (
            ActiveAppSnapshot {
                app: current_app.to_string(),
                since: now,
            },
            None,
        ),
    }
}

/// Pure formatter. Returns empty string when:
/// - app name is empty / whitespace
/// - minutes is below `MIN_DURATION_MINUTES`
///
/// Otherwise produces "用户在「{app}」里已经待了 {N} 分钟。" — a single
/// line so `push_if_nonempty` slots it into the prompt's optional-section
/// chain alongside other hints.
pub fn format_active_app_hint(app: &str, minutes: u64) -> String {
    if app.trim().is_empty() || minutes < MIN_DURATION_MINUTES {
        return String::new();
    }
    format!("用户在「{}」里已经待了 {} 分钟。", app, minutes)
}

/// Iter R22: panel-side read-only inspection of the active-app snapshot.
/// Does NOT mutate `LAST_ACTIVE_APP` (unlike `update_and_format_active_app_hint`)
/// — panel poll can hit this every few seconds without resetting the
/// "since" clock. Returns redacted app name + elapsed minutes when a
/// snapshot exists, `None` on fresh process / non-macOS / when the
/// proactive loop hasn't observed any foreground app yet.
pub fn snapshot_active_app() -> Option<ActiveAppSummary> {
    let snap = LAST_ACTIVE_APP.lock().ok().and_then(|g| g.clone())?;
    let minutes = Instant::now()
        .saturating_duration_since(snap.since)
        .as_secs()
        / 60;
    Some(ActiveAppSummary {
        app: crate::redaction::redact_with_settings(&snap.app),
        minutes,
    })
}

/// Iter R22: panel-side compact summary of the current active app + how
/// long the user has been on it. Mirrors the prompt-hint form
/// (`format_active_app_hint`) but stays as structured data so the chip
/// can color-code by duration band.
#[derive(Clone, Debug, serde::Serialize)]
pub struct ActiveAppSummary {
    pub app: String,
    pub minutes: u64,
}

/// Production wrapper: reads the static snapshot, calls `compute_active_duration`,
/// writes the new snapshot back, redacts the app name before passing to
/// `format_active_app_hint`. Returns the formatted hint string (or empty
/// when the gate doesn't fire). Caller is `run_proactive_turn`; passing
/// the current app + now as args keeps the caller in charge of fetching
/// (e.g. via `system_tools::current_active_window`).
pub fn update_and_format_active_app_hint(current_app: Option<&str>) -> String {
    let Some(app) = current_app else {
        // Non-macOS or osascript failed — leave snapshot untouched, no hint.
        return String::new();
    };
    let now = Instant::now();
    let prev = LAST_ACTIVE_APP.lock().ok().and_then(|g| g.clone());
    let (new_snapshot, duration_minutes) = compute_active_duration(prev.as_ref(), app, now);
    if let Ok(mut g) = LAST_ACTIVE_APP.lock() {
        *g = Some(new_snapshot);
    }
    let Some(minutes) = duration_minutes else {
        return String::new();
    };
    // Redact at hint-format time only (snapshot stays raw so transition
    // detection compares stable un-redacted strings — otherwise the user
    // changing redaction patterns mid-session would falsely register as
    // an app change).
    let redacted_app = crate::redaction::redact_with_settings(app);
    format_active_app_hint(&redacted_app, minutes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(app: &str, since: Instant) -> ActiveAppSnapshot {
        ActiveAppSnapshot {
            app: app.to_string(),
            since,
        }
    }

    #[test]
    fn compute_no_prior_snapshot_returns_none_duration() {
        let now = Instant::now();
        let (new, dur) = compute_active_duration(None, "Cursor", now);
        assert_eq!(new.app, "Cursor");
        assert_eq!(new.since, now);
        assert!(dur.is_none(), "no prior → no duration to report");
    }

    #[test]
    fn compute_app_change_resets_since_and_returns_none() {
        let then = Instant::now() - std::time::Duration::from_secs(900); // 15 min ago
        let prev = snap("Cursor", then);
        let now = Instant::now();
        let (new, dur) = compute_active_duration(Some(&prev), "Slack", now);
        assert_eq!(new.app, "Slack");
        assert_eq!(new.since, now, "since resets to now on app change");
        assert!(dur.is_none(), "app changed → no duration");
    }

    #[test]
    fn compute_same_app_carries_since_and_returns_minutes() {
        let then = Instant::now() - std::time::Duration::from_secs(20 * 60); // 20 min ago
        let prev = snap("Cursor", then);
        let now = Instant::now();
        let (new, dur) = compute_active_duration(Some(&prev), "Cursor", now);
        assert_eq!(new.app, "Cursor");
        assert_eq!(new.since, then, "since carries forward when app unchanged");
        let mins = dur.expect("same app → duration");
        // 20 min ago, with some test-runtime slop. Accept 19-20.
        assert!(
            (19..=20).contains(&mins),
            "expected 19-20 min, got {}",
            mins
        );
    }

    #[test]
    fn format_returns_empty_for_short_duration() {
        assert_eq!(format_active_app_hint("Cursor", 0), "");
        assert_eq!(format_active_app_hint("Cursor", 14), "");
    }

    #[test]
    fn format_fires_at_min_duration_threshold() {
        let out = format_active_app_hint("Cursor", MIN_DURATION_MINUTES);
        assert!(out.contains("Cursor"));
        assert!(out.contains("15 分钟"));
        assert!(out.contains("已经待了"));
    }

    #[test]
    fn format_returns_empty_for_blank_app() {
        assert_eq!(format_active_app_hint("", 30), "");
        assert_eq!(format_active_app_hint("   ", 30), "");
    }

    #[test]
    fn format_handles_long_durations() {
        let out = format_active_app_hint("Slack", 240);
        assert!(out.contains("240"));
        assert!(out.contains("Slack"));
    }
}
