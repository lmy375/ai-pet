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
/// - `wake_seconds_ago`: how long ago the proactive loop last detected a wake-from-sleep.
///   Within `WAKE_GRACE_WINDOW_SECS` we soften cooldown + idle gates so the pet can
///   greet the returning user rather than wait out the normal nap.
pub fn evaluate_pre_input_idle(
    cfg: &crate::commands::settings::ProactiveConfig,
    snap: &ClockSnapshot,
    hour: u8,
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
    // Gate 4: minimum quiet time since last interaction. Below threshold = silent skip
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
    let clock = app.state::<InteractionClockStore>().inner().clone();
    let snap = clock.snapshot().await;
    let hour = chrono::Local::now().hour() as u8;
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
#[path = "gate_tests.rs"]
mod tests;
