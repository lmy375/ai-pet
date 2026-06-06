//! ToneSnapshot + CooldownBreakdown + FeedbackSummary data types + the
//! builder + Tauri command wrapper. `commands::debug::get_debug_snapshot`
//! reuses `build_tone_snapshot` directly to share the same signal
//! ingredients without re-walking Tauri State plumbing.

use chrono::Datelike;
use chrono::Timelike;

use crate::commands::settings::get_settings;

use super::butler_schedule::{count_urgent_butler_deadlines, parse_butler_deadline_prefix};
use super::clock::{InteractionClock, InteractionClockStore};
use super::cooldown::build_cooldown_breakdown;
use super::gate::{
    mute_remaining_seconds, transient_note_active, transient_note_remaining_seconds,
};
use super::prompt_rules::{
    active_composite_rule_labels, active_data_driven_rule_labels,
    active_environmental_rule_labels, companionship_milestone, late_night_wellness_in_cooldown,
};
use super::reminder_hints::build_reminders_hint;
use super::session_helpers::build_plan_hint;
use super::telemetry::{count_trailing_silent, TurnRecord, LAST_PROACTIVE_PROMPT, LAST_PROACTIVE_TURNS};
use super::time_helpers::{
    format_day_of_week_hint, idle_tier, in_quiet_hours, minutes_until_quiet_start, period_of_day,
    user_absence_tier,
};

/// Snapshot of all the "conversational tone" signals the proactive prompt currently
/// uses. Exposed via `get_tone_snapshot` so the panel can render the same info the LLM
/// would see — handy for debugging "why did the pet say *that* right now?".
#[derive(serde::Serialize)]
pub struct ToneSnapshot {
    pub period: String,
    /// Cadence tier label, or None when this would be the pet's first proactive utterance.
    pub cadence: Option<String>,
    pub since_last_proactive_minutes: Option<u64>,
    pub wake_seconds_ago: Option<u64>,
    pub mood_text: Option<String>,
    pub mood_motion: Option<String>,
    /// Minutes until configured quiet hours kick in, when within the 15-min look-ahead.
    /// Lets the panel show "距安静时段 N 分钟" so the user can see why the pet is
    /// suddenly winding down.
    pub pre_quiet_minutes: Option<u64>,
    /// Lifetime count of proactive utterances, persisted in `speech_count.txt`.
    /// Doesn't saturate — used both by the icebreaker rule (< 3) and by the panel chip.
    pub proactive_count: u64,
    /// User-configured chatty-day threshold (`settings.proactive.chatty_day_threshold`).
    /// Surfaced so the panel can compare today's count against it and visually mark when
    /// the pet has crossed into "克制模式". 0 means the rule is disabled.
    pub chatty_day_threshold: u64,
    /// Labels for every data-driven contextual rule the proactive prompt currently has
    /// active (e.g. `["icebreaker", "chatty"]`). Empty when no data-driven rule is firing
    /// — the prompt is in its "neutral" state. Computed once on the backend so the panel
    /// doesn't need to know each rule's threshold logic.
    pub active_prompt_rules: Vec<String>,
    /// Iter D1: weekday + weekend/weekday combined label, e.g. "周二 · 工作日".
    /// Same value the proactive prompt's time line uses (Cβ).
    pub day_of_week: String,
    /// Iter D1: human-readable user-absence cue, e.g. "用户离开了一小会儿". Same value
    /// the proactive prompt's time line uses (Cμ). Surfaced so the panel can render
    /// the register the LLM is currently reading.
    pub idle_register: String,
    /// Iter D1: minutes since last user interaction. Pairs with `idle_register` —
    /// register is the human cue, this is the precise number for tooltip / debug.
    pub idle_minutes: u64,
    /// Iter D2: companionship milestone label when today is one (Cρ — 7 / 30 /
    /// 100 / 180 / 365 / yearly). None otherwise. Surfaced so the panel can show
    /// a celebration cue on the same days the proactive prompt's milestone rule
    /// fires.
    pub companionship_milestone: Option<String>,
    /// Iter D2: companionship days (lifetime count). Already in PanelStatsCard
    /// via a separate Tauri command, but bundling it here lets the strip render
    /// the milestone cue without a second IPC.
    pub companionship_days: u64,
    /// Iter D4: true when the current hour is inside the configured quiet
    /// window (settings.proactive.quiet_hours_start/end). Distinct from
    /// `pre_quiet_minutes`: pre_quiet fires within 15 min *before* the window
    /// starts (a winding-down register cue); this fires *during* the window
    /// when the gate is fully suppressing proactive turns. The panel uses both
    /// to render "approaching → in" transition for the user.
    pub in_quiet_hours: bool,
    /// Iter D9: seconds remaining on the cooldown gate (Iter 5). Some(N) when
    /// the gate is currently blocking — `N = cooldown_seconds - since_last`.
    /// None when the gate is open (cooldown expired or pet has never spoken).
    /// Surfaced so the panel can show "下次开口最多还要 Ns" instead of the
    /// silent gate making the pet feel unresponsive.
    pub cooldown_remaining_seconds: Option<u64>,
    /// Iter D10: true when the awaiting-user-reply gate (Iter 5) is set —
    /// pet spoke proactively last and user hasn't sent anything since. The
    /// gate keeps the pet from doubling up. Distinct from cooldown: cooldown
    /// is time-based, awaiting is state-based ("polite to wait until acked").
    /// Both gates can fire simultaneously; both visible separately.
    pub awaiting_user_reply: bool,
    /// Iter D12: false when the user has turned `settings.proactive.enabled`
    /// off — proactive engine silently no-ops and the pet appears mute
    /// regardless of any other signal. Surfaced so users who toggled
    /// proactive off and forgot get an immediate "🔕 proactive 已关" chip.
    pub proactive_enabled: bool,
    /// Iter R10: short-term feedback summary `{replied, total}` over the same
    /// 20-entry window the panel timeline (R6) and gate-side adaptation
    /// (R7) read. None when no feedback has been recorded yet (fresh
    /// install / first-day session). Surfaced so the tone strip can show a
    /// "💬 N/M" chip at-a-glance instead of users digging into the
    /// feedback timeline collapsible.
    pub feedback_summary: Option<FeedbackSummary>,
    /// Iter R20: speech length register classification over the same 5-line
    /// window the proactive prompt's R19 length-hint reads. None when too
    /// few samples; otherwise kind ∈ "long" / "short" / "mixed". Surfaced
    /// to the tone strip as a "📏 长 / 短 / 混" chip so the user can see
    /// which register the pet is currently stuck in (or not).
    pub speech_register: Option<crate::speech_history::SpeechRegisterSummary>,
    /// Iter R21: repeated-topic ngram if the pet has been circling the same
    /// theme — same detector R11 feeds to the proactive prompt, surfaced
    /// for panel visibility. Already redacted (R11 redacts in the prompt
    /// hint; R21 redacts here for the panel chip). None when no ngram
    /// recurs across enough distinct lines (the common "healthy" case).
    pub repeated_topic: Option<String>,
    /// Iter R23: cooldown breakdown showing how the effective cooldown is
    /// derived: configured × companion_mode × R7-feedback-band. Lets the
    /// panel hover render "1800s × 1.0 (balanced) × 2.0 (high_negative)
    /// = 3600s effective, 还剩 1234s" so the user understands why the
    /// gate is enforcing this number specifically. None when proactive
    /// is disabled or configured cooldown is 0 (gate effectively off).
    pub cooldown_breakdown: Option<CooldownBreakdown>,
    /// Iter R31: char count of the last proactive prompt — gives a
    /// budget chip for "how much context is the LLM seeing each turn?".
    /// None when no turn has fired yet (fresh process). Counts via
    /// `chars().count()` so multibyte CJK doesn't inflate the number
    /// 3× the way `len()` would.
    pub last_prompt_chars: Option<usize>,
    /// Iter R34: trailing silent streak — count of consecutive most-recent
    /// turns where outcome="silent". Surfaces R33's prompt-only signal as
    /// a panel chip so user can see "pet has been quiet 3 turns in a row"
    /// at a glance. Stable between turns (no flicker); resets on next
    /// spoke turn. 0 = no streak / pet just spoke.
    pub consecutive_silent_streak: usize,
    /// Iter R35: mirror on the feedback side — trailing-negative streak
    /// (Ignored | Dismissed in a row). Used by panel chip when ≥3 to
    /// flag "user has been rejecting recent turns" — prompt-side hint
    /// fires at same threshold (R35's `format_consecutive_negative_hint`).
    pub consecutive_negative_streak: usize,
    /// Iter R52: transient mute remaining seconds. None = not muted (or
    /// expired). Some(N) = N seconds left until pet resumes proactive
    /// turns. Distinct from `proactive_enabled` (persistent toggle);
    /// this is "be quiet for next session" state.
    pub mute_remaining_seconds: Option<i64>,
    /// Iter R55: transient instruction note text. None = no note (or
    /// expired). Some(text) = active note text. Distinct from mute —
    /// note adds context, doesn't block.
    pub transient_note: Option<String>,
    /// Iter R56: transient note remaining seconds — symmetric with
    /// `mute_remaining_seconds` (R52). Lets panel chip and button hover
    /// show countdown so user sees how long until note auto-expires.
    pub transient_note_remaining_seconds: Option<i64>,
    /// Iter R78: count of butler_tasks with `[deadline:]` prefix whose
    /// urgency is Imminent (<1h) or Overdue. Approaching (1-6h) and Distant
    /// don't contribute — chip is for "act now", not awareness. 0 when
    /// no deadline-prefixed tasks or all still distant.
    pub urgent_deadline_count: u64,
}

/// Iter R23: structured breakdown of effective cooldown derivation.
/// Frontend renders the math in the chip hover so the user sees how
/// `cooldown_remaining_seconds` ends up at its current value. Both
/// factors are exact (mode is 0.5/1.0/2.0; feedback is 0.7/1.0/2.0)
/// so f64 is safe — no precision drift over the small space.
///
/// Iter R81: extended with `deadline_factor` + `urgent_deadline_count` so a
/// pending Imminent/Overdue butler deadline halves the effective cooldown
/// (real-partner intuition: don't keep your normal quiet rhythm when something
/// urgent is bearing down on the user).
#[derive(serde::Serialize, Clone, Debug)]
pub struct CooldownBreakdown {
    /// Raw `settings.proactive.cooldown_seconds` (before any multipliers).
    pub configured_seconds: u64,
    /// "balanced" / "chatty" / "quiet" — current `companion_mode`.
    pub mode: String,
    /// 0.5× (chatty) / 1.0× (balanced) / 2.0× (quiet). Same multiplier
    /// `apply_companion_mode` uses internally.
    pub mode_factor: f64,
    /// `configured_seconds * mode_factor`, rounded down (matches what
    /// `effective_cooldown_base` returns).
    pub after_mode_seconds: u64,
    /// "high_negative" (ratio > 0.6) / "low_negative" (< 0.2) / "mid"
    /// (between thresholds) / "insufficient_samples" (< 5 entries — R7
    /// returns base unchanged in this case).
    pub feedback_band: String,
    /// 2.0× / 0.7× / 1.0× depending on band. `insufficient_samples` is 1.0×.
    pub feedback_factor: f64,
    /// Iter R81: count of Imminent (<1h) + Overdue butler deadlines. Drives
    /// `deadline_factor`. Surfaced separately so the panel hover can show
    /// "N urgent deadline(s)" alongside the multiplier.
    pub urgent_deadline_count: u64,
    /// Iter R81: 0.5× when `urgent_deadline_count ≥ 1`, else 1.0×. Pure
    /// switch — `deadline_urgency_factor` in butler_schedule.
    pub deadline_factor: f64,
    /// `after_mode_seconds * feedback_factor * deadline_factor`, rounded
    /// down. This is what the gate actually enforces —
    /// `cooldown_remaining_seconds` is computed against this, not against
    /// `configured_seconds`.
    pub effective_seconds: u64,
}

/// Iter R10: simple shape for the tone-strip feedback chip. R1c added
/// `dismissed` so the panel can distinguish *active* rejection (user
/// clicked the bubble within 5s) from *passive* ignore (no interaction).
/// `total` includes all three kinds; `replied + ignored + dismissed = total`
/// where `ignored = total - replied - dismissed`.
#[derive(serde::Serialize, Clone, Debug)]
pub struct FeedbackSummary {
    pub replied: u64,
    pub dismissed: u64,
    pub total: u64,
}

/// Compute the tone snapshot from inner deps. Iter QG6 extracted this from
/// `get_tone_snapshot` so the bundled `get_debug_snapshot` aggregator can reuse
/// the body without re-walking the Tauri State plumbing.
pub async fn build_tone_snapshot(
    clock: &InteractionClock,
    wake: &crate::wake_detector::WakeDetector,
    counters: &crate::commands::debug::ProcessCounters,
) -> Result<ToneSnapshot, String> {
    let now = chrono::Local::now();
    let hour = now.hour() as u8;
    let minute = now.minute() as u8;
    let snap = clock.snapshot().await;
    let cadence_min = snap.since_last_proactive_seconds.map(|s| s / 60);
    let cadence = cadence_min.map(|m| idle_tier(m).to_string());
    // Iter Cν: idle_minutes (since user last interacted) drives the
    // long-absence-reunion composite rule. Distinct from cadence_min above
    // (which tracks the pet's own last utterance).
    let idle_min_for_rules: u64 = snap.idle_seconds / 60;
    let wake_ago = wake.last_wake_seconds_ago().await;
    let (mood_text, mood_motion) = match crate::mood::read_current_mood_parsed() {
        Some((t, m)) => (Some(t), m),
        None => (None, None),
    };
    let pre_quiet_minutes = get_settings().ok().and_then(|s| {
        minutes_until_quiet_start(
            hour,
            minute,
            s.proactive.quiet_hours_start,
            s.proactive.quiet_hours_end,
            15,
        )
    });
    let proactive_count = crate::speech_history::lifetime_speech_count().await;
    let chatty_day_threshold = get_settings()
        .ok()
        .map(|s| s.proactive.effective_chatty_threshold())
        .unwrap_or(5);
    let today_count_for_rules = crate::speech_history::today_speech_count().await;
    let env_counters_for_rules = &counters.env_tool;
    let env_total = env_counters_for_rules
        .spoke_total
        .load(std::sync::atomic::Ordering::Relaxed);
    let env_with_any = env_counters_for_rules
        .spoke_with_any
        .load(std::sync::atomic::Ordering::Relaxed);
    // Environmental rules: derive from already-fetched ToneSnapshot ingredients +
    // memory-IO probes for due reminders / active plan. Cost is one yaml read per
    // category — same as a panel `get_pending_reminders` call, which the panel was
    // already polling at 1 Hz, so this doesn't add new IO pressure.
    let wake_back = matches!(wake_ago, Some(secs) if secs <= 600);
    let first_mood = mood_text
        .as_ref()
        .map(|t| t.trim().is_empty())
        .unwrap_or(true);
    let pre_quiet = pre_quiet_minutes.is_some();
    let reminders_due = !build_reminders_hint(now.naive_local()).is_empty();
    let has_plan = !build_plan_hint().is_empty();
    let env_labels = active_environmental_rule_labels(
        wake_back,
        first_mood,
        pre_quiet,
        reminders_due,
        has_plan,
        today_count_for_rules == 0,
    );
    let companionship_days_for_rules = crate::companionship::companionship_days().await;
    let data_labels = active_data_driven_rule_labels(
        proactive_count as usize,
        today_count_for_rules,
        chatty_day_threshold,
        env_total,
        env_with_any,
        companionship_days_for_rules,
    );
    let composite_labels = active_composite_rule_labels(
        wake_back,
        has_plan,
        cadence_min,
        today_count_for_rules,
        chatty_day_threshold,
        pre_quiet,
        idle_min_for_rules,
        hour,
        late_night_wellness_in_cooldown(),
    );
    let active_prompt_rules: Vec<String> = env_labels
        .iter()
        .chain(data_labels.iter())
        .chain(composite_labels.iter())
        .map(|s| String::from(*s))
        .collect();
    // Iter R20 / R21: shared 5-line fetch feeds both speech_register and
    // repeated_topic so the struct literal below has clean inline expressions.
    let recent_for_signals = crate::speech_history::recent_speeches(5).await;
    // Iter R10 / R23: shared feedback fetch — feedback_summary and
    // cooldown_breakdown both consume it. Single fetch + multiple derived
    // signals = same pattern as recent_for_signals above.
    let recent_feedback_for_signals = crate::feedback_history::recent_feedback(20).await;
    // Iter R78 / R81: shared urgent-deadline count. R78 surfaces it via the
    // ⏳ chip; R81 also folds it into cooldown_breakdown so the same value
    // drives chip + cooldown shrink (single source of truth).
    let urgent_deadline_count: u64 = {
        let now = chrono::Local::now().naive_local();
        let items: Vec<(chrono::NaiveDateTime, String)> = crate::db::butler_tasks_as_memory_items()
            .iter()
            .filter_map(|i| parse_butler_deadline_prefix(&i.description))
            .collect();
        count_urgent_butler_deadlines(&items, now)
    };
    let cooldown_breakdown =
        build_cooldown_breakdown(&recent_feedback_for_signals, urgent_deadline_count);
    Ok(ToneSnapshot {
        period: period_of_day(hour).to_string(),
        cadence,
        since_last_proactive_minutes: cadence_min,
        wake_seconds_ago: wake_ago,
        mood_text,
        mood_motion,
        pre_quiet_minutes,
        proactive_count,
        chatty_day_threshold,
        active_prompt_rules,
        day_of_week: format_day_of_week_hint(now.weekday()),
        idle_register: user_absence_tier(idle_min_for_rules).to_string(),
        idle_minutes: idle_min_for_rules,
        // Iter D2: surface the same milestone label that drives the
        // companionship-milestone prompt rule (Cρ) so the panel can flag the
        // day visually.
        companionship_milestone: companionship_milestone(companionship_days_for_rules)
            .map(|s| s.to_string()),
        companionship_days: companionship_days_for_rules,
        // Iter D4: same in_quiet_hours predicate the gate uses, so the panel
        // can flag "the pet is currently dormant".
        in_quiet_hours: get_settings()
            .ok()
            .map(|s| {
                in_quiet_hours(
                    hour,
                    s.proactive.quiet_hours_start,
                    s.proactive.quiet_hours_end,
                )
            })
            .unwrap_or(false),
        // Iter D9 / R23: cooldown remaining, computed against the EFFECTIVE
        // cooldown (configured × companion_mode × R7-feedback-band) so the
        // chip matches what the gate actually enforces — not the raw
        // settings value. R23 fixed an old D9 bug where chip was based on
        // `cooldown_seconds` while gate used `effective_cooldown`.
        cooldown_remaining_seconds: {
            let effective = cooldown_breakdown
                .as_ref()
                .map(|b| b.effective_seconds)
                .unwrap_or(0);
            match snap.since_last_proactive_seconds {
                Some(since) if effective > 0 && since < effective => Some(effective - since),
                _ => None,
            }
        },
        // Iter D10: pass through the awaiting-user-reply state from clock snapshot.
        awaiting_user_reply: snap.awaiting_user_reply,
        // Iter D12: surface settings.proactive.enabled so users who flipped it
        // off see why the pet has stopped speaking. Defaults to true if
        // settings can't be read so we don't falsely show "disabled" on errors.
        proactive_enabled: get_settings()
            .ok()
            .map(|s| s.proactive.enabled)
            .unwrap_or(true),
        // Iter R10: feedback summary (last 20 entries) for the tone-strip
        // chip. Same window the panel timeline (R6) and adapted-cooldown
        // gate (R7) read, so chip / timeline / gate share one denominator.
        feedback_summary: {
            if recent_feedback_for_signals.is_empty() {
                None
            } else {
                // 把 Liked 与 Replied 一起计入"正向反馈"——chip 的健康度核
                // 心是"被听到 / 被认可"，二者语义相同（一个隐式发消息回应，
                // 一个显式 👍）。tooltip 在 panel 侧分别展示两个计数。
                let replied = recent_feedback_for_signals
                    .iter()
                    .filter(|e| {
                        matches!(
                            e.kind,
                            crate::feedback_history::FeedbackKind::Replied
                                | crate::feedback_history::FeedbackKind::Liked
                        )
                    })
                    .count() as u64;
                let dismissed = recent_feedback_for_signals
                    .iter()
                    .filter(|e| matches!(e.kind, crate::feedback_history::FeedbackKind::Dismissed))
                    .count() as u64;
                Some(FeedbackSummary {
                    replied,
                    dismissed,
                    total: recent_feedback_for_signals.len() as u64,
                })
            }
        },
        // Iter R20 / R21: speech-length register classification + R11's
        // repeated-topic ngram detector — both consume the same 5-line
        // window. Single fetch shared between two derived signals; mirrors
        // run_proactive_turn's speech_hint / repeated_topic_hint /
        // length_register_hint triple-from-one-fetch pattern.
        speech_register: crate::speech_history::classify_speech_register(&recent_for_signals),
        repeated_topic: crate::speech_history::detect_repeated_topic(&recent_for_signals, 4, 3),
        cooldown_breakdown,
        // Iter R31: count chars of the last constructed prompt. chars().count()
        // not len() so 30 char CJK doesn't read as 90 byte budget.
        last_prompt_chars: LAST_PROACTIVE_PROMPT
            .lock()
            .ok()
            .and_then(|g| g.as_ref().map(|s| s.chars().count())),
        // Iter R34: read-only count of trailing-silent ring buffer. Same
        // pure helper R33 uses for prompt nudge — single source of truth
        // (chip threshold and prompt threshold can't drift since both
        // call count_trailing_silent on the same buffer).
        consecutive_silent_streak: LAST_PROACTIVE_TURNS
            .lock()
            .ok()
            .map(|g| {
                let snap: Vec<TurnRecord> = g.iter().cloned().collect();
                count_trailing_silent(&snap)
            })
            .unwrap_or(0),
        // Iter R35: mirror — trailing-negative streak from the same
        // recent_feedback_for_signals fetch. Same pure helper R35 uses
        // for the prompt hint (single source of truth).
        consecutive_negative_streak: crate::feedback_history::count_trailing_negative(
            &recent_feedback_for_signals,
        ),
        // Iter R52: transient mute remaining seconds. Same pure helper
        // gate uses (mute_remaining_seconds) so chip + gate can't drift.
        mute_remaining_seconds: mute_remaining_seconds(),
        // Iter R55: transient note. Same pure helper as prompt assembler,
        // chip + prompt + gate-bypass all read same source.
        transient_note: transient_note_active(),
        // Iter R56: transient note remaining seconds for chip/hover countdown.
        transient_note_remaining_seconds: transient_note_remaining_seconds(),
        // Iter R78: surface the urgent-deadline count via the ⏳ chip.
        // Iter R81: same value also flows into cooldown_breakdown above so
        // the chip and the cooldown shrink stay in lockstep.
        urgent_deadline_count,
    })
}

/// Tauri command thin wrapper. Body lives in `build_tone_snapshot` so the
/// debug-snapshot aggregator can reuse it. Iter QG6.
#[tauri::command]
pub async fn get_tone_snapshot(
    clock: tauri::State<'_, InteractionClockStore>,
    wake: tauri::State<'_, crate::wake_detector::WakeDetectorStore>,
    counters: tauri::State<'_, crate::commands::debug::ProcessCountersStore>,
) -> Result<ToneSnapshot, String> {
    build_tone_snapshot(clock.inner(), wake.inner(), counters.inner()).await
}
