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

/// Iter R27: minutes after which the same-app duration counts as "deep
/// focus" — at this point the prompt hint upgrades from informational
/// ("用户在 X 已 N 分钟") to directive ("...这是深度专注期，这次开口应当
/// 极简或选择沉默"). 60m ≈ a Pomodoro × 2 / a typical deep-work block;
/// below it 15-60 is "regular sustained focus" that doesn't need the
/// stronger nudge.
pub const DEEP_FOCUS_MINUTES: u64 = 60;

/// Iter R62: hard-block threshold. At ≥90min same-app, the gate stops
/// asking the LLM to choose silence (R27 soft directive at 60+) and
/// just skips the proactive turn entirely. Rationale: at 90min+ the
/// false-positive cost of interrupting deep work clearly dominates the
/// missed-engagement cost; LLM-decides path is too risky and burns a
/// call. 90 = 60 (R27 trigger) + 30 (one full nominal interval), so
/// the hard band only kicks in *after* one full cycle of soft directive
/// has had its chance.
pub const HARD_FOCUS_BLOCK_MINUTES: u64 = 90;

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
/// Iter R27: when minutes ≥ `DEEP_FOCUS_MINUTES` (60), the line is
/// upgraded from informational to directive — explicitly tells the LLM
/// to consider sustaining silence so it doesn't break long flow.
/// Below that threshold but above MIN_DURATION_MINUTES, the original
/// "已经待了 N 分钟" descriptive form remains so 15-60min sustained
/// focus is acknowledged without the stronger nudge.
pub fn format_active_app_hint(app: &str, minutes: u64) -> String {
    if app.trim().is_empty() || minutes < MIN_DURATION_MINUTES {
        return String::new();
    }
    if minutes >= DEEP_FOCUS_MINUTES {
        format!(
            "用户在「{}」里已经待了 {} 分钟（深度专注期 ≥{}m）。这次开口应当极简或选择沉默，避免打断长时间工作流。",
            app, minutes, DEEP_FOCUS_MINUTES
        )
    } else {
        format!("用户在「{}」里已经待了 {} 分钟。", app, minutes)
    }
}

/// Iter R62: pure helper. Returns Some(minutes) when the user has been
/// in the same app for ≥ `threshold_minutes`, signaling the gate should
/// hard-skip proactive turns. None when there's no prior snapshot or
/// the duration is below threshold. Pure — caller passes the snapshot
/// (from LAST_ACTIVE_APP) plus `now` so unit tests can drive every
/// branch without touching the static or system clock.
pub fn compute_deep_focus_block(
    prev: Option<&ActiveAppSnapshot>,
    threshold_minutes: u64,
    now: Instant,
) -> Option<u64> {
    let p = prev?;
    let mins = now.saturating_duration_since(p.since).as_secs() / 60;
    if mins >= threshold_minutes {
        Some(mins)
    } else {
        None
    }
}

// Iter R62: production wrapper `deep_focus_block_minutes()` was inlined
// in R64 — gate.rs now reads `LAST_ACTIVE_APP` + calls
// `compute_deep_focus_block` directly with a config-derived threshold,
// since the const-default 90 is no longer the only acceptable value.
// Pure helper `compute_deep_focus_block` remains the single source of
// truth.

/// Iter R63: bookkeeping for the most recent hard-block. Gate writes
/// this on every block tick (refreshing `marked_at` so the value reflects
/// the *last* block tick before user emerged). Run-path takes-and-clears
/// it within RECOVERY_HINT_GRACE_SECS so the first proactive turn after
/// the user releases a long deep-work session can surface a "你刚从 N
/// 分钟专注里切出来" recovery hint. Bridges R62 (gate skip) → next-run
/// context: pet feels attentive instead of jumping in cold.
#[derive(Clone, Debug)]
pub struct LastHardBlock {
    pub app: String,
    pub peak_minutes: u64,
    pub marked_at: Instant,
}

pub static LAST_HARD_BLOCK: std::sync::Mutex<Option<LastHardBlock>> = std::sync::Mutex::new(None);

/// Iter R63: how recent a block has to be for the recovery hint to
/// fire. 10 min = generous enough to cover ticks that get gated by
/// awaiting / cooldown / quiet hours right after the user emerges
/// (those gates also skip but don't fire the recovery hint —
/// take-on-run only happens inside run_proactive_turn). Past 10 min
/// the "刚切出来" framing stops being accurate.
pub const RECOVERY_HINT_GRACE_SECS: u64 = 600;

/// Iter R63: gate calls this on every hard-block tick. Idempotent
/// refresh — overwrites prior LastHardBlock with current values
/// so peak_minutes always reflects the latest block tick (which is
/// also the last value before the block clears).
pub fn record_hard_block(app: &str, peak_minutes: u64) {
    if let Ok(mut g) = LAST_HARD_BLOCK.lock() {
        *g = Some(LastHardBlock {
            app: app.to_string(),
            peak_minutes,
            marked_at: Instant::now(),
        });
    }
}

/// Iter R63: pure helper. Returns Some((app, minutes)) when the last
/// recorded hard-block is within `grace_window_secs` of `now`. None
/// when no block recorded or it's stale. Pure / testable — caller
/// passes both `last_block` and `now` so unit tests can drive every
/// branch deterministically.
pub fn compute_recovery_hint(
    last_block: Option<&LastHardBlock>,
    now: Instant,
    grace_window_secs: u64,
) -> Option<(String, u64)> {
    let b = last_block?;
    let age = now.saturating_duration_since(b.marked_at).as_secs();
    if age > grace_window_secs {
        return None;
    }
    Some((b.app.clone(), b.peak_minutes))
}

/// Iter R63: pure formatter. Returns empty string for empty app /
/// zero peak_minutes (defensive — those values shouldn't reach here
/// in production but the helper stays well-defined for tests).
pub fn format_deep_focus_recovery_hint(app: &str, peak_minutes: u64) -> String {
    if app.trim().is_empty() || peak_minutes == 0 {
        return String::new();
    }
    format!(
        "[刚结束深度专注] 用户刚从「{}」的 {} 分钟连续专注里切出来，可以温和打个招呼或建议歇会儿，不要追问任务进度。",
        app, peak_minutes
    )
}

/// Iter R63: production wrapper. Reads LAST_HARD_BLOCK + Instant::now(),
/// delegates to `compute_recovery_hint`, redacts the app name, formats
/// the hint string, and clears LAST_HARD_BLOCK on successful take. On
/// expiry / no-data, leaves the static intact and returns empty string.
/// Caller is `run_proactive_turn` — clears so the same recovery doesn't
/// fire twice across consecutive runs.
pub fn take_recovery_hint() -> String {
    let Ok(mut g) = LAST_HARD_BLOCK.lock() else {
        return String::new();
    };
    let info = g.clone();
    let Some((app, mins)) =
        compute_recovery_hint(info.as_ref(), Instant::now(), RECOVERY_HINT_GRACE_SECS)
    else {
        return String::new();
    };
    *g = None;
    let redacted = crate::redaction::redact_with_settings(&app);
    format_deep_focus_recovery_hint(&redacted, mins)
}

/// Iter R62: refresh just the snapshot without producing the prompt
/// hint. Gate calls this on every tick so deep-focus block sees fresh
/// state — `update_and_format_active_app_hint` only fires inside
/// `run_proactive_turn`, which doesn't run when the gate skips, leaving
/// the snapshot stale and the block stuck on. Idempotent: running it
/// twice in quick succession (gate + run_proactive_turn) carries the
/// same `since` forward when the app hasn't changed.
pub fn refresh_active_app_snapshot(current_app: Option<&str>) {
    let Some(app) = current_app else { return };
    let now = Instant::now();
    let prev = LAST_ACTIVE_APP.lock().ok().and_then(|g| g.clone());
    let (new_snapshot, _duration) = compute_active_duration(prev.as_ref(), app, now);
    if let Ok(mut g) = LAST_ACTIVE_APP.lock() {
        *g = Some(new_snapshot);
    }
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
        // R27: 240 min is now in the deep-focus band (≥60), so the directive
        // line fires. Preserve the original assertion that app + minutes
        // are present, plus add the new directive marker.
        let out = format_active_app_hint("Slack", 240);
        assert!(out.contains("240"));
        assert!(out.contains("Slack"));
        assert!(out.contains("深度专注期"));
    }

    #[test]
    fn format_below_deep_focus_threshold_uses_descriptive_form() {
        // R27: 15-59 min stays in the original informational form (no
        // "深度专注期" / "极简或选择沉默" directive).
        let out = format_active_app_hint("Cursor", 30);
        assert!(out.contains("Cursor"));
        assert!(out.contains("30 分钟"));
        assert!(
            !out.contains("深度专注期"),
            "30m should NOT trigger deep-focus: {}",
            out
        );
        assert!(!out.contains("极简"));
    }

    #[test]
    fn format_at_deep_focus_threshold_fires_directive() {
        // R27: exactly 60 = boundary, gate is `>=`, should fire.
        let out = format_active_app_hint("Xcode", DEEP_FOCUS_MINUTES);
        assert!(out.contains("深度专注期"));
        assert!(out.contains("极简或选择沉默"));
    }

    #[test]
    fn format_above_deep_focus_threshold_fires_directive() {
        // R27: 90 min — clearly deep focus.
        let out = format_active_app_hint("IntelliJ", 90);
        assert!(out.contains("深度专注期"));
        assert!(out.contains("打断长时间工作流"));
        assert!(out.contains("90"));
    }

    #[test]
    fn format_just_below_deep_focus_keeps_descriptive() {
        // R27: 59 = boundary minus one, should NOT fire deep-focus.
        let out = format_active_app_hint("Terminal", DEEP_FOCUS_MINUTES - 1);
        assert!(!out.contains("深度专注期"));
        assert!(out.contains("已经待了"));
    }

    // -- Iter R62: compute_deep_focus_block tests ---------------------------

    #[test]
    fn deep_focus_block_returns_none_without_snapshot() {
        let now = Instant::now();
        assert_eq!(
            compute_deep_focus_block(None, HARD_FOCUS_BLOCK_MINUTES, now),
            None
        );
    }

    #[test]
    fn deep_focus_block_returns_none_below_threshold() {
        // 89 min in same app = R27 soft directive territory, NOT hard block.
        let then = Instant::now() - std::time::Duration::from_secs(89 * 60);
        let prev = snap("Cursor", then);
        let now = Instant::now();
        assert_eq!(
            compute_deep_focus_block(Some(&prev), HARD_FOCUS_BLOCK_MINUTES, now),
            None
        );
    }

    #[test]
    fn deep_focus_block_fires_at_threshold() {
        // Exactly 90 min — boundary uses `>=`, should fire.
        let then = Instant::now() - std::time::Duration::from_secs(90 * 60);
        let prev = snap("Cursor", then);
        let now = Instant::now();
        let mins = compute_deep_focus_block(Some(&prev), HARD_FOCUS_BLOCK_MINUTES, now)
            .expect("should fire at threshold");
        // Test-runtime slop tolerated: 89-90.
        assert!((89..=90).contains(&mins), "expected ~90 min, got {}", mins);
    }

    #[test]
    fn deep_focus_block_fires_above_threshold() {
        // 3 hours in same app — clear hard-block territory.
        let then = Instant::now() - std::time::Duration::from_secs(180 * 60);
        let prev = snap("IntelliJ", then);
        let now = Instant::now();
        let mins = compute_deep_focus_block(Some(&prev), HARD_FOCUS_BLOCK_MINUTES, now)
            .expect("should fire at 180 min");
        assert!(mins >= 179);
    }

    #[test]
    fn deep_focus_block_returns_none_just_below_threshold() {
        // 89:30 = below the 90-min boundary. Even with second-level
        // precision, integer minutes math returns 89.
        let then = Instant::now() - std::time::Duration::from_secs(89 * 60 + 30);
        let prev = snap("Slack", then);
        let now = Instant::now();
        assert_eq!(
            compute_deep_focus_block(Some(&prev), HARD_FOCUS_BLOCK_MINUTES, now),
            None
        );
    }

    #[test]
    fn deep_focus_block_respects_custom_threshold() {
        // Pure helper takes threshold as arg so callers can tune. Verify
        // a different threshold (e.g. 30) fires earlier than the default.
        let then = Instant::now() - std::time::Duration::from_secs(35 * 60);
        let prev = snap("Cursor", then);
        let now = Instant::now();
        let mins = compute_deep_focus_block(Some(&prev), 30, now)
            .expect("custom 30-min threshold should fire at 35 min");
        assert!(mins >= 34);
    }

    // -- Iter R62: refresh_active_app_snapshot tests ------------------------

    #[test]
    fn refresh_snapshot_handles_none_app() {
        // Non-macOS / failed osascript → noop, no panic.
        // We can't assert on the static (test isolation), but verify the
        // call returns without crashing.
        refresh_active_app_snapshot(None);
    }

    #[test]
    fn refresh_snapshot_writes_through_when_some_app() {
        // Lock the static directly to verify the write — single-threaded
        // test, but other tests may also touch LAST_ACTIVE_APP. Reset
        // before / restore after to avoid interfering.
        let original = LAST_ACTIVE_APP.lock().ok().and_then(|g| g.clone());
        if let Ok(mut g) = LAST_ACTIVE_APP.lock() {
            *g = None;
        }
        refresh_active_app_snapshot(Some("R62TestApp"));
        let after = LAST_ACTIVE_APP.lock().ok().and_then(|g| g.clone());
        assert_eq!(after.as_ref().map(|s| s.app.as_str()), Some("R62TestApp"));
        // Restore.
        if let Ok(mut g) = LAST_ACTIVE_APP.lock() {
            *g = original;
        }
    }

    // -- Iter R63: compute_recovery_hint tests ------------------------------

    fn block(app: &str, mins: u64, marked_at: Instant) -> LastHardBlock {
        LastHardBlock {
            app: app.to_string(),
            peak_minutes: mins,
            marked_at,
        }
    }

    #[test]
    fn recovery_hint_returns_none_when_no_block() {
        let now = Instant::now();
        assert_eq!(compute_recovery_hint(None, now, 600), None);
    }

    #[test]
    fn recovery_hint_returns_some_when_fresh() {
        // Block recorded 30s ago, well within 10-min grace.
        let then = Instant::now() - std::time::Duration::from_secs(30);
        let b = block("Cursor", 95, then);
        let now = Instant::now();
        let result = compute_recovery_hint(Some(&b), now, 600);
        assert_eq!(result, Some(("Cursor".to_string(), 95)));
    }

    #[test]
    fn recovery_hint_returns_none_when_stale() {
        // Block recorded 11 min ago, past 10-min grace window.
        let then = Instant::now() - std::time::Duration::from_secs(11 * 60);
        let b = block("Slack", 120, then);
        let now = Instant::now();
        assert_eq!(compute_recovery_hint(Some(&b), now, 600), None);
    }

    #[test]
    fn recovery_hint_at_exact_grace_boundary_fires() {
        // Boundary: gate uses `>` for stale, so age == grace_secs is still fresh.
        let then = Instant::now() - std::time::Duration::from_secs(600);
        let b = block("VS Code", 90, then);
        let now = Instant::now();
        let result = compute_recovery_hint(Some(&b), now, 600);
        // Allow for runtime slop pushing age slightly past 600.
        assert!(result.is_none() || matches!(result, Some(ref r) if r.1 == 90));
    }

    #[test]
    fn recovery_hint_just_past_grace_returns_none() {
        // Clearly stale: 601s = 1s past grace window.
        let then = Instant::now() - std::time::Duration::from_secs(601);
        let b = block("Xcode", 100, then);
        let now = Instant::now();
        assert_eq!(compute_recovery_hint(Some(&b), now, 600), None);
    }

    // -- Iter R63: format_deep_focus_recovery_hint tests --------------------

    #[test]
    fn format_recovery_hint_includes_app_and_minutes() {
        let out = format_deep_focus_recovery_hint("Cursor", 95);
        assert!(out.contains("Cursor"));
        assert!(out.contains("95"));
        assert!(out.contains("刚结束深度专注"));
        assert!(out.contains("切出来"));
        assert!(out.contains("不要追问任务进度"));
    }

    #[test]
    fn format_recovery_hint_returns_empty_for_blank_app() {
        assert_eq!(format_deep_focus_recovery_hint("", 90), "");
        assert_eq!(format_deep_focus_recovery_hint("   ", 90), "");
    }

    #[test]
    fn format_recovery_hint_returns_empty_for_zero_minutes() {
        // Defensive: peak_minutes==0 is impossible in production (only set
        // when block fires at ≥ 90), but the pure helper stays well-defined.
        assert_eq!(format_deep_focus_recovery_hint("Cursor", 0), "");
    }

    // -- Iter R63: record_hard_block + take_recovery_hint integration -------

    #[test]
    fn record_and_take_round_trip_clears_state() {
        // Ensure clean slate; restore at end so other tests aren't disturbed.
        let original = LAST_HARD_BLOCK.lock().ok().and_then(|g| g.clone());
        if let Ok(mut g) = LAST_HARD_BLOCK.lock() {
            *g = None;
        }

        record_hard_block("R63TestApp", 91);
        let hint = take_recovery_hint();
        assert!(hint.contains("R63TestApp"));
        assert!(hint.contains("91"));

        // Take should clear, so a second take returns empty.
        let hint2 = take_recovery_hint();
        assert_eq!(hint2, "");

        if let Ok(mut g) = LAST_HARD_BLOCK.lock() {
            *g = original;
        }
    }

    #[test]
    fn take_recovery_hint_returns_empty_when_no_record() {
        let original = LAST_HARD_BLOCK.lock().ok().and_then(|g| g.clone());
        if let Ok(mut g) = LAST_HARD_BLOCK.lock() {
            *g = None;
        }
        assert_eq!(take_recovery_hint(), "");
        if let Ok(mut g) = LAST_HARD_BLOCK.lock() {
            *g = original;
        }
    }
}
