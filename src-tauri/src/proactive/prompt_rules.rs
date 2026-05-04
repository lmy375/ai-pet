//! Rule-label producers and supporting consts (Iter QG5c1 extraction from
//! `proactive.rs`).
//!
//! These functions classify the *current state* into a list of rule labels.
//! Each label maps to a chunk of prompt text in `proactive_rules` (still in
//! `proactive.rs`) and to a panel-side description in
//! `panelTypes.ts::PROMPT_RULE_DESCRIPTIONS`. The 3-way alignment is pinned
//! by tests in `proactive.rs::prompt_tests`.
//!
//! Three categories — kept as separate fns so the panel can show "prompt:
//! N hints" with a per-category breakdown later if needed:
//! - environmental: present-state signals (wake-back / first-mood / pre-quiet
//!   / reminders / plan / first-of-day).
//! - data-driven: counter / history-derived (icebreaker / chatty /
//!   companionship-milestone / env-awareness).
//! - composite: multi-signal coincidences (engagement-window /
//!   long-idle-no-restraint / long-absence-reunion / late-night-wellness).
//!
//! The late-night-wellness rate-limit machinery (Iter R8) lives here too —
//! the static stamp + helpers are scoped to "this rule" so they belong with
//! the rule definition.
//!
//! Public surface preserved via glob `pub use` at the top of `proactive.rs`.

/// Minimum sample size before the env-awareness self-correction rule starts firing. Below
/// this we don't have enough signal to distinguish "user just got the pet talking" from
/// "the pet has been ignoring tools for a while". 10 turns is roughly half a day of
/// normal use given default cadences.
pub const ENV_AWARENESS_MIN_SAMPLES: u64 = 10;
/// Ratio threshold (numerator/100) below which the rule fires. 30 → fires when fewer
/// than 30% of recent Spoke turns consulted at least one env-aware tool. Keeps the prod
/// to the genuinely concerning floor; 50% (the panel chip's warning color) is too eager
/// for a prompt-side intervention.
pub const ENV_AWARENESS_LOW_RATE_PCT: u64 = 30;
/// Minutes-since-last-proactive threshold for the `long-idle-no-restraint` composite
/// rule. 60 = "you haven't spoken for an hour" — distinct from the existing cadence
/// tiers used in `cadence_hint`, which top out at "haven't talked in ages" without a
/// numeric anchor. None (= never spoken) is treated as long-idle so the rule helps
/// fresh sessions where 0 prior speech is the same problem as silent for an hour.
pub const LONG_IDLE_MINUTES: u64 = 60;
/// Iter Cν: idle_minutes (since *user* last interacted) threshold for the
/// `long-absence-reunion` composite rule. 240 = 4 hours — distinct from the
/// system-sleep-driven `wake-back` (which fires on a discrete sleep wake event).
/// Long absence covers cases where the laptop stayed on but the user was gone:
/// out for lunch / in a meeting / asleep on a desktop / etc.
pub const LONG_ABSENCE_MINUTES: u64 = 240;

/// Iter R83: idle_minutes threshold for the stronger `extreme-absence-reunion`
/// rule. 1440 = 24 hours — the user has been gone for ≥ a full day, well
/// beyond Cν's "out for the afternoon" window. Pet should switch register
/// from "刚回来呀" (Cν warmth) to "好久不见，还好吗" (gentle check-in concern).
/// Mutually exclusive with `long-absence-reunion` — when extreme fires, long
/// is suppressed so the LLM gets one clear signal, not two overlapping ones.
pub const EXTREME_ABSENCE_MINUTES: u64 = 1440;

/// Iter R3: the wee-night window in which the wellness nudge fires. Hours
/// 0..LATE_NIGHT_END_HOUR — i.e. midnight through 3:59 — covers the band
/// where staying on the computer is a wellness concern; 4am rolls into
/// "early-bird normal" so we stop nudging.
pub const LATE_NIGHT_END_HOUR: u8 = 4;
/// Idle threshold under which the user is treated as actively at the keyboard
/// for the late-night-wellness rule. < 5 minutes since last interaction means
/// they are clearly working / browsing / etc., not just left the laptop on.
pub const LATE_NIGHT_ACTIVE_MAX_IDLE_MIN: u64 = 5;
/// Iter R8: minimum gap between two `late-night-wellness` rule activations.
/// Without this, every loop tick during 0:00-3:59 with active user fires the
/// rule again — six "该睡了" pings per hour is harassment, not concern. 30
/// minutes is enough that a user who ignored once will still feel the second
/// nudge as fresh, not a repeat. Caller-checked via `recently_fired_wellness`
/// param so the label fn stays pure.
pub const LATE_NIGHT_WELLNESS_MIN_GAP_SECONDS: u64 = 1800;

/// Iter R8: stamp of the most recent late-night-wellness rule activation.
/// Used to suppress re-firing within `LATE_NIGHT_WELLNESS_MIN_GAP_SECONDS`.
/// Reset on process restart (forgivable; rate limit only matters during a
/// single overnight session anyway).
pub static LAST_LATE_NIGHT_WELLNESS_AT: std::sync::Mutex<Option<std::time::Instant>> =
    std::sync::Mutex::new(None);

/// Iter R8: pure decider — given the most recent stamp and a "now" Instant,
/// returns true if the gap is shorter than `min_gap_seconds`. Pure so tests
/// can drive it deterministically without touching the static.
pub fn late_night_wellness_recently_fired_at(
    last: Option<std::time::Instant>,
    now: std::time::Instant,
    min_gap_seconds: u64,
) -> bool {
    match last {
        None => false,
        Some(t) => now.saturating_duration_since(t).as_secs() < min_gap_seconds,
    }
}

/// Iter R8: read the static stamp + apply the gap rule. Caller-side
/// convenience over `late_night_wellness_recently_fired_at` so production
/// code paths don't need to touch `Instant::now()` and the static directly.
pub fn late_night_wellness_in_cooldown() -> bool {
    let last = LAST_LATE_NIGHT_WELLNESS_AT.lock().ok().and_then(|g| *g);
    late_night_wellness_recently_fired_at(
        last,
        std::time::Instant::now(),
        LATE_NIGHT_WELLNESS_MIN_GAP_SECONDS,
    )
}

/// Iter R8: stamp the static so subsequent `late_night_wellness_in_cooldown`
/// returns true within the gap window. Called on rule activation (dispatch
/// time) — even if the LLM ultimately stays silent, we treat the
/// activation as "user has been notified for this window" so we don't
/// thrash on near-edge cases.
pub fn mark_late_night_wellness_fired() {
    if let Ok(mut g) = LAST_LATE_NIGHT_WELLNESS_AT.lock() {
        *g = Some(std::time::Instant::now());
    }
}

/// Pure check: does the env-awareness ratio sit below the corrective threshold? Returns
/// false until at least `ENV_AWARENESS_MIN_SAMPLES` Spoke turns are recorded so we don't
/// fire on noise. Extracted for testability — the rule body uses it once.
pub fn env_awareness_low(spoke_total: u64, spoke_with_any: u64) -> bool {
    if spoke_total < ENV_AWARENESS_MIN_SAMPLES {
        return false;
    }
    // Compare spoke_with_any * 100 < ENV_AWARENESS_LOW_RATE_PCT * spoke_total instead of
    // floating-point division — exact integer arithmetic, no rounding edge cases at
    // exactly 30%.
    spoke_with_any * 100 < ENV_AWARENESS_LOW_RATE_PCT * spoke_total
}

/// Iter Cρ: pure helper — return a Chinese milestone label if `days` is a relationship
/// milestone, else None. Called by both the rule body (which formats the label into
/// the prompt) and the data-driven label helper (which pushes the rule label when
/// non-None). Milestones: 7, 30, 100, 180, 365 fixed; every 365 thereafter is
/// "又一个周年". Returns None on day 0 (already covered by the always-rendered
/// companionship_line's "第一天" framing).
pub fn companionship_milestone(days: u64) -> Option<&'static str> {
    match days {
        7 => Some("刚好一周"),
        30 => Some("满一个月"),
        100 => Some("百日纪念"),
        180 => Some("满半年"),
        365 => Some("满一年"),
        d if d > 365 && d % 365 == 0 => Some("又一个周年"),
        _ => None,
    }
}

/// Returns the labels for every *data-driven* contextual rule currently firing in the
/// proactive prompt. "Data-driven" means rules whose firing depends on counters/history
/// (icebreaker / chatty / env-awareness) — distinct from `active_environmental_rule_labels`
/// which covers state-driven rules like wake-back / first-mood / due-reminders.
///
/// Order matches the firing order in `proactive_rules` so a future "show in firing
/// sequence" tooltip stays correct.
pub fn active_data_driven_rule_labels(
    proactive_history_count: usize,
    today_speech_count: u64,
    chatty_day_threshold: u64,
    env_spoke_total: u64,
    env_spoke_with_any: u64,
    companionship_days: u64,
) -> Vec<&'static str> {
    let mut labels: Vec<&'static str> = Vec::with_capacity(4);
    if proactive_history_count < 3 {
        labels.push("icebreaker");
    }
    if chatty_day_threshold > 0 && today_speech_count >= chatty_day_threshold {
        labels.push("chatty");
    }
    if companionship_milestone(companionship_days).is_some() {
        labels.push("companionship-milestone");
    }
    if env_awareness_low(env_spoke_total, env_spoke_with_any) {
        labels.push("env-awareness");
    }
    labels
}

/// Returns the labels for every *environmental* contextual rule currently firing —
/// rules whose firing depends on present-state signals like a recent wake-from-sleep,
/// missing mood file, approaching quiet hours, due reminders, or an in-flight daily
/// plan. Pairs with `active_data_driven_rule_labels`; both feed the panel "prompt:
/// N hints" badge and the decision-log `rules=...` tag.
///
/// Order matches the firing order in `proactive_rules`.
pub fn active_environmental_rule_labels(
    wake_back: bool,
    first_mood: bool,
    pre_quiet: bool,
    reminders_due: bool,
    has_plan: bool,
    first_of_day: bool,
) -> Vec<&'static str> {
    let mut labels: Vec<&'static str> = Vec::with_capacity(6);
    if wake_back {
        labels.push("wake-back");
    }
    if first_mood {
        labels.push("first-mood");
    }
    if first_of_day {
        labels.push("first-of-day");
    }
    if pre_quiet {
        labels.push("pre-quiet");
    }
    if reminders_due {
        labels.push("reminders");
    }
    if has_plan {
        labels.push("plan");
    }
    labels
}

/// Returns labels for *composite* rules — those that fire only when multiple individual
/// signals coincide. Most existing rules are restraints ("be quiet because X"); the
/// composite group makes room for *positive* prompts ("right now is a good moment to
/// open up because X+Y") that the singletons can't express. Members:
///
/// - `engagement-window`: user just came back to the desk AND the pet has an in-flight
///   daily plan — a natural moment to weave concern + plan progress into one line.
/// - `long-idle-no-restraint`: it's been ≥ `LONG_IDLE_MINUTES` since the last proactive
///   AND the pet hasn't been chatty today AND we're not approaching quiet hours — a
///   safe window to surface a fresh topic instead of letting the silence drag on.
/// - `long-absence-reunion` (Iter Cν): user idle ≥ `LONG_ABSENCE_MINUTES` (4h)
///   AND under_chatty AND !pre_quiet — user is back after a half-day-ish gap.
///   Mutually exclusive with `extreme-absence-reunion`.
/// - `extreme-absence-reunion` (Iter R83): user idle ≥ `EXTREME_ABSENCE_MINUTES`
///   (24h) — user has been gone for a full day or more. Replaces (suppresses)
///   `long-absence-reunion` so the LLM gets one clear "shift register to
///   gentle check-in concern" signal, not two overlapping ones.
/// - `late-night-wellness` (Iter R3): hour < `LATE_NIGHT_END_HOUR` AND idle <
///   `LATE_NIGHT_ACTIVE_MAX_IDLE_MIN` — user is still at the keyboard past
///   midnight. Pet should nudge them to wrap up regardless of what the LLM
///   would otherwise think to say. Hard wellness rule, not soft chatty bias.
#[allow(clippy::too_many_arguments)] // each signal is independently sourced; bundling adds plumbing
pub fn active_composite_rule_labels(
    wake_back: bool,
    has_plan: bool,
    since_last_proactive_minutes: Option<u64>,
    today_speech_count: u64,
    chatty_day_threshold: u64,
    pre_quiet: bool,
    idle_minutes: u64,
    hour: u8,
    recently_fired_wellness: bool,
) -> Vec<&'static str> {
    let mut labels: Vec<&'static str> = Vec::with_capacity(4);
    if wake_back && has_plan {
        labels.push("engagement-window");
    }
    let long_idle = match since_last_proactive_minutes {
        Some(m) => m >= LONG_IDLE_MINUTES,
        // Never-spoken is treated as long-idle: no recent speech to defer to.
        None => true,
    };
    let under_chatty = chatty_day_threshold == 0 || today_speech_count < chatty_day_threshold;
    if long_idle && under_chatty && !pre_quiet {
        labels.push("long-idle-no-restraint");
    }
    // Iter Cν: long-absence-reunion fires when the user themselves has been away
    // ≥ LONG_ABSENCE_MINUTES, regardless of pet-side cadence. Gates on
    // under_chatty (don't pile on if today's already chatty) and !pre_quiet
    // (don't add an opener register right before quiet hours kick in).
    //
    // Iter R83: when absence crosses EXTREME_ABSENCE_MINUTES (24h), escalate
    // to `extreme-absence-reunion` and *suppress* the long-absence label —
    // the LLM gets one signal, not two overlapping reunion cues. Real
    // partner shifts from "刚回来呀" warmth to "好久不见，还好吗" check-in
    // concern around the full-day mark; this rule encodes that shift.
    if idle_minutes >= LONG_ABSENCE_MINUTES && under_chatty && !pre_quiet {
        let label = if idle_minutes >= EXTREME_ABSENCE_MINUTES {
            "extreme-absence-reunion"
        } else {
            "long-absence-reunion"
        };
        labels.push(label);
    }
    // Iter R3: late-night wellness — fires regardless of chatty / pre_quiet so
    // the pet always speaks up if user is still at the keyboard past midnight.
    // The whole point is overriding normal cadence to prioritize health.
    // Iter R8: rate-limited via `recently_fired_wellness` so a user staying up
    // 1am-4am isn't pinged 6+ times. 30-minute gap (caller-computed).
    if hour < LATE_NIGHT_END_HOUR
        && idle_minutes < LATE_NIGHT_ACTIVE_MAX_IDLE_MIN
        && !recently_fired_wellness
    {
        labels.push("late-night-wellness");
    }
    labels
}
