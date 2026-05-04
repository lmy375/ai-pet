//! Proactive-loop gate logic (Iter QG5d extraction from `proactive.rs`).
//!
//! This module decides what the proactive loop should *do* on each tick.
//! Composes a series of gates in priority order:
//! 1. Disabled flag.
//! 2. Awaiting reply to a previous proactive utterance.
//! 3. Cooldown since last proactive (Iter R7-adapted from feedback ratio).
//! 4. Quiet hours.
//! 5. macOS Focus / DND.
//! 6. Idle threshold (with wake-from-sleep softening).
//! 7. Input-idle (don't interrupt active keystrokes).
//!
//! The first 5 are pure (no IO) — `evaluate_pre_input_idle` returns
//! `Result<(), LoopAction>`. Step 6+7 require async IO; `evaluate_loop_tick`
//! orchestrates the whole sequence.
//!
//! Public surface preserved via glob `pub use self::gate::*` at the top of
//! `proactive.rs`. The spawn loop body (still in `proactive.rs`) consumes
//! `LoopAction` via the re-export.

use chrono::Timelike;
use tauri::{AppHandle, Manager};

use super::{ClockSnapshot, InteractionClockStore};
use crate::input_idle::user_input_idle_seconds;

/// What the proactive loop should do this tick. Each variant maps to one outer-loop branch:
/// `Silent` skips quietly, `Skip` logs the reason, `Run` triggers a real proactive turn.
/// All variants now carry a debug reason so the panel can show *why* a tick was silent
/// (disabled / quiet hours / idle short — these used to be indistinguishable in the UI).
#[derive(Debug, PartialEq, Eq)]
pub enum LoopAction {
    /// No log, just sleep — used when proactive is disabled or the user simply hasn't been
    /// idle long enough yet (the common case, not interesting). Static reason so the
    /// recorder can show which silent path was taken.
    Silent { reason: &'static str },
    /// Log the reason then sleep — guard fired (awaiting / cooldown / user-active).
    Skip(String),
    /// All gates passed; fire a proactive turn with these idle stats.
    Run {
        idle_seconds: u64,
        input_idle_seconds: Option<u64>,
    },
}

/// Length of the wake-from-sleep grace window (seconds). Within this window after a
/// detected wake, cooldown is treated as elapsed and idle threshold is halved — so the
/// pet is more eager to greet a returning user. Awaiting / focus / quiet gates stay
/// untouched: those reflect user preference, wake doesn't override them.
pub const WAKE_GRACE_WINDOW_SECS: u64 = 600;

/// True when a wake event happened recently enough to soften the gates.
pub fn wake_recent(wake_seconds_ago: Option<u64>) -> bool {
    matches!(wake_seconds_ago, Some(s) if s <= WAKE_GRACE_WINDOW_SECS)
}

/// Pure-data gates that don't need IO. Returns `Err(action)` with the final LoopAction to
/// short-circuit the tick, or `Ok(())` signaling "all sync gates passed, caller should run
/// the input-idle gate next". Inputs:
/// - `hour`: local 24-hour clock (0–23), injected for testability
/// - `focus_active`: macOS Focus state, `None` means unknown/non-macOS (gate is no-op)
/// - `wake_seconds_ago`: how long ago the proactive loop last detected a wake-from-sleep.
///   Within `WAKE_GRACE_WINDOW_SECS` we soften cooldown + idle gates so the pet can
///   greet the returning user rather than wait out the normal nap.
pub fn evaluate_pre_input_idle(
    cfg: &crate::commands::settings::ProactiveConfig,
    snap: &ClockSnapshot,
    hour: u8,
    focus_active: Option<bool>,
    wake_seconds_ago: Option<u64>,
    effective_cooldown_seconds: u64,
) -> Result<(), LoopAction> {
    if !cfg.enabled {
        return Err(LoopAction::Silent { reason: "disabled" });
    }
    // Gate 1: a real friend doesn't keep talking when ignored.
    if snap.awaiting_user_reply {
        return Err(LoopAction::Skip(
            "Proactive: skip — awaiting user reply to previous proactive message".into(),
        ));
    }
    // Gate 2: cooldown since the last proactive utterance, regardless of user idle.
    // Wake softens this — when the user has been away, the cooldown's "don't double up"
    // intent doesn't apply (the prior utterance was probably hours ago anyway).
    // Iter R7: `effective_cooldown_seconds` is computed by the caller from
    // `cfg.cooldown_seconds` + `feedback_history::adapted_cooldown_seconds`,
    // so the gate honors a high-ignore-ratio "back off 2x" / low-ignore-ratio
    // "speak more freely 0.7x" adjustment without the gate fn doing async IO.
    let wake_soft = wake_recent(wake_seconds_ago);
    if let Some(since) = snap.since_last_proactive_seconds {
        if !wake_soft && effective_cooldown_seconds > 0 && since < effective_cooldown_seconds {
            return Err(LoopAction::Skip(format!(
                "Proactive: skip — cooldown ({}s < {}s)",
                since, effective_cooldown_seconds
            )));
        }
    }
    // Gate 3: quiet hours. A real friend lets you sleep. Silent skip rather than logged
    // skip — this can happen on every tick during night, no value in spamming logs.
    if super::in_quiet_hours(hour, cfg.quiet_hours_start, cfg.quiet_hours_end) {
        return Err(LoopAction::Silent {
            reason: "quiet_hours",
        });
    }
    // Gate 4: macOS Focus / DND. The user explicitly opted into "don't disturb me", so
    // skip with a logged reason (less frequent than nightly quiet hours, worth surfacing).
    if cfg.respect_focus_mode && focus_active == Some(true) {
        return Err(LoopAction::Skip(
            "Proactive: skip — macOS Focus / Do-Not-Disturb is active".into(),
        ));
    }
    // Gate 5: minimum quiet time since last interaction. Below threshold = silent skip
    // (this is the common idle-yet case, not worth logging on every tick). Wake softens
    // by halving the threshold (still floor 60s) — user just got back, "haven't been
    // idle long" doesn't really mean what it usually means.
    let raw_threshold = cfg.idle_threshold_seconds.max(60);
    let threshold = if wake_soft {
        (raw_threshold / 2).max(60)
    } else {
        raw_threshold
    };
    if snap.idle_seconds < threshold {
        return Err(LoopAction::Silent {
            reason: "idle_below_threshold",
        });
    }
    Ok(())
}

/// Gate 4: input-idle. Don't interrupt while the user is actively at the keyboard/mouse.
/// `input_idle_seconds = 0` disables the gate; `input_idle = None` (non-macOS) is treated
/// as a pass so behavior degrades to "rely on the interaction-time gate only".
pub fn evaluate_input_idle_gate(
    cfg: &crate::commands::settings::ProactiveConfig,
    snap: &ClockSnapshot,
    input_idle: Option<u64>,
) -> LoopAction {
    let input_ok = match (cfg.input_idle_seconds, input_idle) {
        (0, _) => true,
        (_, Some(secs)) => secs >= cfg.input_idle_seconds,
        (_, None) => true,
    };
    if !input_ok {
        return LoopAction::Skip(format!(
            "Proactive: skip — user active (input_idle={}s < {}s)",
            input_idle.unwrap_or(0),
            cfg.input_idle_seconds
        ));
    }
    LoopAction::Run {
        idle_seconds: snap.idle_seconds,
        input_idle_seconds: input_idle,
    }
}

/// Iter R52: transient mute. None when not muted; Some(t) means "skip
/// proactive turns until t". Set via `set_mute_minutes` Tauri command,
/// cleared via `clear_mute` or by reaching t. Distinct from
/// `proactive.enabled` setting (persistent toggle); MUTE_UNTIL is for
/// "shut up for the next hour while I focus" sessions.
pub static MUTE_UNTIL: std::sync::Mutex<Option<chrono::DateTime<chrono::Local>>> =
    std::sync::Mutex::new(None);

/// Iter R55: transient instruction note — distinct from mute (which fully
/// blocks). Note lets pet **still speak** but with user-supplied context
/// like "I'm in a meeting until 2pm" / "I'm not feeling well today, be
/// gentle". Stored as text + expiry. Injected into proactive prompt as
/// a directive; auto-clears at expiry like mute. text + until paired so
/// stale note can't outlive its meaning.
#[derive(Clone, Debug)]
pub struct TransientNote {
    pub text: String,
    pub until: chrono::DateTime<chrono::Local>,
}

pub static TRANSIENT_NOTE: std::sync::Mutex<Option<TransientNote>> = std::sync::Mutex::new(None);

/// Iter R55: pure helper computing whether the transient note is still
/// active (not expired). Returns the text when active, None otherwise.
/// Pure / testable — caller passes both `note` (from TRANSIENT_NOTE
/// static or anywhere) and `now` (for deterministic tests).
pub fn compute_transient_note_active(
    note: Option<&TransientNote>,
    now: chrono::DateTime<chrono::Local>,
) -> Option<String> {
    let n = note?;
    if n.until <= now {
        return None;
    }
    Some(n.text.clone())
}

/// Iter R55: production wrapper. Reads TRANSIENT_NOTE static + Local::now()
/// and delegates to `compute_transient_note_active`.
pub fn transient_note_active() -> Option<String> {
    let note = TRANSIENT_NOTE.lock().ok().and_then(|g| g.clone());
    compute_transient_note_active(note.as_ref(), chrono::Local::now())
}

/// Iter R56: pure helper computing remaining seconds for the transient
/// note. Mirrors `compute_mute_remaining` shape; symmetric pair (mute +
/// note) both expose remaining for chip / hover countdown. Returns None
/// when note is None or already expired (same boundary semantics: gate
/// releases at exact expiry, > 0 strict).
pub fn compute_transient_note_remaining(
    note: Option<&TransientNote>,
    now: chrono::DateTime<chrono::Local>,
) -> Option<i64> {
    let n = note?;
    let remaining = (n.until - now).num_seconds();
    if remaining > 0 {
        Some(remaining)
    } else {
        None
    }
}

/// Iter R56: production wrapper for `compute_transient_note_remaining`.
pub fn transient_note_remaining_seconds() -> Option<i64> {
    let note = TRANSIENT_NOTE.lock().ok().and_then(|g| g.clone());
    compute_transient_note_remaining(note.as_ref(), chrono::Local::now())
}

/// Iter R59: pure helper computing the new MUTE_UNTIL value given a
/// `minutes` request and current `now`. Returns None when minutes ≤ 0
/// (caller treats this as "clear"); Some(now + minutes) otherwise.
/// Pure / testable — extract setter logic from the Tauri command so
/// boundary cases (negative / zero) are unit-test verifiable.
pub fn compute_new_mute_until(
    minutes: i64,
    now: chrono::DateTime<chrono::Local>,
) -> Option<chrono::DateTime<chrono::Local>> {
    if minutes <= 0 {
        return None;
    }
    Some(now + chrono::Duration::minutes(minutes))
}

/// Iter R59: pure helper computing the new TRANSIENT_NOTE value. Empty
/// (or whitespace-only) text or minutes ≤ 0 → None (clear). Otherwise
/// Some(TransientNote { trimmed text, until = now + minutes }).
pub fn compute_new_transient_note(
    text: &str,
    minutes: i64,
    now: chrono::DateTime<chrono::Local>,
) -> Option<TransientNote> {
    let trimmed = text.trim();
    if trimmed.is_empty() || minutes <= 0 {
        return None;
    }
    Some(TransientNote {
        text: trimmed.to_string(),
        until: now + chrono::Duration::minutes(minutes),
    })
}

/// Iter R52 / R53: pure helper computing remaining mute seconds. Returns
/// `None` when `until` is None (never set / cleared) or already past.
/// Returns `Some(positive)` when mute is still active. Pure / testable —
/// caller passes both `until` (from MUTE_UNTIL static or anywhere) and
/// `now` (for deterministic tests). The non-pure wrapper
/// `mute_remaining_seconds()` reads the global state + uses
/// `chrono::Local::now()`.
pub fn compute_mute_remaining(
    until: Option<chrono::DateTime<chrono::Local>>,
    now: chrono::DateTime<chrono::Local>,
) -> Option<i64> {
    let until = until?;
    let remaining = (until - now).num_seconds();
    if remaining > 0 {
        Some(remaining)
    } else {
        None
    }
}

/// Iter R52: production wrapper around `compute_mute_remaining`. Reads
/// MUTE_UNTIL static + chrono::Local::now() and delegates. Gate calls
/// this; ToneSnapshot reads same — panel chip and gate stay aligned.
pub fn mute_remaining_seconds() -> Option<i64> {
    let until = MUTE_UNTIL.lock().ok().and_then(|g| *g);
    compute_mute_remaining(until, chrono::Local::now())
}

/// Evaluate every gate in priority order and return the action this tick should take.
/// Composes the pure pre-input-idle gates with the IO call to query keyboard/mouse idle.
pub async fn evaluate_loop_tick(
    app: &AppHandle,
    settings: &crate::commands::settings::AppSettings,
) -> LoopAction {
    // Iter R52: mute gate runs first — fastest exit when user has muted
    // the pet for a session. No IO, no settings read; just a static
    // mutex check. Returns immediately so muted ticks don't bother with
    // expensive gates below.
    if let Some(remaining) = mute_remaining_seconds() {
        let mins = remaining / 60;
        return LoopAction::Skip(format!("muted, {} min remaining", mins));
    }
    let cfg = &settings.proactive;
    // Iter R62: deep-focus hard-block gate. Refresh the active-app
    // snapshot first so we see fresh state on every tick (the existing
    // refresh inside `run_proactive_turn` only fires on actually-run
    // ticks; a stuck block would otherwise persist indefinitely after
    // the user switched apps). One osascript call per tick — same cost
    // as inside run_proactive_turn, additive but small (≤200ms / 60s).
    //
    // Iter R64: threshold honors companion_mode (chatty=135 / balanced=90 /
    // quiet=60), so chatty users keep getting engaged past 90min while
    // quiet users back off sooner. base = HARD_FOCUS_BLOCK_MINUTES const.
    let current_app = crate::tools::system_tools::current_active_window()
        .await
        .map(|(app, _win)| app);
    super::refresh_active_app_snapshot(current_app.as_deref());
    let hard_block_threshold = cfg.effective_hard_block_minutes(super::HARD_FOCUS_BLOCK_MINUTES);
    let block_minutes = {
        let prev = super::LAST_ACTIVE_APP.lock().ok().and_then(|g| g.clone());
        super::compute_deep_focus_block(
            prev.as_ref(),
            hard_block_threshold,
            std::time::Instant::now(),
        )
    };
    if let Some(mins) = block_minutes {
        // Iter R63: record the block so the next non-blocked proactive
        // turn (within RECOVERY_HINT_GRACE_SECS) can inject a recovery
        // hint. Reads the snapshot we just refreshed for the app name —
        // raw / un-redacted, redaction happens at hint-format time.
        if let Some(snap) = super::LAST_ACTIVE_APP.lock().ok().and_then(|g| g.clone()) {
            super::record_hard_block(&snap.app, mins);
        }
        return LoopAction::Skip(format!(
            "deep focus hard-block: {} min in same app (threshold {}m, mode {})",
            mins, hard_block_threshold, cfg.companion_mode
        ));
    }
    let clock = app.state::<InteractionClockStore>().inner().clone();
    let snap = clock.snapshot().await;
    let hour = chrono::Local::now().hour() as u8;
    // Only fetch focus state when the gate is enabled — saves a file read every tick.
    let focus_active = if cfg.respect_focus_mode {
        crate::focus_mode::focus_mode_active().await
    } else {
        None
    };
    let wake_seconds_ago = app
        .state::<crate::wake_detector::WakeDetectorStore>()
        .inner()
        .last_wake_seconds_ago()
        .await;

    // Iter R13: companion-mode preset shapes the base cadence (high-level
    // dial). Iter R7 ratio adaptation layers on top of that base. Order
    // matters — mode is user intent, R7 is observed-feedback fine-tune.
    let mode_cooldown = cfg.effective_cooldown_base();
    let recent_fb = crate::feedback_history::recent_feedback(20).await;
    let after_feedback = match crate::feedback_history::negative_signal_ratio(&recent_fb) {
        Some((ratio, n)) => {
            crate::feedback_history::adapted_cooldown_seconds(mode_cooldown, ratio, n)
        }
        None => mode_cooldown,
    };
    // Iter R81: a real partner doesn't keep its quiet rhythm when the user
    // has an Imminent or Overdue butler deadline. Halve the effective
    // cooldown while urgent deadlines exist so the proactive loop fires
    // ~2× more often. Pure switch via `deadline_urgency_factor`; same
    // count the panel ⏳ deadline chip uses, so chip + gate stay aligned.
    let urgent_deadline_count: u64 = {
        let now = chrono::Local::now().naive_local();
        let items: Vec<(chrono::NaiveDateTime, String)> =
            crate::commands::memory::memory_list(Some("butler_tasks".to_string()))
                .ok()
                .and_then(|idx| idx.categories.get("butler_tasks").cloned())
                .map(|cat| {
                    cat.items
                        .iter()
                        .filter_map(|i| {
                            super::butler_schedule::parse_butler_deadline_prefix(&i.description)
                        })
                        .collect()
                })
                .unwrap_or_default();
        super::butler_schedule::count_urgent_butler_deadlines(&items, now)
    };
    let deadline_factor = super::butler_schedule::deadline_urgency_factor(urgent_deadline_count);
    let effective_cooldown = ((after_feedback as f64) * deadline_factor) as u64;

    if let Err(action) = evaluate_pre_input_idle(
        cfg,
        &snap,
        hour,
        focus_active,
        wake_seconds_ago,
        effective_cooldown,
    ) {
        // Iter R82: when R81's deadline shrink was active but cooldown
        // still won, surface that in the decision_log so the timeline
        // shows "cooldown stuck even with deadline accelerating things".
        // Otherwise the log reads identically with/without R81 and the
        // panel can't tell whether deadline urgency was even considered.
        return annotate_skip_with_deadline_factor(action, deadline_factor);
    }
    let input_idle = user_input_idle_seconds().await;
    evaluate_input_idle_gate(cfg, &snap, input_idle)
}

/// Iter R82: pure helper. When `deadline_factor < 1.0` (R81's urgent
/// shrink was active) and the gate Skip is the cooldown variant, suffix
/// the message with `[deadline-shrunk × N]` so the decision_log shows
/// that R81 *was* trying to help but the user-side cooldown still
/// hadn't elapsed. Other skip reasons + non-shrunk paths pass through
/// untouched. Testable without Tauri / async.
pub fn annotate_skip_with_deadline_factor(action: LoopAction, deadline_factor: f64) -> LoopAction {
    if deadline_factor >= 1.0 {
        return action;
    }
    match action {
        LoopAction::Skip(msg) if msg.contains("cooldown") => LoopAction::Skip(format!(
            "{} [deadline-shrunk × {:.1}]",
            msg, deadline_factor
        )),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::settings::ProactiveConfig;

    fn cfg() -> ProactiveConfig {
        ProactiveConfig {
            enabled: true,
            interval_seconds: 60,
            idle_threshold_seconds: 60,
            input_idle_seconds: 60,
            cooldown_seconds: 0, // off by default in tests so we can hit other gates
            // Quiet hours disabled by default in tests (start == end). Cases that need
            // it active set the values explicitly.
            quiet_hours_start: 0,
            quiet_hours_end: 0,
            respect_focus_mode: true,
            chatty_day_threshold: 5,
            // Iter R13: tests run in balanced mode so companion-mode multipliers
            // don't fold into existing gate-test expectations.
            companion_mode: "balanced".to_string(),
        }
    }

    fn snap(idle: u64, awaiting: bool, since_proactive: Option<u64>) -> ClockSnapshot {
        ClockSnapshot {
            idle_seconds: idle,
            awaiting_user_reply: awaiting,
            since_last_proactive_seconds: since_proactive,
        }
    }

    /// Wall clock hour known to be outside any quiet window we configure in tests.
    const NOON: u8 = 12;

    #[test]
    fn disabled_returns_silent() {
        let mut c = cfg();
        c.enabled = false;
        let action = evaluate_pre_input_idle(
            &c,
            &snap(9999, false, None),
            NOON,
            None,
            None,
            c.cooldown_seconds,
        );
        assert_eq!(
            action.unwrap_err(),
            LoopAction::Silent { reason: "disabled" }
        );
    }

    #[test]
    fn awaiting_user_reply_skips_with_log() {
        let action = evaluate_pre_input_idle(
            &cfg(),
            &snap(9999, true, None),
            NOON,
            None,
            None,
            cfg().cooldown_seconds,
        )
        .unwrap_err();
        match action {
            LoopAction::Skip(msg) => assert!(msg.contains("awaiting user reply")),
            other => panic!("expected Skip, got {:?}", other),
        }
    }

    #[test]
    fn cooldown_active_skips() {
        let mut c = cfg();
        c.cooldown_seconds = 1800;
        let action = evaluate_pre_input_idle(
            &c,
            &snap(9999, false, Some(60)),
            NOON,
            None,
            None,
            c.cooldown_seconds,
        )
        .unwrap_err();
        match action {
            LoopAction::Skip(msg) => {
                assert!(msg.contains("cooldown"));
                assert!(msg.contains("60s < 1800s"));
            }
            other => panic!("expected Skip, got {:?}", other),
        }
    }

    #[test]
    fn cooldown_zero_disables_gate() {
        let mut c = cfg();
        c.cooldown_seconds = 0;
        let result = evaluate_pre_input_idle(
            &c,
            &snap(9999, false, Some(0)),
            NOON,
            None,
            None,
            c.cooldown_seconds,
        );
        assert!(result.is_ok(), "expected Ok, got {:?}", result.unwrap_err());
    }

    #[test]
    fn cooldown_elapsed_passes() {
        let mut c = cfg();
        c.cooldown_seconds = 1800;
        let result = evaluate_pre_input_idle(
            &c,
            &snap(9999, false, Some(2000)),
            NOON,
            None,
            None,
            c.cooldown_seconds,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn idle_below_threshold_silent() {
        let c = cfg(); // threshold=60
        let action = evaluate_pre_input_idle(
            &c,
            &snap(30, false, None),
            NOON,
            None,
            None,
            c.cooldown_seconds,
        )
        .unwrap_err();
        assert_eq!(
            action,
            LoopAction::Silent {
                reason: "idle_below_threshold"
            }
        );
    }

    #[test]
    fn idle_threshold_clamped_to_60_minimum() {
        let mut c = cfg();
        c.idle_threshold_seconds = 10;
        let action = evaluate_pre_input_idle(
            &c,
            &snap(30, false, None),
            NOON,
            None,
            None,
            c.cooldown_seconds,
        )
        .unwrap_err();
        assert_eq!(
            action,
            LoopAction::Silent {
                reason: "idle_below_threshold"
            },
            "30s should still be below the clamped 60s"
        );
    }

    #[test]
    fn all_sync_gates_pass_returns_ok() {
        let result = evaluate_pre_input_idle(
            &cfg(),
            &snap(9999, false, None),
            NOON,
            None,
            None,
            cfg().cooldown_seconds,
        );
        assert!(result.is_ok());
    }

    // ---- quiet hours gate inside evaluate_pre_input_idle ----

    #[test]
    fn quiet_hours_silent_during_window() {
        let mut c = cfg();
        c.quiet_hours_start = 23;
        c.quiet_hours_end = 7;
        let action = evaluate_pre_input_idle(
            &c,
            &snap(9999, false, None),
            3,
            None,
            None,
            c.cooldown_seconds,
        )
        .unwrap_err();
        assert_eq!(
            action,
            LoopAction::Silent {
                reason: "quiet_hours"
            }
        );
    }

    #[test]
    fn quiet_hours_passes_outside_window() {
        let mut c = cfg();
        c.quiet_hours_start = 23;
        c.quiet_hours_end = 7;
        let result = evaluate_pre_input_idle(
            &c,
            &snap(9999, false, None),
            14,
            None,
            None,
            c.cooldown_seconds,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn quiet_hours_disabled_does_not_block() {
        let c = cfg();
        let result = evaluate_pre_input_idle(
            &c,
            &snap(9999, false, None),
            3,
            None,
            None,
            c.cooldown_seconds,
        );
        assert!(result.is_ok(), "disabled quiet hours shouldn't gate");
    }

    // ---- focus-mode gate ----

    #[test]
    fn focus_mode_active_skips_when_respected() {
        let action = evaluate_pre_input_idle(
            &cfg(),
            &snap(9999, false, None),
            NOON,
            Some(true),
            None,
            cfg().cooldown_seconds,
        )
        .unwrap_err();
        match action {
            LoopAction::Skip(msg) => assert!(msg.contains("Focus")),
            other => panic!("expected Skip, got {:?}", other),
        }
    }

    #[test]
    fn focus_mode_active_passes_when_disabled_in_settings() {
        let mut c = cfg();
        c.respect_focus_mode = false;
        let result = evaluate_pre_input_idle(
            &c,
            &snap(9999, false, None),
            NOON,
            Some(true),
            None,
            c.cooldown_seconds,
        );
        assert!(result.is_ok(), "user opted out of focus respect");
    }

    #[test]
    fn focus_mode_inactive_passes() {
        let result = evaluate_pre_input_idle(
            &cfg(),
            &snap(9999, false, None),
            NOON,
            Some(false),
            None,
            cfg().cooldown_seconds,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn focus_mode_unknown_passes() {
        let result = evaluate_pre_input_idle(
            &cfg(),
            &snap(9999, false, None),
            NOON,
            None,
            None,
            cfg().cooldown_seconds,
        );
        assert!(result.is_ok());
    }

    // ---- wake-from-sleep softening ----

    #[test]
    fn wake_recent_skips_cooldown_gate() {
        let mut c = cfg();
        c.cooldown_seconds = 1800;
        let result = evaluate_pre_input_idle(
            &c,
            &snap(9999, false, Some(60)),
            NOON,
            None,
            Some(120),
            c.cooldown_seconds,
        );
        assert!(result.is_ok(), "wake should soften cooldown");
    }

    #[test]
    fn wake_does_not_soften_after_grace_window() {
        let mut c = cfg();
        c.cooldown_seconds = 1800;
        let action = evaluate_pre_input_idle(
            &c,
            &snap(9999, false, Some(60)),
            NOON,
            None,
            Some(700),
            c.cooldown_seconds,
        )
        .unwrap_err();
        match action {
            LoopAction::Skip(msg) => assert!(msg.contains("cooldown")),
            other => panic!("expected Skip after grace, got {:?}", other),
        }
    }

    #[test]
    fn wake_recent_halves_idle_threshold() {
        let mut c = cfg();
        c.idle_threshold_seconds = 200;
        let result = evaluate_pre_input_idle(
            &c,
            &snap(120, false, None),
            NOON,
            None,
            Some(60),
            c.cooldown_seconds,
        );
        assert!(
            result.is_ok(),
            "wake should halve threshold so idle 120 passes 200/2 = 100"
        );
    }

    #[test]
    fn wake_idle_floor_60s() {
        let mut c = cfg();
        c.idle_threshold_seconds = 100;
        let action = evaluate_pre_input_idle(
            &c,
            &snap(30, false, None),
            NOON,
            None,
            Some(60),
            c.cooldown_seconds,
        )
        .unwrap_err();
        assert_eq!(
            action,
            LoopAction::Silent {
                reason: "idle_below_threshold"
            }
        );
    }

    #[test]
    fn wake_does_not_bypass_awaiting() {
        let action = evaluate_pre_input_idle(
            &cfg(),
            &snap(9999, true, None),
            NOON,
            None,
            Some(60),
            cfg().cooldown_seconds,
        )
        .unwrap_err();
        match action {
            LoopAction::Skip(msg) => assert!(msg.contains("awaiting user reply")),
            other => panic!("expected awaiting Skip, got {:?}", other),
        }
    }

    #[test]
    fn wake_does_not_bypass_quiet_hours() {
        let mut c = cfg();
        c.quiet_hours_start = 23;
        c.quiet_hours_end = 7;
        let action = evaluate_pre_input_idle(
            &c,
            &snap(9999, false, None),
            3,
            None,
            Some(60),
            c.cooldown_seconds,
        )
        .unwrap_err();
        assert_eq!(
            action,
            LoopAction::Silent {
                reason: "quiet_hours"
            }
        );
    }

    // ---- input-idle gate ----

    #[test]
    fn input_idle_zero_disables_gate_runs() {
        let mut c = cfg();
        c.input_idle_seconds = 0;
        let action = evaluate_input_idle_gate(&c, &snap(9999, false, None), Some(1));
        assert!(matches!(action, LoopAction::Run { .. }));
    }

    #[test]
    fn input_idle_none_treats_as_pass() {
        let action = evaluate_input_idle_gate(&cfg(), &snap(9999, false, None), None);
        match action {
            LoopAction::Run {
                input_idle_seconds, ..
            } => {
                assert_eq!(input_idle_seconds, None);
            }
            other => panic!("expected Run, got {:?}", other),
        }
    }

    #[test]
    fn input_idle_below_min_skips() {
        let action = evaluate_input_idle_gate(&cfg(), &snap(9999, false, None), Some(10));
        match action {
            LoopAction::Skip(msg) => {
                assert!(msg.contains("user active"));
                assert!(msg.contains("input_idle=10s < 60s"));
            }
            other => panic!("expected Skip, got {:?}", other),
        }
    }

    #[test]
    fn input_idle_above_min_runs() {
        let action = evaluate_input_idle_gate(&cfg(), &snap(9999, false, None), Some(120));
        match action {
            LoopAction::Run {
                idle_seconds,
                input_idle_seconds,
            } => {
                assert_eq!(idle_seconds, 9999);
                assert_eq!(input_idle_seconds, Some(120));
            }
            other => panic!("expected Run, got {:?}", other),
        }
    }

    // -- Iter R53: compute_mute_remaining tests -------------------------------

    fn now_at(
        year: i32,
        month: u32,
        day: u32,
        hour: u32,
        minute: u32,
    ) -> chrono::DateTime<chrono::Local> {
        use chrono::TimeZone;
        let naive = chrono::NaiveDate::from_ymd_opt(year, month, day)
            .unwrap()
            .and_hms_opt(hour, minute, 0)
            .unwrap();
        chrono::Local.from_local_datetime(&naive).unwrap()
    }

    #[test]
    fn mute_remaining_returns_none_when_until_is_none() {
        let now = now_at(2026, 5, 4, 10, 0);
        assert_eq!(compute_mute_remaining(None, now), None);
    }

    #[test]
    fn mute_remaining_returns_none_when_until_is_past() {
        // Mute set 5 min ago — already expired.
        let now = now_at(2026, 5, 4, 10, 0);
        let until = now - chrono::Duration::minutes(5);
        assert_eq!(compute_mute_remaining(Some(until), now), None);
    }

    #[test]
    fn mute_remaining_returns_none_when_until_equals_now() {
        // Boundary: gate should release at exactly now (remaining = 0 → None,
        // since the > 0 check excludes equality).
        let now = now_at(2026, 5, 4, 10, 0);
        assert_eq!(compute_mute_remaining(Some(now), now), None);
    }

    #[test]
    fn mute_remaining_returns_seconds_when_until_is_future() {
        let now = now_at(2026, 5, 4, 10, 0);
        let until = now + chrono::Duration::minutes(30);
        let remaining =
            compute_mute_remaining(Some(until), now).expect("future mute should return Some");
        assert_eq!(remaining, 30 * 60);
    }

    #[test]
    fn mute_remaining_handles_one_second_before_expiry() {
        // Edge case: 1 second left should still be Some(1), not None.
        let now = now_at(2026, 5, 4, 10, 0);
        let until = now + chrono::Duration::seconds(1);
        assert_eq!(compute_mute_remaining(Some(until), now), Some(1));
    }

    // -- Iter R55: compute_transient_note_active tests ------------------------

    #[test]
    fn transient_note_returns_none_when_note_is_none() {
        let now = now_at(2026, 5, 4, 10, 0);
        assert_eq!(compute_transient_note_active(None, now), None);
    }

    #[test]
    fn transient_note_returns_text_when_active() {
        let now = now_at(2026, 5, 4, 10, 0);
        let note = TransientNote {
            text: "I'm in a meeting".to_string(),
            until: now + chrono::Duration::minutes(30),
        };
        assert_eq!(
            compute_transient_note_active(Some(&note), now).as_deref(),
            Some("I'm in a meeting"),
        );
    }

    #[test]
    fn transient_note_returns_none_when_expired() {
        let now = now_at(2026, 5, 4, 10, 0);
        let note = TransientNote {
            text: "stale".to_string(),
            until: now - chrono::Duration::minutes(5),
        };
        assert_eq!(compute_transient_note_active(Some(&note), now), None);
    }

    #[test]
    fn transient_note_returns_none_at_exact_expiry() {
        // until == now → expired (boundary semantics: gate releases at
        // exact expiry, no extra second). Same idiom as compute_mute_remaining.
        let now = now_at(2026, 5, 4, 10, 0);
        let note = TransientNote {
            text: "edge".to_string(),
            until: now,
        };
        assert_eq!(compute_transient_note_active(Some(&note), now), None);
    }

    #[test]
    fn transient_note_one_second_before_expiry_still_active() {
        let now = now_at(2026, 5, 4, 10, 0);
        let note = TransientNote {
            text: "almost gone".to_string(),
            until: now + chrono::Duration::seconds(1),
        };
        assert_eq!(
            compute_transient_note_active(Some(&note), now).as_deref(),
            Some("almost gone"),
        );
    }

    // -- Iter R56: compute_transient_note_remaining tests ---------------------

    #[test]
    fn note_remaining_returns_none_when_note_is_none() {
        let now = now_at(2026, 5, 4, 10, 0);
        assert_eq!(compute_transient_note_remaining(None, now), None);
    }

    #[test]
    fn note_remaining_returns_seconds_when_active() {
        let now = now_at(2026, 5, 4, 10, 0);
        let note = TransientNote {
            text: "x".to_string(),
            until: now + chrono::Duration::minutes(45),
        };
        let remaining = compute_transient_note_remaining(Some(&note), now)
            .expect("active note should return Some");
        assert_eq!(remaining, 45 * 60);
    }

    #[test]
    fn note_remaining_returns_none_when_expired() {
        let now = now_at(2026, 5, 4, 10, 0);
        let note = TransientNote {
            text: "x".to_string(),
            until: now - chrono::Duration::minutes(5),
        };
        assert_eq!(compute_transient_note_remaining(Some(&note), now), None);
    }

    #[test]
    fn note_remaining_returns_none_at_exact_expiry() {
        // Boundary: > 0 strict, equality returns None — symmetric with
        // compute_mute_remaining.
        let now = now_at(2026, 5, 4, 10, 0);
        let note = TransientNote {
            text: "x".to_string(),
            until: now,
        };
        assert_eq!(compute_transient_note_remaining(Some(&note), now), None);
    }

    // -- Iter R59: compute_new_mute_until tests -------------------------------

    #[test]
    fn new_mute_until_returns_none_for_zero_minutes() {
        let now = now_at(2026, 5, 4, 10, 0);
        assert_eq!(compute_new_mute_until(0, now), None);
    }

    #[test]
    fn new_mute_until_returns_none_for_negative_minutes() {
        let now = now_at(2026, 5, 4, 10, 0);
        assert_eq!(compute_new_mute_until(-5, now), None);
        assert_eq!(compute_new_mute_until(-1, now), None);
    }

    #[test]
    fn new_mute_until_returns_now_plus_minutes_for_positive() {
        let now = now_at(2026, 5, 4, 10, 0);
        let until = compute_new_mute_until(30, now).expect("positive minutes → Some");
        let diff = (until - now).num_minutes();
        assert_eq!(diff, 30);
    }

    #[test]
    fn new_mute_until_returns_one_minute_for_minutes_equals_one() {
        let now = now_at(2026, 5, 4, 10, 0);
        let until = compute_new_mute_until(1, now).expect("1 min → Some");
        assert_eq!((until - now).num_minutes(), 1);
    }

    // -- Iter R59: compute_new_transient_note tests ---------------------------

    #[test]
    fn new_transient_note_returns_none_for_empty_text() {
        let now = now_at(2026, 5, 4, 10, 0);
        assert!(compute_new_transient_note("", 30, now).is_none());
    }

    #[test]
    fn new_transient_note_returns_none_for_whitespace_text() {
        let now = now_at(2026, 5, 4, 10, 0);
        // Caller's whitespace-only text should clear, not save a vacuous note.
        assert!(compute_new_transient_note("   ", 30, now).is_none());
        assert!(compute_new_transient_note("\t\n  \t", 30, now).is_none());
    }

    #[test]
    fn new_transient_note_returns_none_for_zero_or_negative_minutes() {
        let now = now_at(2026, 5, 4, 10, 0);
        assert!(compute_new_transient_note("valid text", 0, now).is_none());
        assert!(compute_new_transient_note("valid text", -10, now).is_none());
    }

    #[test]
    fn new_transient_note_trims_whitespace() {
        // Leading/trailing whitespace stripped; internal whitespace preserved.
        let now = now_at(2026, 5, 4, 10, 0);
        let note = compute_new_transient_note("  in a meeting until 2pm  ", 30, now)
            .expect("non-empty text + positive minutes → Some");
        assert_eq!(note.text, "in a meeting until 2pm");
    }

    #[test]
    fn new_transient_note_sets_until_correctly() {
        let now = now_at(2026, 5, 4, 10, 0);
        let note = compute_new_transient_note("hi", 60, now).unwrap();
        assert_eq!((note.until - now).num_minutes(), 60);
    }

    // -- Iter R82: annotate_skip_with_deadline_factor tests -----------------

    #[test]
    fn annotate_skip_passes_through_when_deadline_factor_one() {
        // Steady-state: no urgent deadline → factor 1.0 → no change.
        let action = LoopAction::Skip("Proactive: skip — cooldown (10s < 30s)".into());
        let out = annotate_skip_with_deadline_factor(action, 1.0);
        match out {
            LoopAction::Skip(msg) => assert_eq!(msg, "Proactive: skip — cooldown (10s < 30s)"),
            _ => panic!("expected Skip"),
        }
    }

    #[test]
    fn annotate_skip_appends_suffix_when_cooldown_and_shrunk() {
        // R81 shrunk cooldown but it still won — surface that in the log.
        let action = LoopAction::Skip("Proactive: skip — cooldown (10s < 30s)".into());
        let out = annotate_skip_with_deadline_factor(action, 0.5);
        match out {
            LoopAction::Skip(msg) => {
                assert!(msg.contains("cooldown (10s < 30s)"));
                assert!(msg.contains("[deadline-shrunk × 0.5]"));
            }
            _ => panic!("expected Skip"),
        }
    }

    #[test]
    fn annotate_skip_leaves_non_cooldown_skip_alone() {
        // Awaiting / focus / quiet skips don't need the suffix — R81's
        // shrink only relates to cooldown gate.
        let action = LoopAction::Skip(
            "Proactive: skip — awaiting user reply to previous proactive message".into(),
        );
        let out = annotate_skip_with_deadline_factor(action, 0.5);
        match out {
            LoopAction::Skip(msg) => assert!(!msg.contains("deadline-shrunk")),
            _ => panic!("expected Skip"),
        }
    }

    #[test]
    fn annotate_skip_leaves_silent_alone() {
        // Silent variants (disabled / quiet hours) don't carry user-meaningful
        // cooldown info. Pass through unchanged.
        let action = LoopAction::Silent { reason: "disabled" };
        let out = annotate_skip_with_deadline_factor(action, 0.5);
        match out {
            LoopAction::Silent { reason } => assert_eq!(reason, "disabled"),
            _ => panic!("expected Silent"),
        }
    }
}
