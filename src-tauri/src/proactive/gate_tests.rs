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
            chatty_day_threshold: 5,
            // Iter R13: tests run in balanced mode so companion-mode multipliers
            // don't fold into existing gate-test expectations.
            companion_mode: "balanced".to_string(),
            // 心跳与 gate 逻辑无关：默认 0（禁用）保持 gate 单测纯净。
            task_heartbeat_minutes: 0,
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
            c.cooldown_seconds,
        );
        assert!(result.is_ok(), "disabled quiet hours shouldn't gate");
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
