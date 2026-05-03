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

/// Iter R52: read MUTE_UNTIL and return remaining seconds when active,
/// else None. Pure-ish (depends on chrono::Local::now()) but separated
/// for testability — gate calls this; ToneSnapshot reads same static
/// so panel chip and gate stay aligned.
pub fn mute_remaining_seconds() -> Option<i64> {
    let until = MUTE_UNTIL.lock().ok().and_then(|g| *g)?;
    let now = chrono::Local::now();
    let remaining = (until - now).num_seconds();
    if remaining > 0 {
        Some(remaining)
    } else {
        None
    }
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
    let effective_cooldown = match crate::feedback_history::negative_signal_ratio(&recent_fb) {
        Some((ratio, n)) => {
            crate::feedback_history::adapted_cooldown_seconds(mode_cooldown, ratio, n)
        }
        None => mode_cooldown,
    };

    if let Err(action) = evaluate_pre_input_idle(
        cfg,
        &snap,
        hour,
        focus_active,
        wake_seconds_ago,
        effective_cooldown,
    ) {
        return action;
    }
    let input_idle = user_input_idle_seconds().await;
    evaluate_input_idle_gate(cfg, &snap, input_idle)
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
}
