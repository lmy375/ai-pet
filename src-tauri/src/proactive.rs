//! Proactive engagement engine.
//!
//! Spawns a background loop that wakes up periodically and decides whether the pet should
//! initiate a conversation with the user. Currently uses a single signal — time since the last
//! interaction — and asks the LLM whether to speak. Future iterations will add active-app
//! detection, idle-input detection, and mood state.
//!
//! Wire-up: see `lib.rs`. The engine is started once in `setup`. It writes proactive replies
//! into the active session and emits a `proactive-message` Tauri event the frontend listens for.

use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{Datelike, Timelike};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Mutex as TokioMutex;

// Iter QG5 incremental: reminders subsystem extracted to a submodule. The
// glob `pub use` re-exports the public API so external callers
// (`consolidate.rs`, panel commands) keep reaching items via the historical
// `crate::proactive::ReminderTarget` / `parse_reminder_prefix` paths.
mod active_app;
mod butler_schedule;
mod daily_review;
mod gate;
mod morning_briefing;
mod prompt_assembler;
mod prompt_rules;
mod reminders;
mod telemetry;
mod time_helpers;
pub use self::active_app::*;
pub use self::butler_schedule::*;
pub use self::daily_review::*;
pub use self::gate::*;
pub use self::morning_briefing::*;
pub use self::prompt_assembler::*;
pub use self::prompt_rules::*;
pub use self::reminders::*;
pub use self::telemetry::*;
pub use self::time_helpers::*;

use crate::commands::chat::{run_chat_pipeline, ChatMessage, CollectingSink};
use crate::commands::debug::{write_log, LogStore};
use crate::commands::session;
use crate::commands::settings::{get_settings, get_soul};
use crate::commands::shell::ShellStore;
use crate::config::AiConfig;
use crate::mcp::McpManagerStore;
use crate::mood::{read_current_mood_parsed, read_mood_for_event};
use crate::tools::ToolContext;

/// Tracks interaction timing for the proactive engagement loop. Holds the last interaction
/// time, the last proactive utterance time, and whether the most recent proactive message
/// is still waiting for a user reply.
///
/// State transitions:
/// - `mark_user_message()` — user sent something. Clears `awaiting_user_reply`.
/// - `mark_proactive_spoken()` — pet spoke proactively. Sets `awaiting_user_reply = true`
///   and stamps `last_proactive`.
/// - `touch()` — any other interaction (e.g. assistant finished a reactive reply). Only
///   updates `last`; does not affect `awaiting_user_reply` or `last_proactive`.
pub struct InteractionClock {
    inner: TokioMutex<ClockInner>,
}

struct ClockInner {
    last: Instant,
    last_proactive: Option<Instant>,
    awaiting_user_reply: bool,
}

/// Snapshot of clock state used by the proactive scheduler to decide whether to fire.
pub struct ClockSnapshot {
    pub idle_seconds: u64,
    pub since_last_proactive_seconds: Option<u64>,
    pub awaiting_user_reply: bool,
}

impl InteractionClock {
    pub fn new() -> Self {
        Self {
            inner: TokioMutex::new(ClockInner {
                last: Instant::now(),
                last_proactive: None,
                awaiting_user_reply: false,
            }),
        }
    }

    pub async fn touch(&self) {
        let mut g = self.inner.lock().await;
        g.last = Instant::now();
    }

    /// Called when the user sends a message. Clears the awaiting-reply flag — once the user
    /// has spoken, we no longer consider any prior proactive message "ignored".
    pub async fn mark_user_message(&self) {
        let mut g = self.inner.lock().await;
        g.last = Instant::now();
        g.awaiting_user_reply = false;
    }

    /// Called after a proactive utterance is delivered. Sets `awaiting_user_reply = true`
    /// and records the time so cooldown checks can run.
    pub async fn mark_proactive_spoken(&self) {
        let now = Instant::now();
        let mut g = self.inner.lock().await;
        g.last = now;
        g.last_proactive = Some(now);
        g.awaiting_user_reply = true;
    }

    /// Iter R1: return the un-expired `awaiting_user_reply` flag without applying
    /// the D11 4-hour auto-clear. Used by feedback classification — for that
    /// purpose "user actually replied" is binary regardless of how long they
    /// took, while the *gate* check uses `effective_awaiting` so the pet doesn't
    /// stay muted across multi-day absences.
    pub async fn raw_awaiting(&self) -> bool {
        self.inner.lock().await.awaiting_user_reply
    }

    pub async fn snapshot(&self) -> ClockSnapshot {
        let g = self.inner.lock().await;
        let since_proactive = g.last_proactive.map(|t| t.elapsed().as_secs());
        ClockSnapshot {
            idle_seconds: g.last.elapsed().as_secs(),
            since_last_proactive_seconds: since_proactive,
            // Iter D11: auto-expire awaiting after AWAITING_AUTO_CLEAR_SECONDS.
            // The raw state in ClockInner stays — only `mark_user_message` is the
            // canonical clearer — but snapshot reports the effective gate state
            // so both proactive's gate check and the panel chip honor the same
            // expiry. Without this, a pet that spoke once and the user closed
            // the laptop without replying would stay muted indefinitely on the
            // next session, even hours later.
            awaiting_user_reply: effective_awaiting(g.awaiting_user_reply, since_proactive),
        }
    }
}

/// Iter D11: pure decider — given raw awaiting state and seconds since pet's last
/// proactive utterance, return whether the awaiting gate should still fire.
/// Returns true only when the raw flag is set AND the gap is short enough that
/// the original "polite-wait, don't double up" intent still applies. After
/// AWAITING_AUTO_CLEAR_SECONDS the gate falls off so the pet doesn't stay
/// permanently muted across long absences.
pub fn effective_awaiting(raw_awaiting: bool, since_last_proactive_seconds: Option<u64>) -> bool {
    if !raw_awaiting {
        return false;
    }
    match since_last_proactive_seconds {
        Some(secs) => secs < AWAITING_AUTO_CLEAR_SECONDS,
        // No prior proactive recorded but flag is set — treat as not-fresh.
        // Shouldn't really happen since mark_proactive_spoken sets both, but
        // belt-and-suspenders.
        None => false,
    }
}

/// Iter D11: how long the awaiting gate honors the raw flag before auto-clearing.
/// 4 hours: covers a typical "stepped away for lunch + meeting" without forcing
/// the user to chat back; long enough that re-firing the gate later isn't rude.
pub const AWAITING_AUTO_CLEAR_SECONDS: u64 = 4 * 3600;

// Iter QG5e: in-memory stashes (LAST_PROACTIVE_*, LAST_FEEDBACK_RECORDED_FOR,
// LAST_PROACTIVE_TURNS) + TurnRecord + ProactiveTurnMeta + Tauri commands
// (get_last_proactive_prompt / _reply / _meta / get_recent_proactive_turns)
// extracted to `proactive/telemetry.rs`.

pub type InteractionClockStore = Arc<InteractionClock>;

pub fn new_interaction_clock() -> InteractionClockStore {
    Arc::new(InteractionClock::new())
}

#[derive(Clone, Serialize)]
pub struct ProactiveMessage {
    pub text: String,
    pub timestamp: String,
    /// Snapshot of `ai_insights/current_mood` text (motion prefix stripped) after the LLM
    /// ran. None if the LLM hasn't written one yet.
    pub mood: Option<String>,
    /// Live2D motion group the LLM picked via the `[motion: X]` prefix in its mood update.
    /// Frontend prefers this over keyword matching when present.
    pub motion: Option<String>,
}

// Iter QG5c2: SILENT_MARKER moved to `proactive/prompt_assembler.rs` as
// `pub const`. Re-exported via the glob above so `run_proactive_turn` (which
// checks `reply.contains(SILENT_MARKER)`) keeps the bare-name reference.

// Iter QG5d: LoopAction enum moved to `proactive/gate.rs` (re-exported via
// glob above). Spawn-loop body below consumes it via the bare-name path.

// Iter QG5c1: ENV_AWARENESS_*, LONG_IDLE_MINUTES, LONG_ABSENCE_MINUTES
// extracted to `proactive/prompt_rules.rs` (re-exported via glob above).

/// Render the "companionship duration" section of the proactive prompt. Day 0 gets a
/// "this is the first day" framing — encouraging the LLM to use a getting-acquainted
/// register — while N >= 1 just states the count, letting the LLM choose whether and
/// how to draw on the accumulated familiarity. Pure / testable.
pub fn format_companionship_line(days: u64) -> String {
    if days == 0 {
        "你和用户今天才正式认识，是你陪伴 ta 的第一天——语气可以保留一点点初识的客气感。".to_string()
    } else {
        format!(
            "你和用户已经一起走过 {} 天——可以让这份相处时长自然渗进语气，比如对 ta 偏好的预判、共同回忆的暗指（不必硬塞，时机对就用）。",
            days
        )
    }
}

// Iter QG5c1: companionship_milestone moved to `proactive/prompt_rules.rs`
// (it's the rule-label producer for the `companionship-milestone` data-driven
// rule). format_companionship_line above stays — that's the prompt-line
// renderer, not a rule label.

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
    /// Iter D3: macOS Focus mode label when active, None otherwise. Same signal
    /// the proactive engine reads via `focus_mode::focus_status` to decide
    /// whether to gate. Surfaced so the panel can show 🎯 「work」 chip and
    /// the user can immediately see why the pet may be especially quiet.
    pub focus_mode: Option<String>,
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
    /// Iter R22: active-app snapshot (R15's data, panel-visible). Read-only
    /// inspection of the LAST_ACTIVE_APP static — panel polling does NOT
    /// reset the "since" clock. None on fresh process / non-macOS / when
    /// no proactive turn has yet observed the foreground app.
    pub active_app: Option<crate::proactive::active_app::ActiveAppSummary>,
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
    /// Iter R64: effective hard-block threshold (minutes) after applying
    /// `companion_mode` to `HARD_FOCUS_BLOCK_MINUTES`. balanced=90,
    /// chatty=135, quiet=60. Surfaced so the panel chip can color-band
    /// the active-app duration the same way the gate does — keeps
    /// chip color and gate behavior aligned for non-balanced users.
    pub effective_hard_block_minutes: u64,
    /// Iter R65: today's deep-focus stretch summary — finalized stretches
    /// only (in-progress not counted). None when nothing finalized today
    /// yet (or yesterday's data already filtered out by date check).
    /// Surfaced so PanelStatsCard can show "今日深度专注 N 次, X 分钟"
    /// as a self-report stat distinct from the speech-count column.
    pub daily_block_stats: Option<crate::proactive::active_app::DailyBlockStats>,
    /// Iter R76: panel-side flag for "today's peak is a personal record"
    /// (R74 strict-> semantic). True when today's max_single_stretch
    /// strictly exceeds the prior 7-day best. Surfaced so PanelStatsCard
    /// can render a ⭐ icon for at-a-glance celebration without re-running
    /// the comparison logic in TS.
    pub is_personal_record_today: bool,
    /// Iter R78: count of butler_tasks with `[deadline:]` prefix whose
    /// urgency is Imminent (<1h) or Overdue. Approaching (1-6h) and Distant
    /// don't contribute — chip is for "act now", not awareness. 0 when
    /// no deadline-prefixed tasks or all still distant.
    pub urgent_deadline_count: u64,
    /// Iter R68: weekly deep-focus summary — aggregated across last 7
    /// calendar days from DAILY_BLOCK_HISTORY. None when no entries in
    /// the window (fresh install / 7+ days quiet). Surfaced so the user
    /// sees "本周专注 N 次/Xm/Y 天" trend distinct from today's row.
    pub weekly_block_stats: Option<crate::proactive::active_app::WeeklyBlockSummary>,
    /// Iter R69: week-over-week trend for deep-focus minutes — direction
    /// (up / flat / down) + signed % delta vs prior week. None until
    /// both this week and prior week have data (8+ days of history).
    /// Surfaced as inline ↑/=/↓ icon on the panel weekly column.
    pub week_trend: Option<crate::proactive::active_app::WeekOverWeekTrend>,
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

#[derive(serde::Serialize)]
pub struct PendingReminder {
    pub time: String,
    pub topic: String,
    pub title: String,
    pub due_now: bool,
}

/// List every parseable reminder currently in the `todo` memory category, regardless of
/// whether it's due. Lets the panel show both "set for later" entries (helpful to verify
/// the chat actually wrote them) and "due now" entries (helpful to confirm the
/// proactive loop will surface them next tick).
#[tauri::command]
pub fn get_pending_reminders() -> Vec<PendingReminder> {
    let now = chrono::Local::now().naive_local();
    let Ok(index) = crate::commands::memory::memory_list(Some("todo".to_string())) else {
        return vec![];
    };
    let Some(cat) = index.categories.get("todo") else {
        return vec![];
    };
    let mut out = Vec::new();
    for item in &cat.items {
        if let Some((target, topic)) = parse_reminder_prefix(&item.description) {
            out.push(PendingReminder {
                time: format_target(&target),
                topic,
                title: item.title.clone(),
                due_now: is_reminder_due(&target, now, 30),
            });
        }
    }
    out
}

/// Force a proactive turn right now, bypassing the gates (awaiting / cooldown / idle /
/// quiet hours / focus / input-idle). Real values are still passed through into the
/// prompt so the LLM sees the actual idle stats. Used by panel "fire now" / demo flows
/// and for prompt iteration without waiting for natural conditions.
///
/// Iter QG3: routes the same outcome through `record_proactive_outcome` so manual
/// triggers update llm_outcome counters, env_tool stats and the decision-log just
/// like the loop. `source="manual"` keeps the panel able to tell them apart.
#[tauri::command]
pub async fn trigger_proactive_turn(app: tauri::AppHandle) -> Result<String, String> {
    let clock = app.state::<InteractionClockStore>().inner().clone();
    let snap = clock.snapshot().await;
    let input_idle = crate::input_idle::user_input_idle_seconds().await;
    let started = std::time::Instant::now();
    let result = run_proactive_turn(&app, snap.idle_seconds, input_idle).await;

    // Sample chatty_tag fresh so the manual trigger gets the same annotation
    // shape as the loop. rules_tag is None for manual: gates were bypassed so
    // there's no "this rule fired" set to record (see helper doc).
    let chatty_today = crate::speech_history::today_speech_count().await;
    let chatty_threshold = get_settings()
        .ok()
        .map(|s| s.proactive.effective_chatty_threshold())
        .unwrap_or(5);
    let chatty_part =
        chatty_mode_tag(chatty_today, chatty_threshold).unwrap_or_else(|| "-".to_string());
    let counters = app
        .state::<crate::commands::debug::ProcessCountersStore>()
        .inner()
        .clone();
    let decisions = app
        .state::<crate::decision_log::DecisionLogStore>()
        .inner()
        .clone();
    record_proactive_outcome(&counters, &decisions, "manual", &chatty_part, None, &result);

    let outcome = result?;
    let elapsed_ms = started.elapsed().as_millis();
    Ok(match outcome.reply {
        Some(text) => format!(
            "开口完成 ({} ms, idle={}s): {}",
            elapsed_ms, snap.idle_seconds, text
        ),
        None => format!(
            "宠物选择沉默 ({} ms, idle={}s)",
            elapsed_ms, snap.idle_seconds
        ),
    })
}

/// Iter R23: derive the cooldown breakdown for the panel chip hover.
/// Mirrors `gate.rs`'s effective-cooldown computation exactly so the
/// chip's "configured × mode × feedback × deadline = effective" math
/// matches the number the gate is actually enforcing. Returns `None`
/// when proactive is disabled or configured cooldown is 0 (gate
/// effectively off in either case — no breakdown to show).
///
/// Iter R81: `urgent_deadline_count` (Imminent + Overdue butler tasks)
/// drives a discrete 0.5× shrink on top of the R7 feedback factor.
pub fn build_cooldown_breakdown(
    recent_fb: &[crate::feedback_history::FeedbackEntry],
    urgent_deadline_count: u64,
) -> Option<CooldownBreakdown> {
    let settings = get_settings().ok()?;
    if !settings.proactive.enabled {
        return None;
    }
    let configured = settings.proactive.cooldown_seconds;
    if configured == 0 {
        return None;
    }
    let mode = settings.proactive.companion_mode.clone();
    let after_mode = settings.proactive.effective_cooldown_base();
    // mode_factor: derive from the ratio so a future mode addition
    // (e.g. "ultra-quiet") shows up correctly without needing a hardcoded
    // table here.
    let mode_factor = if configured == 0 {
        1.0
    } else {
        after_mode as f64 / configured as f64
    };
    // Match feedback_history::adapted_cooldown_seconds branching exactly.
    // Pure helper in feedback_history isolates the band classification so
    // it can be unit-tested without get_settings() / Tauri state.
    let (feedback_band, feedback_factor) =
        crate::feedback_history::classify_feedback_band(recent_fb);
    let deadline_factor = deadline_urgency_factor(urgent_deadline_count);
    let effective = ((after_mode as f64) * feedback_factor * deadline_factor) as u64;
    Some(CooldownBreakdown {
        configured_seconds: configured,
        mode,
        mode_factor,
        after_mode_seconds: after_mode,
        feedback_band: feedback_band.to_string(),
        feedback_factor,
        urgent_deadline_count,
        deadline_factor,
        effective_seconds: effective,
    })
}

#[cfg(test)]
mod cooldown_breakdown_tests {
    use crate::feedback_history::{classify_feedback_band, FeedbackEntry, FeedbackKind};

    fn entry(kind: FeedbackKind) -> FeedbackEntry {
        FeedbackEntry {
            timestamp: "2026-05-04T12:00:00+08:00".to_string(),
            kind,
            excerpt: "x".to_string(),
        }
    }

    #[test]
    fn band_insufficient_below_min_samples() {
        // R23: < 5 samples → "insufficient_samples", 1.0× (R7 returns base unchanged).
        let (band, factor) = classify_feedback_band(&[]);
        assert_eq!(band, "insufficient_samples");
        assert_eq!(factor, 1.0);
        let entries: Vec<_> = (0..4).map(|_| entry(FeedbackKind::Ignored)).collect();
        let (band, factor) = classify_feedback_band(&entries);
        assert_eq!(band, "insufficient_samples");
        assert_eq!(factor, 1.0);
    }

    #[test]
    fn band_high_negative_doubles() {
        // > 0.6 ratio: 4/5 ignored = 0.8 → high_negative, 2.0×.
        let mut entries = vec![entry(FeedbackKind::Ignored); 4];
        entries.push(entry(FeedbackKind::Replied));
        let (band, factor) = classify_feedback_band(&entries);
        assert_eq!(band, "high_negative");
        assert_eq!(factor, 2.0);
    }

    #[test]
    fn band_low_negative_shrinks() {
        // < 0.2 ratio: 1/10 ignored = 0.1 → low_negative, 0.7×.
        let mut entries = vec![entry(FeedbackKind::Ignored)];
        entries.extend(vec![entry(FeedbackKind::Replied); 9]);
        let (band, factor) = classify_feedback_band(&entries);
        assert_eq!(band, "low_negative");
        assert_eq!(factor, 0.7);
    }

    #[test]
    fn band_mid_keeps_base() {
        // 0.2 ≤ ratio ≤ 0.6 → "mid", 1.0× (cooldown unchanged).
        let mut entries = vec![entry(FeedbackKind::Ignored); 2]; // 2/5 = 0.4
        entries.extend(vec![entry(FeedbackKind::Replied); 3]);
        let (band, factor) = classify_feedback_band(&entries);
        assert_eq!(band, "mid");
        assert_eq!(factor, 1.0);
    }

    #[test]
    fn band_dismissed_counted_alongside_ignored() {
        // R1c: dismissed counts as negative. 3 dismissed + 2 replied = 0.6 ratio,
        // 0.6 is NOT > 0.6 (strict inequality) → mid band.
        let mut entries = vec![entry(FeedbackKind::Dismissed); 3];
        entries.extend(vec![entry(FeedbackKind::Replied); 2]);
        let (band, factor) = classify_feedback_band(&entries);
        assert_eq!(band, "mid");
        assert_eq!(factor, 1.0);
        // 4 dismissed + 1 replied = 0.8 → high_negative.
        let mut entries = vec![entry(FeedbackKind::Dismissed); 4];
        entries.push(entry(FeedbackKind::Replied));
        let (band, factor) = classify_feedback_band(&entries);
        assert_eq!(band, "high_negative");
        assert_eq!(factor, 2.0);
    }
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
        let items: Vec<(chrono::NaiveDateTime, String)> =
            crate::commands::memory::memory_list(Some("butler_tasks".to_string()))
                .ok()
                .and_then(|idx| idx.categories.get("butler_tasks").cloned())
                .map(|cat| {
                    cat.items
                        .iter()
                        .filter_map(|i| parse_butler_deadline_prefix(&i.description))
                        .collect()
                })
                .unwrap_or_default();
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
        // Iter D3: macOS Focus state — same source as the gate path uses.
        // Returns None on non-macOS or when no Focus is active.
        focus_mode: match crate::focus_mode::focus_status().await {
            Some(s) if s.active => s.name.or_else(|| Some("active".to_string())),
            _ => None,
        },
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
        repeated_topic: crate::speech_history::detect_repeated_topic(&recent_for_signals, 4, 3)
            .map(|t| crate::redaction::redact_with_settings(&t)),
        // Iter R22: read-only inspect of LAST_ACTIVE_APP — does NOT update
        // the static's `since` clock the way run_proactive_turn does, so
        // panel polling is safe.
        active_app: snapshot_active_app(),
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
        // Iter R64: effective hard-block threshold (minutes) after companion_mode
        // applies. Same value gate uses, so chip color band stays aligned with
        // gate behavior even when user picks chatty (135) or quiet (60). Reads
        // settings here (separate from `build_cooldown_breakdown`'s read) — fine,
        // settings reads are cheap, and keeps this field's derivation co-located
        // with its definition.
        effective_hard_block_minutes: get_settings()
            .ok()
            .map(|s| {
                s.proactive
                    .effective_hard_block_minutes(HARD_FOCUS_BLOCK_MINUTES)
            })
            .unwrap_or(HARD_FOCUS_BLOCK_MINUTES),
        // Iter R65: today's deep-focus stretch summary. Same accessor the
        // gate / take_recovery_hint write through — single source of
        // truth. None on fresh process / before any stretch finalizes
        // today (or yesterday's record filtered out by date check).
        daily_block_stats: crate::proactive::active_app::current_daily_block_stats(),
        // Iter R76: panel-side record flag. Reuses the R74 wrapper's
        // signal — non-empty hint string == record fired. Avoids a
        // second history walk; same source as R74 / R75 so panel,
        // proactive prompt, and chat layer all agree on what's a record.
        is_personal_record_today: !crate::proactive::active_app::current_personal_record_hint()
            .is_empty(),
        // Iter R78: surface the urgent-deadline count via the ⏳ chip.
        // Iter R81: same value also flows into cooldown_breakdown above so
        // the chip and the cooldown shrink stay in lockstep.
        urgent_deadline_count,
        // Iter R68: weekly deep-focus summary — aggregated across last 7
        // calendar days. Same DAILY_BLOCK_HISTORY source as daily_block_stats.
        weekly_block_stats: crate::proactive::active_app::current_weekly_block_summary(),
        // Iter R69: week-over-week trend (this week vs prior week). None
        // until both windows have data; needs 8+ days of history.
        week_trend: crate::proactive::active_app::current_week_over_week_trend(),
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

/// Iter R52: set transient mute for `minutes` from now. Used when user
/// wants pet quiet during a focused session without flipping the
/// permanent `proactive.enabled` setting. Pass 0 to clear. Returns the
/// resulting `MUTE_UNTIL` ISO timestamp (or empty when cleared) so the
/// frontend can show a confirmation chip / countdown.
#[tauri::command]
pub fn set_mute_minutes(minutes: i64) -> String {
    // R59: pure helper extracted; Tauri command is now thin wrapper that
    // computes new state + writes to mutex + formats response.
    let new_until = compute_new_mute_until(minutes, chrono::Local::now());
    if let Ok(mut g) = MUTE_UNTIL.lock() {
        *g = new_until;
    }
    new_until
        .map(|t| t.format("%Y-%m-%dT%H:%M:%S%:z").to_string())
        .unwrap_or_default()
}

/// Iter R52: read the current MUTE_UNTIL state. Returns ISO timestamp
/// when active, empty string when not muted (or expired). Stay
/// consistent with chip semantics — `mute_remaining_seconds()` returns
/// None for both "never muted" and "expired", so frontend treats both
/// as "not muted" without needing to distinguish.
#[tauri::command]
pub fn get_mute_until() -> String {
    let Some(secs) = mute_remaining_seconds() else {
        return String::new();
    };
    let until = chrono::Local::now() + chrono::Duration::seconds(secs);
    until.format("%Y-%m-%dT%H:%M:%S%:z").to_string()
}

/// Iter R55: set transient instruction note for `minutes` from now. Empty
/// text or 0 minutes clears. Distinct from mute (R52) — note doesn't
/// block proactive turns, just adds context. Returns ISO timestamp
/// when active, empty when cleared.
#[tauri::command]
pub fn set_transient_note(text: String, minutes: i64) -> String {
    // R59: pure helper extracted; Tauri command thin wrapper.
    let new_note = compute_new_transient_note(&text, minutes, chrono::Local::now());
    let until_iso = new_note
        .as_ref()
        .map(|n| n.until.format("%Y-%m-%dT%H:%M:%S%:z").to_string())
        .unwrap_or_default();
    if let Ok(mut g) = TRANSIENT_NOTE.lock() {
        *g = new_note;
    }
    until_iso
}

/// Iter R55: read current TRANSIENT_NOTE state. Returns `(text, until_iso)`
/// when active, both empty when none. Frontend uses both: text for chip
/// preview, until for countdown.
#[tauri::command]
pub fn get_transient_note() -> (String, String) {
    let Some(note) = TRANSIENT_NOTE.lock().ok().and_then(|g| g.clone()) else {
        return (String::new(), String::new());
    };
    let now = chrono::Local::now();
    if note.until <= now {
        return (String::new(), String::new());
    }
    (
        note.text,
        note.until.format("%Y-%m-%dT%H:%M:%S%:z").to_string(),
    )
}

// Iter QG5c-prep: pure time/calendar/idle-band helpers (idle_tier /
// user_absence_tier / period_of_day / weekday_zh / weekday_kind_zh /
// format_day_of_week_hint / minutes_until_quiet_start / in_quiet_hours)
// extracted to `proactive/time_helpers.rs`. Re-exported via the glob at
// the top of this file.

// Iter QG5d: gate logic (LoopAction, evaluate_pre_input_idle,
// evaluate_input_idle_gate, evaluate_loop_tick, wake_recent,
// WAKE_GRACE_WINDOW_SECS) moved to `proactive/gate.rs`.

/// Spawn the background engagement loop. Reads settings on every tick so changes take effect
/// without a restart. Honors `proactive.enabled`; sleeps a short fallback interval when disabled.
pub fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        // A short startup delay so we don't fire before the UI is ready.
        tokio::time::sleep(Duration::from_secs(20)).await;

        loop {
            let settings = match get_settings() {
                Ok(s) => s,
                Err(_) => {
                    tokio::time::sleep(Duration::from_secs(60)).await;
                    continue;
                }
            };
            let interval = settings.proactive.interval_seconds.max(60);

            // Heartbeat — if the gap since the last iteration is unexpectedly large the
            // process was likely suspended (laptop closed / system sleep). The detector
            // remembers the wake timestamp; run_proactive_turn looks it up to inject a
            // "welcome back" hint into the prompt.
            let wake_detector = app
                .state::<crate::wake_detector::WakeDetectorStore>()
                .inner()
                .clone();
            if let Some(gap) = wake_detector.observe().await {
                let log_store = app.state::<LogStore>().inner().clone();
                write_log(
                    &log_store.0,
                    &format!(
                        "Proactive: wake-from-sleep detected (gap {}s)",
                        gap.as_secs()
                    ),
                );
            }

            // 早安简报：与常规 proactive 节奏并列，但绕过 cooldown / chatty
            // 等数值化门控（自带"每日 1 次"语义）。先于 evaluate_loop_tick 跑，
            // 这样如果今天还没说早安，门控时刻一到就立刻触发；触发后通过
            // mark_proactive_spoken 让本 tick 之后的常规 cooldown 重新生效。
            let _ = maybe_run_morning_briefing(&app, &settings, chrono::Local::now()).await;

            let action = evaluate_loop_tick(&app, &settings).await;
            // Record before dispatching so even paths that immediately sleep are visible
            // in the panel — the entire point of this log is "why didn't anything happen".
            let decisions = app
                .state::<crate::decision_log::DecisionLogStore>()
                .inner()
                .clone();
            // Pull soft-rule context once so we can tag both the gate decision and the
            // post-LLM outcome with the same numbers — keeps the decision log explainable
            // when the pet stays silent because of a prompt-level rule rather than a gate.
            let chatty_today = crate::speech_history::today_speech_count().await;
            let chatty_threshold = settings.proactive.effective_chatty_threshold();
            let chatty_tag = chatty_mode_tag(chatty_today, chatty_threshold);
            // Snapshot the data-driven prompt rules that *will* fire this turn (icebreaker /
            // chatty / env-awareness). Computed at dispatch time so every decision-log
            // entry — Silent / Skip / Run / Spoke / LlmSilent / LlmError — gets stamped
            // with the same rule set the LLM saw, enabling event-by-event audit later.
            let lifetime_count = crate::speech_history::lifetime_speech_count().await;
            let env_counters = &app
                .state::<crate::commands::debug::ProcessCountersStore>()
                .inner()
                .env_tool;
            let env_total = env_counters
                .spoke_total
                .load(std::sync::atomic::Ordering::Relaxed);
            let env_with_any = env_counters
                .spoke_with_any
                .load(std::sync::atomic::Ordering::Relaxed);
            // Combine environmental + data-driven labels so the decision-log rules tag
            // matches the panel "prompt: N hints" badge — same composition order as
            // ToneSnapshot.active_prompt_rules.
            let now_for_rules = chrono::Local::now();
            let mood_for_rules = crate::mood::read_current_mood_parsed();
            let wake_ago_for_rules = app
                .state::<crate::wake_detector::WakeDetectorStore>()
                .inner()
                .last_wake_seconds_ago()
                .await;
            let pre_quiet_for_rules = minutes_until_quiet_start(
                now_for_rules.hour() as u8,
                now_for_rules.minute() as u8,
                settings.proactive.quiet_hours_start,
                settings.proactive.quiet_hours_end,
                15,
            )
            .is_some();
            let wake_back_for_rules = matches!(wake_ago_for_rules, Some(secs) if secs <= 600);
            let has_plan_for_rules = !build_plan_hint().is_empty();
            let env_label_set = active_environmental_rule_labels(
                wake_back_for_rules,
                mood_for_rules
                    .as_ref()
                    .map(|(t, _)| t.trim().is_empty())
                    .unwrap_or(true),
                pre_quiet_for_rules,
                !build_reminders_hint(now_for_rules.naive_local()).is_empty(),
                has_plan_for_rules,
                chatty_today == 0,
            );
            let companionship_days_for_rules = crate::companionship::companionship_days().await;
            let data_label_set = active_data_driven_rule_labels(
                lifetime_count as usize,
                chatty_today,
                chatty_threshold,
                env_total,
                env_with_any,
                companionship_days_for_rules,
            );
            // Pull cadence (minutes since last proactive) for the long-idle composite,
            // and idle-minutes (user-side absence) for the long-absence-reunion rule.
            // Same source as run_proactive_turn's cadence_hint / idle_register.
            let snap_for_rules = app
                .state::<InteractionClockStore>()
                .inner()
                .snapshot()
                .await;
            let since_last_for_rules = snap_for_rules.since_last_proactive_seconds.map(|s| s / 60);
            let idle_min_for_rules: u64 = snap_for_rules.idle_seconds / 60;
            let composite_label_set = active_composite_rule_labels(
                wake_back_for_rules,
                has_plan_for_rules,
                since_last_for_rules,
                chatty_today,
                chatty_threshold,
                pre_quiet_for_rules,
                idle_min_for_rules,
                now_for_rules.hour() as u8,
                late_night_wellness_in_cooldown(),
            );
            let active_labels: Vec<&'static str> = env_label_set
                .iter()
                .chain(data_label_set.iter())
                .chain(composite_label_set.iter())
                .copied()
                .collect();
            // Iter R8: stamp the wellness static at dispatch time when the rule
            // appears in this turn's active set. Doing it here (loop wrapper)
            // rather than after the LLM response means even if the model stays
            // silent, we still consume the 30-min window — preventing a near-
            // edge thrash where we'd reactivate the rule on the next tick.
            if active_labels.contains(&"late-night-wellness") {
                mark_late_night_wellness_fired();
            }
            let rules_tag = if active_labels.is_empty() {
                None
            } else {
                Some(format!("rules={}", active_labels.join("+")))
            };
            match &action {
                LoopAction::Silent { reason } => {
                    decisions.push("Silent", (*reason).to_string());
                }
                LoopAction::Skip(reason) => {
                    decisions.push("Skip", reason.clone());
                }
                LoopAction::Run {
                    idle_seconds,
                    input_idle_seconds,
                } => {
                    let mut reason = format!(
                        "idle={}s, input_idle={}",
                        idle_seconds,
                        input_idle_seconds
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| "?".to_string()),
                    );
                    if let Some(t) = &chatty_tag {
                        reason.push_str(", ");
                        reason.push_str(t);
                    }
                    if let Some(t) = &rules_tag {
                        reason.push_str(", ");
                        reason.push_str(t);
                    }
                    decisions.push("Run", reason);
                }
            }

            match action {
                LoopAction::Silent { .. } => {}
                LoopAction::Skip(reason) => {
                    let log_store = app.state::<LogStore>().inner().clone();
                    write_log(&log_store.0, &reason);
                }
                LoopAction::Run {
                    idle_seconds,
                    input_idle_seconds,
                } => {
                    // Record long-running prompt tilt (Iter 96): bump exactly one of four
                    // buckets based on the active label set we computed for this Run. Done
                    // here rather than after the LLM call so the count tracks "Run with
                    // these rules dispatched" — the prompt was sent regardless of outcome.
                    app.state::<crate::commands::debug::ProcessCountersStore>()
                        .inner()
                        .prompt_tilt
                        .record_dispatch(&active_labels);
                    let outcome = run_proactive_turn(&app, idle_seconds, input_idle_seconds).await;
                    let chatty_part = chatty_tag.clone().unwrap_or_else(|| "-".to_string());
                    let counters_for_outcome = app
                        .state::<crate::commands::debug::ProcessCountersStore>()
                        .inner()
                        .clone();
                    record_proactive_outcome(
                        &counters_for_outcome,
                        &decisions,
                        "loop",
                        &chatty_part,
                        rules_tag.as_deref(),
                        &outcome,
                    );
                    if let Err(e) = outcome {
                        eprintln!("Proactive turn failed: {}", e);
                    }
                }
            }

            tokio::time::sleep(Duration::from_secs(interval)).await;
        }
    });
}

/// Build the prompt, ask the LLM, emit the reply, and persist it. Returns the spoken
/// reply text on success — `Some(text)` when the pet actually said something, `None`
/// when it chose to stay silent. Callers can use this for status display; the spawn
/// loop discards the value.
/// What `run_proactive_turn` returns. `reply == Some` means the pet spoke; `None` means
/// it stayed silent (empty reply or `<silent>` marker). `tools` lists the unique tool
/// names the LLM called during this turn — empty when the model ignored every tool, or
/// when the turn aborted before reaching the final pipeline response.
pub struct ProactiveTurnOutcome {
    pub reply: Option<String>,
    pub tools: Vec<String>,
}

async fn run_proactive_turn(
    app: &AppHandle,
    idle_seconds: u64,
    input_idle_seconds: Option<u64>,
) -> Result<ProactiveTurnOutcome, String> {
    let config = AiConfig::from_settings()?;
    let mcp_store = app.state::<McpManagerStore>().inner().clone();
    let log_store = app.state::<LogStore>().inner().clone();
    let shell_store = app.state::<ShellStore>().inner().clone();
    let process_counters = app
        .state::<crate::commands::debug::ProcessCountersStore>()
        .inner()
        .clone();
    let clock = app.state::<InteractionClockStore>().inner().clone();

    let tools_used: std::sync::Arc<std::sync::Mutex<Vec<String>>> =
        std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    // 调试器：收 LLM 在本 turn 中的全部工具调用（name+args+result），按调用
    // 顺序。run_chat_pipeline 在 ctx.tool_calls.is_some() 时会推到这里。
    // 处理路径外的 chat 流（reactive / telegram / consolidate）不带这个
    // collector → 零开销。
    let tool_calls: std::sync::Arc<std::sync::Mutex<Vec<crate::proactive::ToolCallEntry>>> =
        std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    // Iter TR3: proactive turns also flow through the human-review gate. The
    // panel modal can resolve a parked tool call regardless of which entry
    // point initiated it.
    let tool_review = app
        .state::<crate::tool_review::ToolReviewRegistryStore>()
        .inner()
        .clone();
    // Iter R2: forward the decision_log so review outcomes show up alongside
    // proactive Spoke / Silent / Skip entries in the panel.
    let decision_log_for_ctx = app
        .state::<crate::decision_log::DecisionLogStore>()
        .inner()
        .clone();
    let ctx = ToolContext::new(log_store, shell_store, process_counters)
        .with_tools_used_collector(tools_used.clone())
        .with_tool_calls_collector(tool_calls.clone())
        .with_tool_review(tool_review)
        .with_decision_log(decision_log_for_ctx);

    // Try to load the latest session so the proactive turn has the recent context. If none
    // exists yet, fall back to a system-only conversation.
    let (session_id, mut messages) = load_active_session();

    let soul = get_soul().unwrap_or_default();
    let now_local = chrono::Local::now();
    // Iter R12: silent end-of-day review write. Idempotent per day (LAST_DAILY_REVIEW_DATE
    // + index existence check). Runs before the rest of the turn so the memory write
    // happens even if the turn is later gated to Silent — the review is its own outcome.
    maybe_run_daily_review(now_local).await;
    let idle_minutes = idle_seconds / 60;
    let input_hint = match input_idle_seconds {
        Some(secs) => format!("用户键鼠空闲约 {} 秒。", secs),
        None => "（无法读取键鼠空闲信息。）".to_string(),
    };

    let mood_parsed = read_current_mood_parsed();
    let is_first_mood = !matches!(&mood_parsed, Some((text, _)) if !text.trim().is_empty());
    let mood_hint = format_proactive_mood_hint(
        mood_parsed.as_ref().map(|(t, _)| t.as_str()).unwrap_or(""),
        &|s| crate::redaction::redact_with_settings(s),
    );

    // Distance since the pet last spoke proactively — different from idle_seconds (which
    // resets on any interaction). Lets the LLM pick a register: continuation vs. casual
    // check-in vs. "haven't talked in ages".
    let (cadence_hint, since_last_proactive_minutes) = {
        let snap = clock.snapshot().await;
        let mins = snap.since_last_proactive_seconds.map(|s| s / 60);
        let hint = match mins {
            Some(m) => format!("距上次你主动开口约 {} 分钟（{}）。", m, idle_tier(m)),
            None => "你还没有主动开过口，这是第一次。".to_string(),
        };
        (hint, mins)
    };

    // Iter R1: classify the previous proactive turn now that we're firing a new
    // one. raw_awaiting tells us whether the user actually replied between the
    // two: false → replied (mark_user_message cleared the flag), true → ignored.
    // Dedup via LAST_FEEDBACK_RECORDED_FOR keyed on the prior turn's timestamp.
    {
        let prev_ts = LAST_PROACTIVE_TIMESTAMP.lock().ok().and_then(|g| g.clone());
        let prev_reply = LAST_PROACTIVE_REPLY.lock().ok().and_then(|g| g.clone());
        let already_for = LAST_FEEDBACK_RECORDED_FOR
            .lock()
            .ok()
            .and_then(|g| g.clone());
        if let (Some(ts), Some(text)) = (prev_ts.clone(), prev_reply) {
            if Some(&ts) != already_for.as_ref() {
                let raw = clock.raw_awaiting().await;
                let kind = if raw {
                    crate::feedback_history::FeedbackKind::Ignored
                } else {
                    crate::feedback_history::FeedbackKind::Replied
                };
                crate::feedback_history::record_event(kind, text.trim()).await;
                if let Ok(mut g) = LAST_FEEDBACK_RECORDED_FOR.lock() {
                    *g = Some(ts);
                }
            }
        }
    }
    // Build the hint from the most recent entry (after we may have just written
    // one above). Empty when there's no history yet.
    //
    // Iter R26: widened from `recent_feedback(1)` to `recent_feedback(20)` —
    // last entry feeds `format_feedback_hint` (latest event), full window
    // feeds `format_feedback_aggregate_hint` (trend). Same window the
    // gate's R7 cooldown adapter uses, so prompt and gate agree on
    // "recent era" of feedback.
    let recent_feedback = crate::feedback_history::recent_feedback(20).await;
    // R60: pass redact closure so excerpt gets the same privacy filter
    // as other prompt hints (speech_hint / repeated_topic_hint etc).
    let feedback_hint = crate::feedback_history::format_feedback_hint(&recent_feedback, &|s| {
        crate::redaction::redact_with_settings(s)
    });
    let feedback_aggregate_hint =
        crate::feedback_history::format_feedback_aggregate_hint(&recent_feedback);
    // Iter R33: trailing silence streak detection. Reads the ring buffer
    // (cap=5) and counts how many of the most-recent turns ended in
    // "silent". If ≥3, prompt gets a nudge to break the streak — but
    // softly ("否则继续沉默也无妨" preserves LLM judgment).
    let consecutive_silent_hint = {
        const SILENT_STREAK_THRESHOLD: usize = 3;
        let streak = LAST_PROACTIVE_TURNS
            .lock()
            .ok()
            .map(|g| {
                let snap: Vec<TurnRecord> = g.iter().cloned().collect();
                count_trailing_silent(&snap)
            })
            .unwrap_or(0);
        format_consecutive_silent_hint(streak, SILENT_STREAK_THRESHOLD)
    };
    // Iter R35: mirror on the feedback side — trailing-negative streak
    // (Ignored | Dismissed in a row). 3+ is "I'm not landing" — try a
    // different angle. Reuses the recent_feedback fetch (R26 widened
    // it to (20)).
    let consecutive_negative_hint = {
        const NEGATIVE_STREAK_THRESHOLD: usize = 3;
        let streak = crate::feedback_history::count_trailing_negative(&recent_feedback);
        crate::feedback_history::format_consecutive_negative_hint(streak, NEGATIVE_STREAK_THRESHOLD)
    };
    // Iter R55: transient instruction note. Wraps the user-provided text
    // with a clear "[临时指示]" header so LLM treats it as authoritative
    // current-state directive (vs general history hint).
    let transient_note_hint = match transient_note_active() {
        Some(text) => format!(
            "[临时指示] 用户当前留下的状态/指令：「{}」。这是用户主动告知 pet 的当前状态，开口时请直接尊重 / 配合，不要怀疑或追问。",
            crate::redaction::redact_with_settings(&text)
        ),
        None => String::new(),
    };

    // If the proactive loop noticed a sleep gap recently (≤ 10 minutes ago), surface it
    // so the LLM can choose a "welcome back" register. Strong signal that the user was
    // physically away rather than just idle at the desk.
    let wake_hint = {
        let detector = app
            .state::<crate::wake_detector::WakeDetectorStore>()
            .inner()
            .clone();
        match detector.last_wake_seconds_ago().await {
            Some(secs) if secs <= 600 => {
                format!(
                    "（用户的电脑在大约 {} 秒前刚从休眠唤醒，看起来 ta 离开桌子一会儿后才回来。）",
                    secs
                )
            }
            _ => String::new(),
        }
    };

    // Pull the pet's recent proactive lines from a dedicated history file so the model
    // doesn't repeat itself. Independent of session messages — survives session resets
    // and chat.max_context_messages trimming.
    // Iter R11: read recent speeches once and reuse for both speech_hint
    // (the bullet list of past utterances) and repeated_topic_hint (the
    // redundancy detector). Same window so both layers see the same
    // mental model.
    let recent_speeches = crate::speech_history::recent_speeches(5).await;
    let speech_hint = if recent_speeches.is_empty() {
        String::new()
    } else {
        // Iter Cy: redact each line before re-injecting into the prompt. The pet's
        // own past utterances may have referenced private terms (the LLM doesn't
        // know to self-redact); redacting at read-time prevents re-leak even
        // though the on-disk history file stays pristine.
        let bullets: Vec<String> = recent_speeches
            .iter()
            .map(|line| {
                let stripped = crate::speech_history::strip_timestamp(line);
                format!("· {}", crate::redaction::redact_with_settings(stripped))
            })
            .collect();
        format!(
            "你最近主动说过的几句话（旧→新），开口前看一眼避免重复：\n{}",
            bullets.join("\n")
        )
    };
    // Iter R11: 4-char window, fires when a topic appears in ≥ 3 of the
    // last 5 utterances. The bullets in `speech_hint` already redact
    // private terms; the detector runs on raw lines (after timestamp
    // strip) so it operates on actual content. Resulting hint then gets
    // its own redaction pass — defense in depth.
    let repeated_topic_hint =
        match crate::speech_history::detect_repeated_topic(&recent_speeches, 4, 3) {
            Some(topic) => crate::redaction::redact_with_settings(&format!(
                "你最近多次提到「{}」——这次开口请换个角度或换个话题，避免让用户觉得在重复。",
                topic
            )),
            None => String::new(),
        };
    // Iter R19: length-register variance nudge. Reuses the same recent_speeches
    // binding as speech_hint + repeated_topic_hint — three layers of insight
    // (bullet list / topic ngram / length distribution) from one fetch.
    let length_register_hint = crate::speech_history::format_speech_length_hint(&recent_speeches);

    // Surface the user's active Focus mode (if any) so the pet can speak around it. This
    // path normally only runs when the user has unset `respect_focus_mode` — otherwise the
    // gate would have skipped before we got here.
    let focus_hint = match crate::focus_mode::focus_status().await {
        Some(s) if s.active => match &s.name {
            Some(n) => format!(
                "用户当前开着 macOS Focus 模式：「{}」（说明 ta 想专注，开口要克制）。",
                n
            ),
            None => "用户当前开着某个 macOS Focus 模式（说明 ta 想专注，开口要克制）。".to_string(),
        },
        _ => String::new(),
    };

    let period = period_of_day(now_local.hour() as u8);
    let time_str = now_local.format("%Y-%m-%d %H:%M").to_string();
    // Iter Cβ: weekday/weekend label, e.g. "周日 · 周末". Joins with period in the
    // time line so the LLM can lean on "周五晚上"-flavor cues without parsing dates.
    let day_of_week = format_day_of_week_hint(now_local.weekday());
    // Iter Cμ: register cue derived from idle_minutes — "用户刚刚还在" vs
    // "用户至少一天没和你互动". Lets the LLM differentiate 5-min idle from 5-hour idle.
    let idle_register = user_absence_tier(idle_minutes);

    // Scan the `todo` memory category for user-set reminders that have just come due.
    // Each becomes a bullet line. The whole hint is empty when nothing's due.
    let reminders_hint = build_reminders_hint(now_local.naive_local());

    // Pull the pet's own short-term plan from ai_insights/daily_plan, if it has written one.
    let plan_hint = build_plan_hint();
    let persona_hint = build_persona_hint();
    // Iter 103: read mood-trend summary from mood_history.log (window=50, min=5).
    // Window is generous because mood is deduped against the last entry, so 50 lines
    // typically span 1-2 weeks of distinct mood changes. min=5 avoids early-day noise.
    let mood_trend_hint = crate::mood_history::build_trend_hint(50, 5).await;
    // Iter Cα: surface user_profile memory as ambient context so the LLM sees
    // basic user habits without firing memory_search every turn. Empty until
    // the pet has written at least one user_profile entry.
    let user_profile_hint = build_user_profile_hint();
    // Iter Cγ: surface owner-assigned butler tasks each proactive turn so the
    // pet's task queue stays visible — the pet shouldn't forget the user asked
    // it to "每天早上发日历" between turns. Iter Cζ adds schedule-awareness
    // (`[every: HH:MM]` / `[once: ...]` prefixes); `now` is passed so due tasks
    // bubble to the top with a "⏰ 到期" marker.
    let butler_tasks_hint = build_butler_tasks_hint(now_local.naive_local());
    // 长任务心跳：把"被动过手却停滞过久"的 pending 任务点名出来，让 LLM
    // 这一轮要么写一句进展、要么改 done / error。读 settings 决定阈值
    // (0 = 关闭)，IO 层在 build_task_heartbeat_hint 里已做空串短路。
    let task_heartbeat_hint = {
        let threshold = get_settings()
            .map(|s| s.proactive.task_heartbeat_minutes)
            .unwrap_or(0);
        build_task_heartbeat_hint(now_local.naive_local(), threshold)
    };

    // Iter R77: pull butler_tasks with `[deadline:]` prefix and format the
    // urgency-aware hint. Reads same memory category as butler_tasks_hint
    // but filters to deadline-prefixed items only — pet reminds user about
    // the deadline rather than auto-executing. Empty when no Approaching /
    // Imminent / Overdue deadlines.
    let deadline_hint = build_butler_deadlines_hint(now_local.naive_local());
    // Iter Cυ: owner-name from settings, empty when unset.
    let user_name = get_settings().map(|s| s.user_name).unwrap_or_default();

    // Lifetime proactive utterance count — drives the icebreaker rule.
    let proactive_history_count = crate::speech_history::count_speeches().await;
    // Days since the pet was first installed — drives the long-term persona register
    // (Iter 101). On first ever proactive turn this also writes install_date.txt.
    let companionship_days = crate::companionship::companionship_days().await;
    // Today's proactive count from the per-day sidecar — drives the "tone it down today"
    // rule when at or above the user-configurable threshold.
    let today_speech_count = crate::speech_history::today_speech_count().await;
    let chatty_day_threshold = get_settings()
        .ok()
        .map(|s| s.proactive.effective_chatty_threshold())
        .unwrap_or(5);
    // Iter R15: snapshot the foreground app + update the duration tracker.
    // Hint fires when the user has been in the same app ≥ MIN_DURATION_MINUTES;
    // empty otherwise. Reads via the same osascript path as get_active_window
    // tool — non-macOS / failure → None → empty hint, no panic.
    let current_app = crate::tools::system_tools::current_active_window()
        .await
        .map(|(app, _)| app);
    let active_app_hint = update_and_format_active_app_hint(current_app.as_deref());

    // Iter R63: take-and-clear the deep-focus recovery hint if a hard-block
    // ended within the last 10 minutes. Single-shot per block stretch — once
    // taken, won't re-fire on subsequent runs in the same window. Empty when
    // no recent hard-block or already consumed.
    let deep_focus_recovery_hint = take_recovery_hint();

    // Iter R66: yesterday's deep-focus recap, gated to first-of-day like
    // cross_day_hint / yesterday_recap_hint. Empty when no yesterday data
    // (process restarted today / fresh install / quiet day yesterday).
    let yesterday_focus_hint = if today_speech_count == 0 {
        format_yesterday_focus_recap_hint(yesterday_block_stats().as_ref())
    } else {
        String::new()
    };

    // Iter R74: personal-record celebration. Fires only when today's peak
    // strictly exceeds the prior 7-day best (no tied / first-ever). Empty
    // string when no record. Not first-of-day gated — celebrate as soon
    // as the new peak finalizes (could be multiple turns after a long
    // stretch wraps).
    let personal_record_hint = current_personal_record_hint();

    // Iter R14: at the first proactive turn of a new day, surface yesterday's
    // last 2 utterances so the pet can pick up a thread instead of starting
    // cold every morning. Empty when (a) not first-of-day or (b) yesterday
    // has no recorded speeches. Each line is timestamp-stripped + redacted.
    //
    // Iter R16: also pull yesterday's `daily_review_YYYY-MM-DD` description as
    // a separate "总览" hint — pairs with the speeches as high-level + specific
    // two-layer recap. Same first-of-day gate.
    let yesterday = now_local.date_naive() - chrono::Duration::days(1);
    let cross_day_hint = if today_speech_count == 0 {
        let lines = crate::speech_history::speeches_for_date_async(yesterday, 2).await;
        if lines.is_empty() {
            String::new()
        } else {
            let bullets: Vec<String> = lines
                .iter()
                .map(|line| {
                    let stripped = crate::speech_history::strip_timestamp(line);
                    format!("· {}", crate::redaction::redact_with_settings(stripped))
                })
                .collect();
            format!(
                "[昨日尾声] 昨天最后说过：\n{}\n如果话题自然能续上就续，不必生硬呼应。",
                bullets.join("\n")
            )
        }
    } else {
        String::new()
    };
    let yesterday_recap_hint = if today_speech_count == 0 {
        let desc = read_daily_review_description(yesterday);
        format_yesterday_recap_hint(desc.as_deref())
    } else {
        String::new()
    };
    // Env-awareness ratio over the recent window (process-wide atomic, reset by panel).
    // Drives a self-correction rule: when the model's been ignoring env tools, nudge it
    // to call get_active_window this turn.
    let env_counters = &app
        .state::<crate::commands::debug::ProcessCountersStore>()
        .inner()
        .env_tool;
    let env_spoke_total = env_counters
        .spoke_total
        .load(std::sync::atomic::Ordering::Relaxed);
    let env_spoke_with_any = env_counters
        .spoke_with_any
        .load(std::sync::atomic::Ordering::Relaxed);

    let pre_quiet_minutes = {
        let settings = get_settings().ok();
        settings.and_then(|s| {
            minutes_until_quiet_start(
                now_local.hour() as u8,
                now_local.minute() as u8,
                s.proactive.quiet_hours_start,
                s.proactive.quiet_hours_end,
                15,
            )
        })
    };
    let prompt = build_proactive_prompt(&PromptInputs {
        time: &time_str,
        period,
        day_of_week: &day_of_week,
        idle_minutes,
        idle_register,
        input_hint: &input_hint,
        cadence_hint: &cadence_hint,
        mood_hint: &mood_hint,
        focus_hint: &focus_hint,
        wake_hint: &wake_hint,
        speech_hint: &speech_hint,
        is_first_mood,
        pre_quiet_minutes,
        reminders_hint: &reminders_hint,
        plan_hint: &plan_hint,
        proactive_history_count,
        today_speech_count,
        chatty_day_threshold,
        env_spoke_total,
        env_spoke_with_any,
        since_last_proactive_minutes,
        companionship_days,
        persona_hint: &persona_hint,
        mood_trend_hint: &mood_trend_hint,
        user_profile_hint: &user_profile_hint,
        butler_tasks_hint: &butler_tasks_hint,
        task_heartbeat_hint: &task_heartbeat_hint,
        user_name: &user_name,
        feedback_hint: &feedback_hint,
        feedback_aggregate_hint: &feedback_aggregate_hint,
        consecutive_silent_hint: &consecutive_silent_hint,
        consecutive_negative_hint: &consecutive_negative_hint,
        transient_note_hint: &transient_note_hint,
        hour: now_local.hour() as u8,
        recently_fired_wellness: late_night_wellness_in_cooldown(),
        repeated_topic_hint: &repeated_topic_hint,
        cross_day_hint: &cross_day_hint,
        active_app_hint: &active_app_hint,
        yesterday_recap_hint: &yesterday_recap_hint,
        length_register_hint: &length_register_hint,
        deep_focus_recovery_hint: &deep_focus_recovery_hint,
        yesterday_focus_hint: &yesterday_focus_hint,
        personal_record_hint: &personal_record_hint,
        deadline_hint: &deadline_hint,
    });
    // Iter E1: stash the prompt so the panel can show "what did the LLM see this
    // turn?" — useful for prompt tuning without instrumenting log scraping.
    if let Ok(mut g) = LAST_PROACTIVE_PROMPT.lock() {
        *g = Some(prompt.clone());
    }
    // Iter E3: also stash the timestamp at prompt-build time. Set here (not at
    // reply time) so the user sees "when was this turn started" — closer to
    // when the displayed signals (mood/cadence/etc.) were sampled.
    if let Ok(mut g) = LAST_PROACTIVE_TIMESTAMP.lock() {
        *g = Some(now_local.format("%Y-%m-%d %H:%M:%S").to_string());
    }

    // Ensure system message anchors the conversation; build a temporary message list.
    if messages.is_empty() {
        messages.push(serde_json::json!({ "role": "system", "content": soul }));
    }
    messages.push(serde_json::json!({ "role": "user", "content": prompt }));

    let chat_messages: Vec<ChatMessage> = messages
        .iter()
        .filter_map(|v| serde_json::from_value(v.clone()).ok())
        .collect();

    let sink = CollectingSink::new();
    let reply = run_chat_pipeline(chat_messages, &sink, &config, &mcp_store, &ctx).await?;
    let reply_trimmed = reply.trim();
    // Iter E2: stash the raw reply so the panel modal can pair it with the
    // prompt — full request/response loop in one view.
    if let Ok(mut g) = LAST_PROACTIVE_REPLY.lock() {
        *g = Some(reply.clone());
    }

    let tools = tools_used.lock().map(|g| g.clone()).unwrap_or_default();
    // Iter E3: stash the distinct tool names alongside the prompt/reply pair.
    // Deduped via BTreeSet then collected so panel UI doesn't have to handle
    // duplicates from the registry's per-call list.
    let tools_dedup: Vec<String> = {
        let mut seen = std::collections::BTreeSet::new();
        for t in &tools {
            seen.insert(t.clone());
        }
        seen.into_iter().collect()
    };
    if let Ok(mut g) = LAST_PROACTIVE_TOOLS.lock() {
        *g = tools_dedup.clone();
    }
    // 调试器：拿出本 turn 累积的完整 tool 调用记录（name+args+result，按
    // LLM 调用顺序），随 TurnRecord 一起进 ring buffer 给 modal 展示。
    let tool_calls_collected = tool_calls.lock().map(|g| g.clone()).unwrap_or_default();
    // Iter E4: also append the full turn record to the ring buffer so the
    // panel can navigate prev/next across the last N turns. Cap at
    // PROACTIVE_TURN_HISTORY_CAP via pop_front.
    if let Ok(mut g) = LAST_PROACTIVE_TURNS.lock() {
        let ts = LAST_PROACTIVE_TIMESTAMP
            .lock()
            .ok()
            .and_then(|t| t.clone())
            .unwrap_or_default();
        // Iter R25: classify outcome inline. Same condition the silent-marker
        // check below uses — kept in sync so TurnRecord.outcome doesn't drift
        // from the actual return path.
        let outcome = if reply_trimmed.is_empty() || reply_trimmed.contains(SILENT_MARKER) {
            "silent"
        } else {
            "spoke"
        };
        g.push_back(TurnRecord {
            timestamp: ts,
            prompt: prompt.clone(),
            reply: reply.clone(),
            tools_used: tools_dedup,
            tool_calls: tool_calls_collected,
            outcome: outcome.to_string(),
        });
        while g.len() > PROACTIVE_TURN_HISTORY_CAP {
            g.pop_front();
        }
    }

    // Treat empty / silent marker as "do nothing".
    if reply_trimmed.is_empty() || reply_trimmed.contains(SILENT_MARKER) {
        ctx.log(&format!("Proactive: silent (idle={}s)", idle_seconds));
        return Ok(ProactiveTurnOutcome { reply: None, tools });
    }

    ctx.log(&format!(
        "Proactive: speaking ({} chars, idle={}s)",
        reply_trimmed.len(),
        idle_seconds
    ));

    // Persist into the active session: the proactive prompt is hidden from the user, but the
    // assistant's reply is shown so the conversation context stays coherent.
    if let Some(id) = session_id {
        let _ = persist_assistant_message(&id, reply_trimmed);
    }

    clock.mark_proactive_spoken().await;
    // Append to the dedicated speech history so the next proactive turn's prompt can
    // surface this line back to the LLM and avoid repetition.
    crate::speech_history::record_speech(reply_trimmed).await;

    // Re-read mood after the turn — if the LLM updated it via memory_edit, the file has been
    // rewritten and we should ship the latest snapshot to the frontend.
    let (mood_after, motion_after) = read_mood_for_event(&ctx, "Proactive");
    // Iter 103: append the post-turn mood to mood_history.log (best-effort, deduped
    // against last entry inside record_mood). Captures the trajectory of the pet's
    // emotional register over time so future proactive turns can reflect on it.
    if let Some(text) = &mood_after {
        crate::mood_history::record_mood(text, &motion_after).await;
    }

    let payload = ProactiveMessage {
        text: reply_trimmed.to_string(),
        timestamp: now_local.format("%Y-%m-%dT%H:%M:%S%.3f").to_string(),
        mood: mood_after,
        motion: motion_after,
    };
    let _ = app.emit("proactive-message", payload);

    Ok(ProactiveTurnOutcome {
        reply: Some(reply_trimmed.to_string()),
        tools,
    })
}

/// Scan the `todo` memory category for items whose description starts with a reminder
/// prefix and are due now (within the 30-minute window). Returns a multi-line bullet
/// hint, or empty string when nothing is due. `now` is injected so tests / call sites
/// share the same time anchor for parsed Absolute targets.
fn build_reminders_hint(now: chrono::NaiveDateTime) -> String {
    let Ok(index) = crate::commands::memory::memory_list(Some("todo".to_string())) else {
        return String::new();
    };
    let Some(cat) = index.categories.get("todo") else {
        return String::new();
    };
    let mut items: Vec<(String, String, String)> = Vec::new();
    for item in &cat.items {
        if let Some((target, topic)) = parse_reminder_prefix(&item.description) {
            if is_reminder_due(&target, now, 30) {
                items.push((format_target(&target), topic, item.title.clone()));
            }
        }
    }
    format_reminders_hint(&items, &|s| crate::redaction::redact_with_settings(s))
}

// Iter QG5b: butler-tasks pure helpers (ButlerSchedule, parse / due /
// completion / format_butler_tasks_block + the two prompt-block consts)
// extracted to `proactive/butler_schedule.rs`. Re-exported via the glob at
// the top of this file so external callers (`consolidate.rs`'s
// `is_completed_once` import, panel commands) keep using
// `crate::proactive::...` paths.

/// Iter R77: read butler_tasks memory + extract `[deadline:]` prefixed items,
/// format the urgency-tier hint. Distinct from build_butler_tasks_hint
/// which surfaces the full task list — this one is laser-focused on
/// time-urgent deadline reminders. Empty when no deadlines / all Distant.
pub fn build_butler_deadlines_hint(now: chrono::NaiveDateTime) -> String {
    let Ok(index) = crate::commands::memory::memory_list(Some("butler_tasks".to_string())) else {
        return String::new();
    };
    let Some(cat) = index.categories.get("butler_tasks") else {
        return String::new();
    };
    let items: Vec<(chrono::NaiveDateTime, String)> = cat
        .items
        .iter()
        .filter_map(|i| parse_butler_deadline_prefix(&i.description))
        .collect();
    let block = format_butler_deadlines_hint(&items, now);
    if block.is_empty() {
        return String::new();
    }
    crate::redaction::redact_with_settings(&block)
}

/// 长任务心跳的 IO 层封装：读 `butler_tasks` → 过滤心跳候选 → 把命中
/// 的标题列表交给 `format_heartbeat_hint`，最后过 redaction。
///
/// 与 `build_butler_tasks_hint` 互补 —— butler_tasks_hint 是"队列里还有
/// 什么待办"的全景，task_heartbeat_hint 是"哪几条已经动过手却卡住了"的
/// 局部追踪。两个 hint 都注入到 prompt 时，LLM 既能看到完整队列又能看
/// 到必须本轮处理的"心跳点名"。
///
/// `threshold_minutes == 0` 时不做任何 IO，直接返回空串 — 让禁用路径几乎
/// 零成本。失败模式（memory_list 失败 / 类目缺失）静默退化为空串。
pub fn build_task_heartbeat_hint(
    now: chrono::NaiveDateTime,
    threshold_minutes: u32,
) -> String {
    if threshold_minutes == 0 {
        return String::new();
    }
    let Ok(index) = crate::commands::memory::memory_list(Some("butler_tasks".to_string())) else {
        return String::new();
    };
    let Some(cat) = index.categories.get("butler_tasks") else {
        return String::new();
    };
    let titles: Vec<String> = cat
        .items
        .iter()
        .filter(|i| {
            crate::task_heartbeat::is_heartbeat_candidate(
                &i.description,
                &i.created_at,
                &i.updated_at,
                now,
                threshold_minutes,
            )
        })
        .map(|i| i.title.clone())
        .collect();
    let block = crate::task_heartbeat::format_heartbeat_hint(&titles, threshold_minutes);
    if block.is_empty() {
        return String::new();
    }
    crate::redaction::redact_with_settings(&block)
}

/// Read butler_tasks memory entries and format the prompt-side digest. `now` is
/// injected so the call site (run_proactive_turn) shares one clock anchor with the
/// rest of the prompt build. Returns "" when the category is empty. Output is redacted
/// via `redact_with_settings` for the same reason as `build_user_profile_hint`.
pub fn build_butler_tasks_hint(now: chrono::NaiveDateTime) -> String {
    let Ok(index) = crate::commands::memory::memory_list(Some("butler_tasks".to_string())) else {
        return String::new();
    };
    let Some(cat) = index.categories.get("butler_tasks") else {
        return String::new();
    };
    let tuples: Vec<(String, String, String)> = cat
        .items
        .iter()
        .map(|i| (i.title.clone(), i.description.clone(), i.updated_at.clone()))
        .collect();
    let block = format_butler_tasks_block(
        &tuples,
        BUTLER_TASKS_HINT_MAX_ITEMS,
        BUTLER_TASKS_HINT_DESC_CHARS,
        now,
    );
    if block.is_empty() {
        return String::new();
    }
    crate::redaction::redact_with_settings(&block)
}

/// Iter D5: serializable shape for `get_persona_summary` — text + last-updated
/// timestamp so the Persona panel can show "X days ago" freshness. `text` is
/// empty when no summary exists yet; `updated_at` is the ISO-8601 from the
/// memory entry, also empty in that case.
#[derive(serde::Serialize)]
pub struct PersonaSummary {
    pub text: String,
    pub updated_at: String,
}

/// Tauri command returning the raw persona-summary description (Iter 105) — without
/// the "你最近一次自我反思的画像（来自 consolidate）：" header `build_persona_hint`
/// adds. The Persona panel surfaces this directly so users can read what the pet
/// has written about itself. Iter D5: now returns text + updated_at so the panel
/// can display freshness ("X 天前更新").
#[tauri::command]
pub fn get_persona_summary() -> PersonaSummary {
    crate::commands::memory::read_ai_insights_item("persona_summary")
        .map(|i| PersonaSummary {
            text: i.description.trim().to_string(),
            updated_at: i.updated_at,
        })
        .unwrap_or_else(|| PersonaSummary {
            text: String::new(),
            updated_at: String::new(),
        })
}

/// Read the pet's self-authored persona summary from `ai_insights/persona_summary`.
/// Iter 102: this is what the consolidate loop generates by reflecting on recent
/// speech_history + user_profile. Returns the description verbatim with a header line,
/// or empty when no summary has been written yet (fresh installs / not enough signal).
///
/// `pub` since Iter 104 — reactive chat reuses this to inject the same persona layer
/// into its system prompt, so the long-term identity isn't proactive-only.
pub fn build_persona_hint() -> String {
    let Some(item) = crate::commands::memory::read_ai_insights_item("persona_summary") else {
        return String::new();
    };
    if item.description.trim().is_empty() {
        return String::new();
    }
    // Iter Cw: redact the persona summary before re-injecting into the
    // proactive prompt. The LLM-authored description may have echoed private
    // terms (active_window app names / user_profile entries it didn't know
    // were sensitive when it wrote them); redacting here ensures the same
    // user-configured patterns cover this self-loop input too. The on-disk
    // memory file stays pristine — the panel's `get_persona_summary` command
    // intentionally returns the unredacted text since that view is local.
    let redacted = crate::redaction::redact_with_settings(item.description.trim());
    format!(
        "你最近一次自我反思的画像（来自 consolidate）：\n{}",
        redacted
    )
}

/// Cap on how many `user_profile` entries to surface in the proactive prompt. Above
/// this the digest gets long enough to dominate the prompt and dilute its other
/// signals; the LLM can still call `memory_search` for the older ones if a topic
/// asks for them.
pub const USER_PROFILE_HINT_MAX_ITEMS: usize = 6;
/// Per-entry description char cap. Long bios become noisy when stacked 6 deep, so
/// the prompt sees a one-liner per habit; the full body is one tool call away.
pub const USER_PROFILE_HINT_DESC_CHARS: usize = 80;

/// Pure helper — formats a list of `(title, description, updated_at)` tuples into
/// the user-profile prompt block. Sorted by `updated_at` descending so the most
/// recently-touched habits surface first. Returns "" when items is empty so the
/// prompt builder's `push_if_nonempty` skips the line cleanly.
///
/// Extracted from `build_user_profile_hint` so the truncation / sort / header logic
/// is unit-testable without going through `memory_list`'s on-disk index.
pub fn format_user_profile_block(
    items: &[(String, String, String)],
    max_items: usize,
    max_desc_chars: usize,
) -> String {
    if items.is_empty() || max_items == 0 {
        return String::new();
    }
    let mut sorted: Vec<&(String, String, String)> = items.iter().collect();
    // updated_at is ISO-8601 with offset → string compare matches chronological
    // order; descending = most recent first.
    sorted.sort_by(|a, b| b.2.cmp(&a.2));
    let n = sorted.len().min(max_items);
    let mut lines: Vec<String> = Vec::with_capacity(n + 1);
    lines.push(format!(
        "你了解的用户习惯（来自 user_profile 记忆，最新 {} 条）：",
        n
    ));
    for (title, desc, _) in sorted.iter().take(n) {
        let trimmed = desc.trim();
        let truncated: String = if trimmed.chars().count() <= max_desc_chars {
            trimmed.to_string()
        } else {
            let head: String = trimmed.chars().take(max_desc_chars).collect();
            format!("{}…", head)
        };
        lines.push(format!("- {}：{}", title.trim(), truncated));
    }
    lines.join("\n")
}

/// Read `user_profile` memory entries and format a compact digest block for the
/// proactive prompt (Iter Cα). Returns empty when the category has no entries —
/// `push_if_nonempty` then skips it cleanly. Output is redacted via
/// `redact_with_settings` so any private terms the LLM stored in habit
/// descriptions don't leak back into a fresh proactive prompt.
pub fn build_user_profile_hint() -> String {
    let Ok(index) = crate::commands::memory::memory_list(Some("user_profile".to_string())) else {
        return String::new();
    };
    let Some(cat) = index.categories.get("user_profile") else {
        return String::new();
    };
    let tuples: Vec<(String, String, String)> = cat
        .items
        .iter()
        .map(|i| (i.title.clone(), i.description.clone(), i.updated_at.clone()))
        .collect();
    let block = format_user_profile_block(
        &tuples,
        USER_PROFILE_HINT_MAX_ITEMS,
        USER_PROFILE_HINT_DESC_CHARS,
    );
    if block.is_empty() {
        return String::new();
    }
    crate::redaction::redact_with_settings(&block)
}

/// Iter R12: bare description read of `ai_insights/daily_plan`. Used by the
/// daily-review writer which needs the plan body without the prompt-hint
/// header. Empty when nothing's been written. Stays unredacted because the
/// review caller redacts at the bullet-line level.
fn read_daily_plan_description() -> String {
    crate::commands::memory::read_ai_insights_item("daily_plan")
        .map(|i| i.description)
        .unwrap_or_default()
}

/// Iter R16: read the description of `ai_insights/daily_review_YYYY-MM-DD`
/// for the given date. Returns the raw description (e.g.
/// "[review] 今天主动开口 7 次，计划 3/5") or None if the entry doesn't
/// exist. Caller (proactive turn) hands it to `format_yesterday_recap_hint`
/// to reframe past-tense for the prompt.
fn read_daily_review_description(date: chrono::NaiveDate) -> Option<String> {
    let title = format!("daily_review_{}", date);
    crate::commands::memory::read_ai_insights_item(&title).map(|i| i.description)
}

/// Iter R12: index-existence check for cross-process-restart idempotency.
/// LAST_DAILY_REVIEW_DATE only covers the current process; if the user
/// restarts the app at 23:00 after the 22:00 review already wrote, the
/// in-memory date is None and we'd otherwise re-fire. This catches that.
fn daily_review_exists(title: &str) -> bool {
    crate::commands::memory::read_ai_insights_item(title).is_some()
}

/// 早安简报：和 daily_review 的 ai_insights 标题同形（`morning_briefing_
/// YYYY-MM-DD`），用于跨进程重启时判定"今天是否已经发过早安"。
fn morning_briefing_exists(title: &str) -> bool {
    crate::commands::memory::read_ai_insights_item(title).is_some()
}

/// 早安简报触发器。门控通过后调用 LLM 生成一段早安播报，写入 speech_history、
/// 当前 session、ai_insights 标记，并通过 `proactive-message` 事件推送到前端
/// 气泡。与 daily_review 的等价：本函数承担所有 IO，纯门控 + 文本拼装在
/// `proactive/morning_briefing.rs`。
///
/// 与现有节奏控制层的关系（与文档 docs/20260504-1150-morning-briefing.md
/// 的「与 mute / 专注模式 / 主动发言冷却」节对齐）：
/// - 尊重 mute（用户主动按下"安静一会儿"时早安也跟着安静，免得打破期待）；
/// - 尊重 macOS Focus / 勿扰（仅当 settings.proactive.respect_focus_mode 开启时）；
/// - **绕过**主动发言冷却 — 早安自带"每日 1 次"语义，不参与一般 cooldown 节流；
///   触发后通过 `mark_proactive_spoken` 让常规 proactive 循环之后照常 cooldown。
///
/// 任何 IO 失败都不冒泡 — 早安是 best-effort 信号，失败时静默返回，下一 tick
/// 仍可重试（如未越过 grace 窗口）。
async fn maybe_run_morning_briefing(
    app: &AppHandle,
    settings: &crate::commands::settings::AppSettings,
    now_local: chrono::DateTime<chrono::Local>,
) -> Option<String> {
    let cfg = &settings.morning_briefing;
    let muted = mute_remaining_seconds().is_some();
    let focus_active = if settings.proactive.respect_focus_mode {
        crate::focus_mode::focus_status()
            .await
            .map(|s| s.active)
            .unwrap_or(false)
    } else {
        false
    };
    if morning_briefing_block_reason(
        cfg.enabled,
        muted,
        focus_active,
        settings.proactive.respect_focus_mode,
    )
    .is_some()
    {
        return None;
    }
    let now_naive = now_local.naive_local();
    let today = now_local.date_naive();
    let last = LAST_MORNING_BRIEFING_DATE.lock().ok().and_then(|g| *g);
    if !should_trigger_morning_briefing(
        now_naive,
        cfg.hour,
        cfg.minute,
        MORNING_BRIEFING_DEFAULT_GRACE_MINUTES,
        last,
    ) {
        return None;
    }
    let title = format!("morning_briefing_{}", today);
    if morning_briefing_exists(&title) {
        if let Ok(mut g) = LAST_MORNING_BRIEFING_DATE.lock() {
            *g = Some(today);
        }
        return None;
    }

    let log_store = app.state::<LogStore>().inner().clone();

    // 拼装 intent
    let yesterday = today.pred_opt().unwrap_or(today);
    let yesterday_excerpt = read_daily_review_description(yesterday);
    let mood_hint = read_current_mood_parsed()
        .map(|(t, _)| t)
        .filter(|t| !t.trim().is_empty());
    let intent = format_morning_briefing_intent(
        settings.user_name.trim(),
        yesterday_excerpt.as_deref(),
        mood_hint.as_deref(),
        today,
    );

    let config = match AiConfig::from_settings() {
        Ok(c) => c,
        Err(e) => {
            write_log(
                &log_store.0,
                &format!("MorningBriefing: AiConfig error: {}", e),
            );
            return None;
        }
    };
    let mcp_store = app.state::<McpManagerStore>().inner().clone();
    let shell_store = app.state::<ShellStore>().inner().clone();
    let process_counters = app
        .state::<crate::commands::debug::ProcessCountersStore>()
        .inner()
        .clone();
    let tool_review = app
        .state::<crate::tool_review::ToolReviewRegistryStore>()
        .inner()
        .clone();
    let decisions = app
        .state::<crate::decision_log::DecisionLogStore>()
        .inner()
        .clone();
    let ctx = ToolContext::new(log_store.clone(), shell_store, process_counters)
        .with_tool_review(tool_review)
        .with_decision_log(decisions.clone());

    let soul = get_soul().unwrap_or_default();
    let messages = vec![
        ChatMessage {
            role: "system".to_string(),
            content: serde_json::Value::String(soul),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        },
        ChatMessage {
            role: "user".to_string(),
            content: serde_json::Value::String(intent),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        },
    ];
    let sink = CollectingSink::new();
    let reply = match run_chat_pipeline(messages, &sink, &config, &mcp_store, &ctx).await {
        Ok(r) => r,
        Err(e) => {
            write_log(
                &log_store.0,
                &format!("MorningBriefing: chat pipeline error: {}", e),
            );
            return None;
        }
    };
    let reply_trimmed = reply.trim();
    if reply_trimmed.is_empty() || reply_trimmed.contains(SILENT_MARKER) {
        // 模型主动选择沉默时仍占用今天的"额度"，否则会在 grace 窗口内反复重试。
        write_log(&log_store.0, "MorningBriefing: pet returned empty / silent");
        if let Ok(mut g) = LAST_MORNING_BRIEFING_DATE.lock() {
            *g = Some(today);
        }
        return None;
    }

    // 持久化 + 推送给前端：与 run_proactive_turn 末段保持平行。session 落盘失败
    // 不致命（气泡仍会显示）— 仅对它做静默忽略。
    if let Some(id) = load_active_session().0 {
        let _ = persist_assistant_message(&id, reply_trimmed);
    }
    let clock = app.state::<InteractionClockStore>().inner().clone();
    clock.mark_proactive_spoken().await;
    crate::speech_history::record_speech(reply_trimmed).await;

    let description = format_morning_briefing_description(reply_trimmed);
    let _ = crate::commands::memory::memory_edit(
        "create".to_string(),
        "ai_insights".to_string(),
        title,
        Some(description),
        Some(reply_trimmed.to_string()),
    );

    let (mood_after, motion_after) = read_mood_for_event(&ctx, "MorningBriefing");
    if let Some(text) = &mood_after {
        crate::mood_history::record_mood(text, &motion_after).await;
    }
    let payload = ProactiveMessage {
        text: reply_trimmed.to_string(),
        timestamp: now_local.format("%Y-%m-%dT%H:%M:%S%.3f").to_string(),
        mood: mood_after,
        motion: motion_after,
    };
    let _ = app.emit("proactive-message", payload);

    if let Ok(mut g) = LAST_MORNING_BRIEFING_DATE.lock() {
        *g = Some(today);
    }
    decisions.push(
        "MorningBriefing",
        format!("{} chars", reply_trimmed.chars().count()),
    );

    Some(reply_trimmed.to_string())
}

/// Iter R12: gate + write the end-of-day review. Idempotent per day via
/// `LAST_DAILY_REVIEW_DATE` (in-process) + `daily_review_exists` (cross
/// restart). Speech lines are redacted + timestamp-stripped before writing.
/// Errors from `memory_edit` are swallowed — review is best-effort, not
/// load-bearing for the proactive turn that called it.
async fn maybe_run_daily_review(now_local: chrono::DateTime<chrono::Local>) {
    use chrono::Timelike;
    let today = now_local.date_naive();
    let hour = now_local.hour() as u8;
    let last = LAST_DAILY_REVIEW_DATE.lock().ok().and_then(|g| *g);
    if !should_trigger_daily_review(hour, today, last) {
        return;
    }
    let title = format!("daily_review_{}", today);
    if daily_review_exists(&title) {
        // Already on disk from a prior process — just mark in-memory and move on.
        if let Ok(mut g) = LAST_DAILY_REVIEW_DATE.lock() {
            *g = Some(today);
        }
        return;
    }
    // Cap at 100 entries — typical day is well under 30; bound just keeps the
    // review file size sane on pathological cases.
    let raw = crate::speech_history::speeches_for_date_async(today, 100).await;
    let lines: Vec<String> = raw
        .iter()
        .map(|line| {
            let stripped = crate::speech_history::strip_timestamp(line);
            crate::redaction::redact_with_settings(stripped)
        })
        .collect();
    let plan_raw = read_daily_plan_description();
    let detail = format_daily_review_detail(&lines, &plan_raw, today);
    // Iter R12b: pull `[N/M]` progress markers out of the plan into the
    // index description when parseable; falls back to "有计划" otherwise.
    let plan_progress = parse_plan_progress(&plan_raw);
    let description =
        format_daily_review_description(lines.len(), plan_progress, !plan_raw.trim().is_empty());
    let _ = crate::commands::memory::memory_edit(
        "create".to_string(),
        "ai_insights".to_string(),
        title,
        Some(description),
        Some(detail),
    );
    if let Ok(mut g) = LAST_DAILY_REVIEW_DATE.lock() {
        *g = Some(today);
    }
}

/// Read the pet's own short-term plan from `ai_insights/daily_plan`. Returns the plan
/// description verbatim with a header line, or empty when nothing's been written. The
/// plan format is intentionally open — the LLM owns the structure (bullet list with
/// progress markers like `[1/2]` is the suggested convention but not enforced).
fn build_plan_hint() -> String {
    let description = crate::commands::memory::read_ai_insights_item("daily_plan")
        .map(|i| i.description)
        .unwrap_or_default();
    format_plan_hint(&description, &|s| crate::redaction::redact_with_settings(s))
}

/// Load the most recent session's messages (without the proactive prompt). Returns
/// `(session_id, messages)` or `(None, [])` if none exists yet.
fn load_active_session() -> (Option<String>, Vec<serde_json::Value>) {
    let index = session::list_sessions();
    let Some(meta) = index.sessions.last().cloned() else {
        return (None, vec![]);
    };
    match session::load_session(meta.id.clone()) {
        Ok(s) => (Some(s.id), s.messages),
        Err(_) => (None, vec![]),
    }
}

/// Append an assistant turn to the active session file so the bubble + history reflect it.
fn persist_assistant_message(session_id: &str, text: &str) -> Result<(), String> {
    let mut sess = session::load_session(session_id.to_string())?;
    sess.messages
        .push(serde_json::json!({ "role": "assistant", "content": text }));
    sess.items
        .push(serde_json::json!({ "type": "assistant", "content": text }));
    sess.updated_at = chrono::Local::now()
        .format("%Y-%m-%dT%H:%M:%S%.3f")
        .to_string();
    session::save_session(sess)
}

#[cfg(test)]
mod prompt_tests {
    use super::*;
    use crate::mood::{MOOD_CATEGORY, MOOD_TITLE};

    fn base_inputs<'a>() -> PromptInputs<'a> {
        PromptInputs {
            time: "2026-05-03 14:30",
            period: "下午",
            // 2026-05-03 is a Sunday — matches the 周末 register tests assert.
            day_of_week: "周日 · 周末",
            idle_minutes: 20,
            // 20 min idle = 用户离开了一小会儿 (16-60 band) — keeps existing tests stable.
            idle_register: "用户离开了一小会儿",
            input_hint: "用户键鼠空闲约 60 秒。",
            cadence_hint: "距上次你主动开口约 8 分钟（刚说过话，话题还热）。",
            mood_hint: "你上次记录的心情/状态：「平静」。",
            focus_hint: "",
            wake_hint: "",
            speech_hint: "",
            is_first_mood: false,
            pre_quiet_minutes: None,
            reminders_hint: "",
            plan_hint: "",
            // Default well past the icebreaker threshold so existing tests stay at the
            // base 6 rule count; icebreaker tests bump this down explicitly.
            proactive_history_count: 100,
            // Default 1 = above the first-of-day trigger (Iter Cξ requires == 0)
            // and still below the default chatty_day_threshold of 5 — keeps existing
            // tests neutral on both rules. Tests for either bump this explicitly.
            today_speech_count: 1,
            // Mirrors the production default (5) — keeps existing tests stable while
            // letting chatty-day tests assert behavior at exact threshold boundary.
            chatty_day_threshold: 5,
            // Default 0/0 = below ENV_AWARENESS_MIN_SAMPLES → rule won't fire. Tests that
            // exercise the corrective rule bump these explicitly.
            env_spoke_total: 0,
            env_spoke_with_any: 0,
            // Default Some(8) — well below LONG_IDLE_MINUTES, matches the cadence_hint
            // string ("约 8 分钟（刚说过话，话题还热）"). Tests for long-idle bump this
            // above 60 explicitly.
            since_last_proactive_minutes: Some(8),
            // Default 5 — past day-0 special framing, but not a milestone (30 / 100 /
            // 365 / etc.) so the Iter Cρ companionship-milestone rule stays silent
            // by default. Tests that need either the day-0 framing or a milestone
            // set this explicitly.
            companionship_days: 5,
            // Default empty — base inputs simulate the pre-Iter 102 state where no
            // persona summary has been written yet. Tests for the persona hint set this
            // to a non-empty string to assert injection.
            persona_hint: "",
            // Default empty — pre-Iter 103 state, no mood trend yet. Tests for the
            // trend hint set this explicitly.
            mood_trend_hint: "",
            // Default empty — pre-Iter Cα state, no user_profile entries surfaced.
            // Tests for the user-profile hint set this explicitly.
            user_profile_hint: "",
            // Default empty — pre-Iter Cγ state, no owner-assigned butler tasks yet.
            // Tests for the butler_tasks hint set this explicitly.
            butler_tasks_hint: "",
            // 默认空：心跳测试需要时显式覆盖。
            task_heartbeat_hint: "",
            // Default empty — pre-Iter Cυ state, no owner name set in settings.
            // Tests for the user_name line set this explicitly.
            user_name: "",
            // Default empty — pre-Iter R1 state, no feedback log yet. Tests for
            // the feedback hint set this explicitly.
            feedback_hint: "",
            feedback_aggregate_hint: "",
            consecutive_silent_hint: "",
            consecutive_negative_hint: "",
            transient_note_hint: "",
            // Default 14 (afternoon) — well outside the late-night-wellness window
            // (hours 0..LATE_NIGHT_END_HOUR=4) so existing tests stay neutral.
            // Tests for the late-night rule set this to 0/1/2/3 explicitly.
            hour: 14,
            // Default false — pre-Iter R8 gate state. Tests for the rate limit
            // bump this to true to verify suppression.
            recently_fired_wellness: false,
            // Default empty — pre-Iter R11 state, no detected redundancy.
            // Tests for the topic-repeat rule set this explicitly.
            repeated_topic_hint: "",
            // Default empty — pre-Iter R14 state, no cross-day hint. Tests
            // exercising the first-of-day continuity layer set this explicitly.
            cross_day_hint: "",
            yesterday_recap_hint: "",
            length_register_hint: "",
            // Default empty — pre-Iter R15 state, no active-app duration tracker.
            active_app_hint: "",
            // Default empty — pre-Iter R63 state, no recent deep-focus block.
            deep_focus_recovery_hint: "",
            // Default empty — pre-Iter R66 state, no yesterday focus history.
            yesterday_focus_hint: "",
            // Default empty — pre-Iter R74 state, no personal record beaten today.
            personal_record_hint: "",
            // Default empty — pre-Iter R77 state, no deadline-prefixed butler tasks.
            deadline_hint: "",
        }
    }

    #[test]
    fn prompt_includes_required_sections() {
        let p = build_proactive_prompt(&base_inputs());
        assert!(p.starts_with("[系统提示·主动开口检查]"));
        assert!(p.contains("2026-05-03 14:30"));
        assert!(p.contains("下午"));
        assert!(p.contains("20 分钟"));
        assert!(p.contains("用户键鼠空闲约 60 秒"));
        assert!(p.contains("刚说过话"));
        assert!(p.contains("「平静」"));
        assert!(p.contains("约束："));
        assert!(p.contains("[motion: Tap]"));
    }

    #[test]
    fn empty_optional_hints_skip_their_lines() {
        let p = build_proactive_prompt(&base_inputs());
        // None of the conditional hint markers should appear when their inputs are blank.
        assert!(!p.contains("Focus 模式"));
        assert!(!p.contains("从休眠唤醒"));
        assert!(!p.contains("最近主动说过的几句话"));
        // And no leading/trailing/double blank from skipped sections — the focus point
        // is that join("\n") doesn't produce stray empty lines for skipped optionals.
        assert!(!p.contains("\n\n\n"));
    }

    #[test]
    fn focus_hint_renders_when_provided() {
        let mut inputs = base_inputs();
        inputs.focus_hint = "用户当前开着 macOS Focus 模式：「work」。";
        let p = build_proactive_prompt(&inputs);
        assert!(p.contains("「work」"));
    }

    #[test]
    fn wake_hint_renders_when_provided() {
        let mut inputs = base_inputs();
        inputs.wake_hint = "（用户的电脑在大约 60 秒前刚从休眠唤醒。）";
        let p = build_proactive_prompt(&inputs);
        assert!(p.contains("从休眠唤醒"));
    }

    #[test]
    fn speech_hint_with_bullets_passes_through_verbatim() {
        let mut inputs = base_inputs();
        let bullets =
            "你最近主动说过的几句话（旧→新），开口前看一眼避免重复：\n· 早上好啊\n· 加油码代码";
        inputs.speech_hint = bullets;
        let p = build_proactive_prompt(&inputs);
        assert!(p.contains("早上好啊"));
        assert!(p.contains("加油码代码"));
    }

    #[test]
    fn mood_category_and_title_interpolated() {
        let p = build_proactive_prompt(&base_inputs());
        assert!(p.contains(MOOD_CATEGORY));
        assert!(p.contains(MOOD_TITLE));
    }

    // ---- proactive_rules ----

    #[test]
    fn rules_count_and_format() {
        let rules = proactive_rules(&base_inputs());
        assert_eq!(rules.len(), 6, "ladder change → update count + add a test");
        // Every rule is a bullet starting with "- ".
        for r in &rules {
            assert!(r.starts_with("- "), "rule must be a bullet: {:?}", r);
        }
    }

    #[test]
    fn rules_interpolate_constants() {
        let rules = proactive_rules(&base_inputs());
        let joined = rules.join("\n");
        assert!(joined.contains(SILENT_MARKER));
        assert!(joined.contains(MOOD_CATEGORY));
        assert!(joined.contains(MOOD_TITLE));
        // The motion-tag enumeration is part of the last rule and must remain there.
        for tag in ["Tap", "Flick", "Flick3", "Idle"] {
            assert!(joined.contains(tag), "missing motion tag: {}", tag);
        }
    }

    #[test]
    fn rules_appear_in_full_prompt() {
        let p = build_proactive_prompt(&base_inputs());
        let rules = proactive_rules(&base_inputs());
        for r in &rules {
            assert!(p.contains(r.as_str()), "rule missing from prompt: {:?}", r);
        }
    }

    // ---- context-driven rule additions ----

    #[test]
    fn wake_rule_appears_when_wake_hint_present() {
        let mut inputs = base_inputs();
        inputs.wake_hint = "（用户的电脑在大约 60 秒前刚从休眠唤醒。）";
        let rules = proactive_rules(&inputs);
        assert_eq!(rules.len(), 7, "base 6 + 1 wake-context rule");
        assert!(rules.iter().any(|r| r.contains("用户刚从离开桌子回来")));
    }

    #[test]
    fn first_mood_rule_appears_when_flagged() {
        let mut inputs = base_inputs();
        inputs.is_first_mood = true;
        let rules = proactive_rules(&inputs);
        assert_eq!(rules.len(), 7, "base 6 + 1 first-mood rule");
        assert!(rules.iter().any(|r| r.contains("memory_edit create")));
    }

    #[test]
    fn both_context_rules_can_coexist() {
        let mut inputs = base_inputs();
        inputs.wake_hint = "唤醒提示";
        inputs.is_first_mood = true;
        let rules = proactive_rules(&inputs);
        assert_eq!(rules.len(), 8, "base 6 + 2 contextual rules");
    }

    #[test]
    fn no_context_rules_with_default_inputs() {
        let rules = proactive_rules(&base_inputs());
        assert_eq!(
            rules.len(),
            6,
            "no context-driven rules without their flags"
        );
    }

    #[test]
    fn reminders_rule_appears_when_hint_present() {
        let mut inputs = base_inputs();
        let bullet_text =
            "你有以下到期的用户提醒（请挑最相关的一条带进开口）：\n· 23:00 吃药（条目标题: meds）";
        inputs.reminders_hint = bullet_text;
        let rules = proactive_rules(&inputs);
        assert_eq!(rules.len(), 7, "base 6 + 1 reminders rule");
        assert!(rules.iter().any(|r| r.contains("memory_edit delete")));
    }

    #[test]
    fn plan_rule_appears_when_hint_present() {
        let mut inputs = base_inputs();
        inputs.plan_hint = "你今天的小目标 / 计划：\n· 关心用户工作进展 [0/2]";
        let rules = proactive_rules(&inputs);
        assert!(rules.iter().any(|r| r.contains("你有今日计划在执行中")));
    }

    #[test]
    fn icebreaker_rule_appears_when_count_under_three() {
        let mut inputs = base_inputs();
        inputs.proactive_history_count = 0;
        let rules = proactive_rules(&inputs);
        assert!(rules.iter().any(|r| r.contains("你和用户还不熟")));
        // Must include the actual count so the LLM has a sense of where on the curve.
        assert!(rules.iter().any(|r| r.contains("0 次")));
    }

    #[test]
    fn icebreaker_rule_absent_at_threshold() {
        let mut inputs = base_inputs();
        inputs.proactive_history_count = 3;
        let rules = proactive_rules(&inputs);
        assert!(!rules.iter().any(|r| r.contains("你和用户还不熟")));
    }

    #[test]
    fn chatty_day_rule_appears_at_or_above_threshold() {
        let mut inputs = base_inputs();
        inputs.today_speech_count = inputs.chatty_day_threshold;
        let rules = proactive_rules(&inputs);
        assert!(rules.iter().any(|r| r.contains("今天已经聊了不少")));
        // The actual count must surface so the LLM knows where on the curve it sits.
        let count_str = format!("{} 次", inputs.chatty_day_threshold);
        assert!(rules.iter().any(|r| r.contains(&count_str)));
    }

    #[test]
    fn chatty_day_rule_absent_below_threshold() {
        let mut inputs = base_inputs();
        inputs.today_speech_count = inputs.chatty_day_threshold - 1;
        let rules = proactive_rules(&inputs);
        assert!(!rules.iter().any(|r| r.contains("今天已经聊了不少")));
    }

    #[test]
    fn chatty_day_rule_disabled_when_threshold_zero() {
        // threshold == 0 means user opted out of the chatty-day nudge entirely; the rule
        // must not fire even when today_speech_count is sky-high.
        let mut inputs = base_inputs();
        inputs.chatty_day_threshold = 0;
        inputs.today_speech_count = 9999;
        let rules = proactive_rules(&inputs);
        assert!(!rules.iter().any(|r| r.contains("今天已经聊了不少")));
    }

    #[test]
    fn env_awareness_low_below_min_samples_returns_false() {
        // Even 0/9 (which would be 0%) doesn't fire because we don't trust the ratio yet.
        assert!(!env_awareness_low(9, 0));
        assert!(!env_awareness_low(ENV_AWARENESS_MIN_SAMPLES - 1, 0));
    }

    #[test]
    fn env_awareness_low_at_threshold_strict_inequality() {
        // Exactly 30% (3/10) → not low; just under (2/10 = 20%) → low.
        assert!(!env_awareness_low(10, 3));
        assert!(env_awareness_low(10, 2));
    }

    #[test]
    fn env_awareness_low_at_full_coverage_returns_false() {
        // 100% coverage definitely shouldn't trigger the prod.
        assert!(!env_awareness_low(20, 20));
    }

    #[test]
    fn env_awareness_corrective_rule_appears_when_low() {
        let mut inputs = base_inputs();
        inputs.env_spoke_total = 12;
        inputs.env_spoke_with_any = 2; // ~16%
        let rules = proactive_rules(&inputs);
        assert!(rules
            .iter()
            .any(|r| r.contains("最近你开口前几乎都没看环境")));
        // Includes the actual numbers + threshold so the LLM has concrete signal.
        assert!(rules.iter().any(|r| r.contains("12 次")));
        assert!(rules.iter().any(|r| r.contains("get_active_window")));
    }

    #[test]
    fn active_data_driven_rule_labels_empty_in_neutral_state() {
        // Past icebreaker, below chatty threshold, no env-awareness data.
        assert!(active_data_driven_rule_labels(100, 0, 5, 0, 0, 0).is_empty());
    }

    #[test]
    fn active_data_driven_rule_labels_picks_up_each_rule_independently() {
        // Only icebreaker.
        assert_eq!(
            active_data_driven_rule_labels(0, 0, 5, 0, 0, 0),
            vec!["icebreaker"],
        );
        // Only chatty.
        assert_eq!(
            active_data_driven_rule_labels(100, 5, 5, 0, 0, 0),
            vec!["chatty"],
        );
        // Only env-awareness.
        assert_eq!(
            active_data_driven_rule_labels(100, 0, 5, 12, 2, 0),
            vec!["env-awareness"],
        );
    }

    #[test]
    fn active_data_driven_rule_labels_combine_in_firing_order() {
        // All three at once: should appear in the same order proactive_rules pushes them.
        let labels = active_data_driven_rule_labels(0, 6, 5, 12, 1, 0);
        assert_eq!(labels, vec!["icebreaker", "chatty", "env-awareness"]);
    }

    #[test]
    fn active_data_driven_rule_labels_zero_threshold_disables_chatty() {
        // chatty threshold == 0 means the user opted out — even today_count=99 shouldn't
        // surface the chatty label.
        assert!(active_data_driven_rule_labels(100, 99, 0, 0, 0, 0).is_empty());
    }

    #[test]
    fn active_environmental_rule_labels_empty_when_all_false() {
        assert!(
            active_environmental_rule_labels(false, false, false, false, false, false).is_empty()
        );
    }

    #[test]
    fn active_environmental_rule_labels_picks_each_independently() {
        assert_eq!(
            active_environmental_rule_labels(true, false, false, false, false, false),
            vec!["wake-back"],
        );
        assert_eq!(
            active_environmental_rule_labels(false, true, false, false, false, false),
            vec!["first-mood"],
        );
        assert_eq!(
            active_environmental_rule_labels(false, false, true, false, false, false),
            vec!["pre-quiet"],
        );
        assert_eq!(
            active_environmental_rule_labels(false, false, false, true, false, false),
            vec!["reminders"],
        );
        assert_eq!(
            active_environmental_rule_labels(false, false, false, false, true, false),
            vec!["plan"],
        );
        assert_eq!(
            active_environmental_rule_labels(false, false, false, false, false, true),
            vec!["first-of-day"],
        );
    }

    #[test]
    fn companionship_milestone_fixed_thresholds() {
        assert_eq!(companionship_milestone(7), Some("刚好一周"));
        assert_eq!(companionship_milestone(30), Some("满一个月"));
        assert_eq!(companionship_milestone(100), Some("百日纪念"));
        assert_eq!(companionship_milestone(180), Some("满半年"));
        assert_eq!(companionship_milestone(365), Some("满一年"));
    }

    #[test]
    fn companionship_milestone_returns_none_for_non_milestones() {
        assert_eq!(companionship_milestone(0), None);
        assert_eq!(companionship_milestone(1), None);
        assert_eq!(companionship_milestone(6), None);
        assert_eq!(companionship_milestone(8), None);
        assert_eq!(companionship_milestone(29), None);
        assert_eq!(companionship_milestone(31), None);
        assert_eq!(companionship_milestone(99), None);
        assert_eq!(companionship_milestone(364), None);
        assert_eq!(companionship_milestone(366), None);
    }

    #[test]
    fn companionship_milestone_yearly_after_first_year() {
        assert_eq!(companionship_milestone(730), Some("又一个周年"));
        assert_eq!(companionship_milestone(1095), Some("又一个周年"));
        assert_eq!(companionship_milestone(1460), Some("又一个周年"));
        // Off-by-one days near anniversaries should not fire.
        assert_eq!(companionship_milestone(729), None);
        assert_eq!(companionship_milestone(731), None);
    }

    #[test]
    fn companionship_milestone_rule_fires_through_proactive_rules() {
        let mut inputs = base_inputs();
        inputs.companionship_days = 100;
        let rules = proactive_rules(&inputs);
        assert!(rules.iter().any(|r| r.contains("百日纪念")));
        assert!(rules.iter().any(|r| r.contains("今天是和用户相处的")));

        inputs.companionship_days = 5; // not a milestone
        let rules = proactive_rules(&inputs);
        assert!(!rules.iter().any(|r| r.contains("今天是和用户相处的")));
    }

    #[test]
    fn effective_awaiting_clears_when_raw_false() {
        // Raw flag false → always false regardless of timing.
        assert!(!effective_awaiting(false, Some(0)));
        assert!(!effective_awaiting(false, Some(99999)));
        assert!(!effective_awaiting(false, None));
    }

    #[test]
    fn effective_awaiting_honors_recent_raw() {
        // Raw flag true and pet spoke recently → gate fires.
        assert!(effective_awaiting(true, Some(0)));
        assert!(effective_awaiting(true, Some(60)));
        assert!(effective_awaiting(
            true,
            Some(AWAITING_AUTO_CLEAR_SECONDS - 1)
        ));
    }

    #[test]
    fn effective_awaiting_expires_after_threshold() {
        // Raw flag true but pet spoke long ago → gate falls off.
        assert!(!effective_awaiting(true, Some(AWAITING_AUTO_CLEAR_SECONDS)));
        assert!(!effective_awaiting(
            true,
            Some(AWAITING_AUTO_CLEAR_SECONDS + 1)
        ));
        assert!(!effective_awaiting(true, Some(99999)));
    }

    #[test]
    fn effective_awaiting_none_since_proactive_does_not_fire() {
        // Defensive: raw flag set but no recorded last_proactive — treat as
        // "shouldn't be awaiting" (mark_proactive_spoken sets both atomically,
        // so this state shouldn't happen, but we don't panic if it does).
        assert!(!effective_awaiting(true, None));
    }

    #[test]
    fn prompt_includes_user_name_line_when_set() {
        // Iter Cυ: when settings.user_name is non-empty, the proactive prompt
        // gets a "你的主人是「X」" line right after the companionship line so
        // the LLM can address the owner by name.
        let mut inputs = base_inputs();
        inputs.user_name = "moon";
        let p = build_proactive_prompt(&inputs);
        assert!(p.contains("你的主人是「moon」"));
        // Ordering: companionship line precedes user_name line precedes the
        // optional persona/profile blocks (those are empty in base_inputs).
        let p_companion = p.find("一起走过").unwrap();
        let p_user = p.find("你的主人是").unwrap();
        assert!(p_companion < p_user, "companionship before user_name");
    }

    #[test]
    fn prompt_omits_user_name_line_when_empty_or_whitespace() {
        let inputs = base_inputs(); // default user_name = ""
        let p = build_proactive_prompt(&inputs);
        assert!(!p.contains("你的主人是"));

        let mut inputs2 = base_inputs();
        inputs2.user_name = "   \t  ";
        let p2 = build_proactive_prompt(&inputs2);
        assert!(
            !p2.contains("你的主人是"),
            "whitespace-only must be skipped"
        );
    }

    #[test]
    fn prompt_trims_user_name_whitespace() {
        let mut inputs = base_inputs();
        inputs.user_name = "  moon  ";
        let p = build_proactive_prompt(&inputs);
        assert!(p.contains("你的主人是「moon」"));
        assert!(!p.contains("「  moon"));
    }

    #[test]
    fn first_of_day_only_fires_when_today_count_is_zero() {
        // Iter Cξ: first-of-day is wired in proactive_rules from
        // `inputs.today_speech_count == 0`. Pin the integration here so a future
        // refactor (e.g., changing how the env_labels callsite reads the count)
        // doesn't silently break the trigger.
        let mut inputs = base_inputs();
        inputs.today_speech_count = 0;
        let rules_zero = proactive_rules(&inputs);
        assert!(rules_zero.iter().any(|r| r.contains("今天的第一次开口")));

        inputs.today_speech_count = 1;
        let rules_one = proactive_rules(&inputs);
        assert!(!rules_one.iter().any(|r| r.contains("今天的第一次开口")));
    }

    #[test]
    fn active_environmental_rule_labels_combine_in_firing_order() {
        let labels = active_environmental_rule_labels(true, true, true, true, true, true);
        // Iter Cξ: first-of-day slots between first-mood and pre-quiet so the
        // greeting register comes after mood bootstrap but before quiet-hours
        // wind-down.
        assert_eq!(
            labels,
            vec![
                "wake-back",
                "first-mood",
                "first-of-day",
                "pre-quiet",
                "reminders",
                "plan"
            ],
        );
    }

    #[test]
    fn format_companionship_line_day_zero_uses_first_day_framing() {
        let line = format_companionship_line(0);
        assert!(line.contains("第一天"));
        assert!(line.contains("初识"));
    }

    #[test]
    fn format_companionship_line_after_day_zero_states_count() {
        let line = format_companionship_line(42);
        assert!(line.contains("42 天"));
        // Mentions familiarity / shared time so the LLM is invited to use it.
        assert!(line.contains("相处时长"));
    }

    #[test]
    fn prompt_includes_companionship_line() {
        let mut inputs = base_inputs();
        inputs.companionship_days = 7;
        let p = build_proactive_prompt(&inputs);
        assert!(p.contains("7 天"));
    }

    #[test]
    fn prompt_includes_first_day_framing_at_day_zero() {
        let mut inputs = base_inputs();
        inputs.companionship_days = 0;
        let p = build_proactive_prompt(&inputs);
        assert!(p.contains("第一天"));
    }

    #[test]
    fn prompt_includes_persona_hint_when_set() {
        let mut inputs = base_inputs();
        inputs.persona_hint =
            "你最近一次自我反思的画像（来自 consolidate）：\n我倾向短句，话题偏当下场景。";
        let p = build_proactive_prompt(&inputs);
        assert!(p.contains("自我反思的画像"));
        assert!(p.contains("我倾向短句"));
    }

    #[test]
    fn prompt_omits_persona_hint_when_empty() {
        let inputs = base_inputs(); // default persona_hint = ""
        let p = build_proactive_prompt(&inputs);
        assert!(!p.contains("自我反思的画像"));
    }

    #[test]
    fn prompt_includes_mood_trend_hint_when_set() {
        let mut inputs = base_inputs();
        inputs.mood_trend_hint =
            "你最近 30 次心情记录里：Tap × 12、Idle × 10、Flick × 5（按出现次数排序）。这是你长期的情绪谱——可以让 ta 渗进当下语气，但不必生硬带出。";
        let p = build_proactive_prompt(&inputs);
        assert!(p.contains("长期的情绪谱"));
        assert!(p.contains("Tap × 12"));
    }

    #[test]
    fn prompt_omits_mood_trend_hint_when_empty() {
        let inputs = base_inputs(); // default mood_trend_hint = ""
        let p = build_proactive_prompt(&inputs);
        assert!(!p.contains("长期的情绪谱"));
    }

    #[test]
    fn prompt_includes_user_profile_hint_when_set() {
        let mut inputs = base_inputs();
        inputs.user_profile_hint =
            "你了解的用户习惯（来自 user_profile 记忆，最新 2 条）：\n- 起床时间：通常 8:30 起床\n- 喜欢：偏好 dark theme 编辑器";
        let p = build_proactive_prompt(&inputs);
        assert!(p.contains("用户习惯"));
        assert!(p.contains("起床时间"));
        assert!(p.contains("dark theme"));
    }

    #[test]
    fn prompt_omits_user_profile_hint_when_empty() {
        let inputs = base_inputs(); // default user_profile_hint = ""
        let p = build_proactive_prompt(&inputs);
        assert!(!p.contains("用户习惯"));
    }

    #[test]
    fn format_user_profile_block_empty_returns_empty() {
        assert_eq!(
            format_user_profile_block(&[], 6, 80),
            String::new(),
            "no items → no block",
        );
    }

    #[test]
    fn format_user_profile_block_zero_max_returns_empty() {
        let items = vec![(
            "habit".into(),
            "desc".into(),
            "2026-05-03T10:00:00+08:00".into(),
        )];
        assert_eq!(format_user_profile_block(&items, 0, 80), String::new());
    }

    #[test]
    fn format_user_profile_block_sorts_by_updated_at_desc() {
        let items = vec![
            (
                "旧习惯".into(),
                "desc-a".into(),
                "2026-04-01T10:00:00+08:00".into(),
            ),
            (
                "新习惯".into(),
                "desc-b".into(),
                "2026-05-03T10:00:00+08:00".into(),
            ),
            (
                "中习惯".into(),
                "desc-c".into(),
                "2026-04-20T10:00:00+08:00".into(),
            ),
        ];
        let out = format_user_profile_block(&items, 6, 80);
        let new_idx = out.find("新习惯").unwrap();
        let mid_idx = out.find("中习惯").unwrap();
        let old_idx = out.find("旧习惯").unwrap();
        assert!(new_idx < mid_idx, "newest should be first");
        assert!(mid_idx < old_idx, "middle should precede oldest");
    }

    #[test]
    fn format_user_profile_block_caps_item_count() {
        let items: Vec<(String, String, String)> = (0..10)
            .map(|i| {
                (
                    format!("habit-{i}"),
                    format!("desc-{i}"),
                    format!("2026-05-{:02}T10:00:00+08:00", i + 1),
                )
            })
            .collect();
        let out = format_user_profile_block(&items, 3, 80);
        assert!(out.contains("最新 3 条"));
        // Top-3 by date = day 10, 9, 8 (i = 9, 8, 7).
        assert!(out.contains("habit-9"));
        assert!(out.contains("habit-8"));
        assert!(out.contains("habit-7"));
        assert!(!out.contains("habit-6"), "4th-newest should be excluded");
    }

    #[test]
    fn format_user_profile_block_truncates_long_descriptions() {
        let long_desc: String = "字".repeat(120);
        let items = vec![(
            "habit".into(),
            long_desc,
            "2026-05-03T10:00:00+08:00".into(),
        )];
        let out = format_user_profile_block(&items, 6, 20);
        assert!(out.contains("…"), "should append ellipsis when truncated");
        // Body line should be 1 title + ：+ 20 chars + … = well under the 120.
        let body_chars = out
            .lines()
            .nth(1)
            .unwrap()
            .chars()
            .filter(|c| *c == '字')
            .count();
        assert_eq!(
            body_chars, 20,
            "should keep exactly 20 chars before ellipsis"
        );
    }

    #[test]
    fn prompt_includes_butler_tasks_hint_when_set() {
        let mut inputs = base_inputs();
        inputs.butler_tasks_hint =
            "用户委托给你的管家任务（共 1 条，按最早委托排在前）：\n- 早报：每天 8 点把日历发给我";
        let p = build_proactive_prompt(&inputs);
        assert!(p.contains("管家任务"));
        assert!(p.contains("早报"));
        assert!(p.contains("把日历发给我"));
    }

    #[test]
    fn prompt_omits_butler_tasks_hint_when_empty() {
        let inputs = base_inputs();
        let p = build_proactive_prompt(&inputs);
        assert!(!p.contains("管家任务"));
    }

    #[test]
    fn prompt_includes_day_of_week_in_time_line() {
        let mut inputs = base_inputs();
        inputs.day_of_week = "周一 · 工作日";
        let p = build_proactive_prompt(&inputs);
        // Time line is the first non-header line; assert the new label is right
        // after the period.
        assert!(p.contains("（下午，周一 · 工作日）"));
    }

    // user_absence_tier tests moved to proactive/time_helpers.rs (Iter QG5c-prep).

    #[test]
    fn prompt_includes_idle_register_in_time_line() {
        let mut inputs = base_inputs();
        inputs.idle_register = "用户走开有一两小时了";
        inputs.idle_minutes = 90;
        let p = build_proactive_prompt(&inputs);
        // The register sits inside the parenthetical right after the minute count.
        assert!(p.contains("约 90 分钟（用户走开有一两小时了）"));
    }

    // weekday_zh / weekday_kind_zh / format_day_of_week_hint tests moved to
    // proactive/time_helpers.rs (Iter QG5c-prep).

    #[test]
    fn format_user_profile_block_no_truncation_when_under_cap() {
        let items = vec![(
            "habit".into(),
            "短描述".into(),
            "2026-05-03T10:00:00+08:00".into(),
        )];
        let out = format_user_profile_block(&items, 6, 80);
        assert!(out.contains("- habit：短描述"));
        assert!(!out.contains("…"));
    }

    #[test]
    fn active_composite_rule_labels_engagement_window_requires_both_signals() {
        // engagement-window: wake_back AND has_plan.
        // long-idle-no-restraint: long_idle (None == long-idle) AND under_chatty AND !pre_quiet.
        // Default for long-idle inputs: Some(8) (short idle), today=0, threshold=5, pre_quiet=false.
        // idle_min = 0 → long-absence-reunion silent throughout this test.
        // hour = 14 (afternoon) → late-night-wellness silent throughout.
        let short_idle = Some(8u64);
        assert!(
            active_composite_rule_labels(false, false, short_idle, 0, 5, false, 0, 14, false)
                .is_empty()
        );
        assert!(
            active_composite_rule_labels(true, false, short_idle, 0, 5, false, 0, 14, false)
                .is_empty()
        );
        assert!(
            active_composite_rule_labels(false, true, short_idle, 0, 5, false, 0, 14, false)
                .is_empty()
        );
        assert_eq!(
            active_composite_rule_labels(true, true, short_idle, 0, 5, false, 0, 14, false),
            vec!["engagement-window"],
        );
    }

    #[test]
    fn active_composite_rule_labels_long_idle_requires_three_signals() {
        // hour = 14 (afternoon) keeps late-night-wellness silent throughout.
        // long_idle yes, under_chatty yes, !pre_quiet yes → fires.
        assert_eq!(
            active_composite_rule_labels(
                false,
                false,
                Some(LONG_IDLE_MINUTES),
                0,
                5,
                false,
                0,
                14,
                false
            ),
            vec!["long-idle-no-restraint"],
        );
        // None (never spoken) is treated as long-idle.
        assert_eq!(
            active_composite_rule_labels(false, false, None, 0, 5, false, 0, 14, false),
            vec!["long-idle-no-restraint"],
        );
        // Short idle (< threshold) → no fire.
        assert!(active_composite_rule_labels(
            false,
            false,
            Some(LONG_IDLE_MINUTES - 1),
            0,
            5,
            false,
            0,
            14,
            false,
        )
        .is_empty());
        // chatty (today >= threshold) → no fire.
        assert!(
            active_composite_rule_labels(false, false, Some(120), 5, 5, false, 0, 14, false)
                .is_empty()
        );
        // pre_quiet active → no fire.
        assert!(
            active_composite_rule_labels(false, false, Some(120), 0, 5, true, 0, 14, false)
                .is_empty()
        );
        // Threshold == 0 disables chatty gate, so under_chatty is always true.
        assert_eq!(
            active_composite_rule_labels(false, false, Some(120), 9999, 0, false, 0, 14, false),
            vec!["long-idle-no-restraint"],
        );
    }

    #[test]
    fn active_composite_rule_labels_both_can_fire_together() {
        // wake_back + has_plan + long_idle + under_chatty + !pre_quiet → both labels.
        // hour = 14 keeps late-night-wellness silent.
        let labels = active_composite_rule_labels(
            true,
            true,
            Some(LONG_IDLE_MINUTES),
            0,
            5,
            false,
            0,
            14,
            false,
        );
        assert_eq!(labels, vec!["engagement-window", "long-idle-no-restraint"]);
    }

    #[test]
    fn active_composite_rule_labels_long_absence_reunion_gates() {
        // hour = 14 (afternoon) → late-night-wellness silent throughout. idle is
        // already large enough that the late-night idle<5 gate would never fire.
        // Threshold boundary — at threshold + under_chatty + !pre_quiet → fires.
        assert_eq!(
            active_composite_rule_labels(
                false,
                false,
                Some(8),
                0,
                5,
                false,
                LONG_ABSENCE_MINUTES,
                14,
                false,
            ),
            vec!["long-absence-reunion"],
        );
        // Just below threshold → silent.
        assert!(active_composite_rule_labels(
            false,
            false,
            Some(8),
            0,
            5,
            false,
            LONG_ABSENCE_MINUTES - 1,
            14,
            false,
        )
        .is_empty());
        // chatty active → silent (don't pile reunion onto a chatty day).
        assert!(active_composite_rule_labels(
            false,
            false,
            Some(8),
            5,
            5,
            false,
            LONG_ABSENCE_MINUTES + 60,
            14,
            false,
        )
        .is_empty());
        // pre_quiet active → silent (don't open new register before quiet hours).
        assert!(active_composite_rule_labels(
            false,
            false,
            Some(8),
            0,
            5,
            true,
            LONG_ABSENCE_MINUTES + 60,
            14,
            false,
        )
        .is_empty());
    }

    // -- Iter R83: extreme-absence-reunion mutual-exclusion -----------------

    #[test]
    fn active_composite_rule_labels_extreme_absence_reunion_at_24h_threshold() {
        // At EXTREME_ABSENCE_MINUTES exactly → extreme fires, long is suppressed.
        let labels = active_composite_rule_labels(
            false,
            false,
            Some(8),
            0,
            5,
            false,
            EXTREME_ABSENCE_MINUTES,
            14,
            false,
        );
        assert_eq!(labels, vec!["extreme-absence-reunion"]);
        assert!(!labels.contains(&"long-absence-reunion"));
    }

    #[test]
    fn active_composite_rule_labels_extreme_absence_reunion_well_past_24h() {
        // Days-long absence still resolves to extreme, not duplicated.
        let labels = active_composite_rule_labels(
            false,
            false,
            Some(8),
            0,
            5,
            false,
            EXTREME_ABSENCE_MINUTES * 3,
            14,
            false,
        );
        assert_eq!(labels, vec!["extreme-absence-reunion"]);
    }

    #[test]
    fn active_composite_rule_labels_just_below_24h_stays_long_absence() {
        // EXTREME_ABSENCE_MINUTES - 1 → long fires, not extreme. Boundary
        // pinned the same way Cν's threshold test pins LONG_ABSENCE_MINUTES.
        let labels = active_composite_rule_labels(
            false,
            false,
            Some(8),
            0,
            5,
            false,
            EXTREME_ABSENCE_MINUTES - 1,
            14,
            false,
        );
        assert_eq!(labels, vec!["long-absence-reunion"]);
    }

    #[test]
    fn active_composite_rule_labels_extreme_absence_still_gated_by_chatty() {
        // Same gates as Cν: chatty active → both reunion variants silent.
        assert!(active_composite_rule_labels(
            false,
            false,
            Some(8),
            5,
            5,
            false,
            EXTREME_ABSENCE_MINUTES + 60,
            14,
            false,
        )
        .is_empty());
    }

    #[test]
    fn active_composite_rule_labels_late_night_wellness_gating() {
        // Iter R3: late-night-wellness fires iff hour < LATE_NIGHT_END_HOUR (4)
        // AND idle_minutes < LATE_NIGHT_ACTIVE_MAX_IDLE_MIN (5).
        // Pure name → no other signal interferes; pass neutral defaults.
        // Hours 0,1,2,3 with idle=0 should fire.
        for h in 0u8..LATE_NIGHT_END_HOUR {
            let labels =
                active_composite_rule_labels(false, false, Some(8), 0, 5, false, 0, h, false);
            assert!(
                labels.contains(&"late-night-wellness"),
                "hour={} idle=0 should trigger late-night-wellness",
                h,
            );
        }
        // Hour 4 (boundary off) → silent.
        let labels = active_composite_rule_labels(
            false,
            false,
            Some(8),
            0,
            5,
            false,
            0,
            LATE_NIGHT_END_HOUR,
            false,
        );
        assert!(!labels.contains(&"late-night-wellness"));
        // hour=2 but idle=5 (boundary off) → silent.
        let labels = active_composite_rule_labels(
            false,
            false,
            Some(8),
            0,
            5,
            false,
            LATE_NIGHT_ACTIVE_MAX_IDLE_MIN,
            2,
            false,
        );
        assert!(!labels.contains(&"late-night-wellness"));
        // hour=2 + idle=4 (in band) → fires regardless of chatty / pre_quiet.
        // This rule deliberately bypasses both gates because user health > cadence.
        let labels = active_composite_rule_labels(false, false, Some(8), 999, 5, true, 4, 2, false);
        assert!(labels.contains(&"late-night-wellness"));
    }

    #[test]
    fn late_night_wellness_recently_fired_at_gates_window() {
        // Iter R8: pure decider for the rate-limit window. None last → never recent;
        // last < gap → recent (suppress); last == gap → not recent (allow);
        // last > gap → not recent.
        let now = std::time::Instant::now();
        assert!(
            !late_night_wellness_recently_fired_at(None, now, 1800),
            "no prior fire = always allow"
        );
        let recent = now - std::time::Duration::from_secs(900); // 15 min ago
        assert!(
            late_night_wellness_recently_fired_at(Some(recent), now, 1800),
            "15 min < 30 min gap → suppress"
        );
        let exactly_at_gap = now - std::time::Duration::from_secs(1800);
        assert!(
            !late_night_wellness_recently_fired_at(Some(exactly_at_gap), now, 1800),
            "boundary off — exactly at gap is allowed (cooldown elapsed)"
        );
        let beyond_gap = now - std::time::Duration::from_secs(3600);
        assert!(
            !late_night_wellness_recently_fired_at(Some(beyond_gap), now, 1800),
            "1h > 30 min → cooldown elapsed"
        );
    }

    #[test]
    fn active_composite_rule_labels_late_night_wellness_suppressed_in_cooldown() {
        // Iter R8: when the rate limit is in effect, the rule must not fire even
        // though hour + idle satisfy the trigger.
        let labels = active_composite_rule_labels(false, false, Some(8), 0, 5, false, 0, 2, true);
        assert!(
            !labels.contains(&"late-night-wellness"),
            "recently_fired_wellness=true should suppress"
        );
    }

    #[test]
    fn active_composite_rule_labels_three_can_coexist() {
        // wake_back + has_plan + long_idle + under_chatty + !pre_quiet + long_absence
        // → all three labels emit in order: engagement-window, long-idle-no-restraint,
        //   long-absence-reunion.
        let labels = active_composite_rule_labels(
            true,
            true,
            Some(LONG_IDLE_MINUTES),
            0,
            5,
            false,
            LONG_ABSENCE_MINUTES + 60,
            14,
            false,
        );
        assert_eq!(
            labels,
            vec![
                "engagement-window",
                "long-idle-no-restraint",
                "long-absence-reunion"
            ],
        );
    }

    /// Read panelTypes.ts and return the kebab/snake keys defined in
    /// PROMPT_RULE_DESCRIPTIONS. Used by both alignment tests so the parsing logic
    /// only lives in one place. Plain string scanning — no regex dep — and tolerant
    /// of both quoted (`"wake-back": {`) and bare-identifier (`plan: {`) keys.
    ///
    /// Iter 98: relocated from PanelDebug.tsx to panelTypes.ts so PanelDebug owns only
    /// state + layout. If the dict moves again, update this path and the panic
    /// messages below in lockstep.
    fn parse_prompt_rule_dict_keys() -> Vec<String> {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
            .expect("CARGO_MANIFEST_DIR is set during cargo test runs");
        let panel_path = std::path::PathBuf::from(manifest_dir)
            .join("..")
            .join("src")
            .join("components")
            .join("panel")
            .join("panelTypes.ts");
        let panel_src = std::fs::read_to_string(&panel_path)
            .unwrap_or_else(|e| panic!("read panelTypes.ts: {}", e));
        let mut keys: Vec<String> = Vec::new();
        let mut in_dict = false;
        for line in panel_src.lines() {
            // Accept both `const ...` and `export const ...` forms — Iter 97 made the
            // dict an export so a sibling component can import it.
            if line.starts_with("const PROMPT_RULE_DESCRIPTIONS")
                || line.starts_with("export const PROMPT_RULE_DESCRIPTIONS")
            {
                in_dict = true;
                continue;
            }
            if !in_dict {
                continue;
            }
            let trimmed = line.trim();
            if trimmed == "};" {
                break;
            }
            // Each top-level entry starts with `<key>: {` (key value is itself a {…}
            // object literal). Inner lines use `title: "..."` / `summary: "..."` and
            // don't match `: {` so they're naturally skipped.
            if let Some(idx) = trimmed.find(": {") {
                let raw = trimmed[..idx].trim().trim_matches('"');
                if !raw.is_empty()
                    && raw
                        .chars()
                        .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
                {
                    keys.push(raw.to_string());
                }
            }
        }
        keys
    }

    #[test]
    fn proactive_rules_has_match_arm_for_every_backend_label() {
        // Iter 91 / Iter 93 — guard the proactive_rules match against the helper label
        // set. If a label ships from a helper but proactive_rules has no arm for it,
        // the fallback `(规则文本待补)` slips into the prompt — visible in panel logs
        // but easy to miss in CI. This test fails immediately instead.
        //
        // Some labels are mutually exclusive (chatty vs long-idle-no-restraint share the
        // chatty threshold; pre-quiet excludes long-idle). We run two scenarios and
        // require each fingerprint to fire in at least one of them — combined coverage
        // still pins down the full match arm set.
        let fingerprints: &[(&str, &str)] = &[
            ("wake-back", "用户刚从离开桌子回来"),
            ("first-mood", "第一次开口"),
            ("first-of-day", "今天的第一次开口"),
            ("pre-quiet", "快进入安静时段"),
            ("reminders", "有到期的用户提醒"),
            ("plan", "你有今日计划在执行中"),
            ("icebreaker", "你和用户还不熟"),
            ("chatty", "今天已经聊了不少"),
            ("companionship-milestone", "今天是和用户相处的"),
            ("env-awareness", "最近你开口前几乎都没看环境"),
            ("engagement-window", "此刻是开新话题的好时机"),
            ("long-idle-no-restraint", "沉默已久但没在克制状态"),
            ("long-absence-reunion", "用户离开了不短的时间"),
            ("late-night-wellness", "深夜还在用电脑"),
        ];

        // Sanity: the fingerprint table must cover every label the helpers emit. Use
        // each helper's max-trigger inputs to enumerate the universe.
        // Two composite calls: one with high idle (long-absence-reunion fires),
        // one with low idle + hour=2 (late-night-wellness fires) — labels conflict
        // on idle_minutes / hour so we union the two outputs to get the universe.
        let backend_labels: std::collections::HashSet<&'static str> =
            active_environmental_rule_labels(true, true, true, true, true, true)
                .into_iter()
                .chain(active_data_driven_rule_labels(0, 999, 1, 999, 0, 100))
                .chain(active_composite_rule_labels(
                    true,
                    true,
                    Some(120),
                    0,
                    5,
                    false,
                    LONG_ABSENCE_MINUTES + 60,
                    14,
                    false,
                ))
                .chain(active_composite_rule_labels(
                    false, false, None, 0, 5, false, 0, 2, false,
                ))
                .collect();
        let fingerprint_labels: std::collections::HashSet<&str> =
            fingerprints.iter().map(|(l, _)| *l).collect();
        let untested: Vec<&&'static str> = backend_labels
            .iter()
            .filter(|l| !fingerprint_labels.contains(*l))
            .collect();
        assert!(
            untested.is_empty(),
            "fingerprint table missing entries for backend labels: {:?}. Add a row \
             with a substring unique to that label's rule text.",
            untested
        );

        // Scenario 1: short-idle + chatty active + pre-quiet active.
        // Covers: wake-back, first-mood, pre-quiet, reminders, plan, icebreaker, chatty,
        // env-awareness, engagement-window, companionship-milestone.
        let mut s1 = base_inputs();
        s1.wake_hint = "（用户的电脑在大约 60 秒前刚从休眠唤醒。）";
        s1.is_first_mood = true;
        s1.pre_quiet_minutes = Some(10);
        s1.reminders_hint = "你有以下到期的用户提醒：\n· something";
        s1.plan_hint = "你今天的小目标：\n· something";
        s1.proactive_history_count = 0;
        s1.today_speech_count = s1.chatty_day_threshold;
        s1.env_spoke_total = 12;
        s1.env_spoke_with_any = 1;
        s1.companionship_days = 100; // milestone — fires companionship-milestone
                                     // since_last_proactive_minutes stays at base_inputs default (Some(8)) — short.
        let rules1 = proactive_rules(&s1);

        // Scenario 2: long-idle + under-chatty + !pre-quiet + long-absence.
        // Covers: long-idle-no-restraint, long-absence-reunion, plus everything
        // except chatty / pre-quiet.
        let mut s2 = s1;
        s2.pre_quiet_minutes = None;
        s2.today_speech_count = 0;
        s2.since_last_proactive_minutes = Some(LONG_IDLE_MINUTES);
        s2.idle_minutes = LONG_ABSENCE_MINUTES + 60;
        s2.idle_register = "用户已经离开了大半天";
        let rules2 = proactive_rules(&s2);

        // Scenario 3: late-night-wellness — needs hour<4 AND idle_minutes<5.
        // Distinct from s1/s2 (both at hour=14, large idle) because the rule is
        // specifically designed to fire when the user is actively at the
        // keyboard past midnight.
        let mut s3 = base_inputs();
        s3.hour = 2;
        s3.idle_minutes = 1;
        s3.idle_register = "用户刚刚还在";
        let rules3 = proactive_rules(&s3);

        let combined: Vec<&String> = rules1
            .iter()
            .chain(rules2.iter())
            .chain(rules3.iter())
            .collect();
        assert!(
            !combined.iter().any(|r| r.contains("规则文本待补")),
            "proactive_rules emitted the unknown-label fallback. A helper added a \
             label without a matching arm in proactive_rules.\nRules1: {:#?}\nRules2: {:#?}\nRules3: {:#?}",
            rules1,
            rules2,
            rules3,
        );
        for (label, fp) in fingerprints {
            assert!(
                combined.iter().any(|r| r.contains(fp)),
                "label '{}' should produce a rule containing '{}' in at least one of \
                 the two scenarios, but neither matched. Either the arm is missing \
                 or its text changed.",
                label,
                fp
            );
        }
    }

    #[test]
    fn frontend_prompt_rule_descriptions_have_no_ghost_labels() {
        // Iter 90 — reverse of Iter 89's coverage check. A "ghost" entry is a key in
        // PROMPT_RULE_DESCRIPTIONS that no backend label helper ever returns; it
        // wastes dictionary space and quietly hides a UI string nobody can ever see.
        // Catches dead translations left behind after a backend rule was renamed or
        // removed.
        let frontend_keys = parse_prompt_rule_dict_keys();
        assert!(
            !frontend_keys.is_empty(),
            "PROMPT_RULE_DESCRIPTIONS dictionary appears empty — parser may be broken \
             or the dict was emptied. Iter 89's test should also be failing."
        );
        // All-true / max-trigger inputs surface every label all backend helpers can
        // ever produce. Same input recipe as Iter 89's test for consistency.
        // Two composite calls union: late-night needs hour<4 + idle<5 which
        // conflicts with long-absence (idle ≥ 240) so we collect both.
        let env = active_environmental_rule_labels(true, true, true, true, true, true);
        let data = active_data_driven_rule_labels(0, 999, 1, 999, 0, 100);
        let composite_a = active_composite_rule_labels(
            true,
            true,
            Some(120),
            0,
            5,
            false,
            LONG_ABSENCE_MINUTES + 60,
            14,
            false,
        );
        let composite_b =
            active_composite_rule_labels(false, false, None, 0, 5, false, 0, 2, false);
        let backend: std::collections::HashSet<&'static str> = env
            .iter()
            .chain(data.iter())
            .chain(composite_a.iter())
            .chain(composite_b.iter())
            .copied()
            .collect();
        let ghosts: Vec<&str> = frontend_keys
            .iter()
            .filter(|k| !backend.contains(k.as_str()))
            .map(|s| s.as_str())
            .collect();
        assert!(
            ghosts.is_empty(),
            "PROMPT_RULE_DESCRIPTIONS contains ghost keys with no backend producer: \
             {:?}. Either remove them, or add the corresponding backend label.",
            ghosts
        );
    }

    #[test]
    fn frontend_prompt_rule_descriptions_cover_every_backend_label() {
        // Iter 89 — guard the contract between the Rust label helpers and the frontend
        // PROMPT_RULE_DESCRIPTIONS dictionary in PanelDebug.tsx. If either side adds a
        // label without updating the other, the panel falls back to "(label X 暂无中文
        // 描述)" — visible but easy to miss in dev. This test fails CI instead.
        //
        // Iter 90 unified the parsing path: both this test and the ghost-keys test go
        // through `parse_prompt_rule_dict_keys`, so they validate against the same
        // canonical key set extracted from the dictionary block.
        let frontend_keys = parse_prompt_rule_dict_keys();
        assert!(
            !frontend_keys.is_empty(),
            "PROMPT_RULE_DESCRIPTIONS dictionary not found or empty in panelTypes.ts \
             — the frontend may have moved or renamed it. Update parse_prompt_rule_dict_keys."
        );
        let frontend_set: std::collections::HashSet<&str> =
            frontend_keys.iter().map(|s| s.as_str()).collect();
        // All-true inputs surface every possible label all helpers can ever return.
        // late-night-wellness needs hour<4 + idle<5 — conflicts with long-absence,
        // so emit it via a second composite call and chain.
        let env = active_environmental_rule_labels(true, true, true, true, true, true);
        let data = active_data_driven_rule_labels(0, 999, 1, 999, 0, 100);
        let composite_a = active_composite_rule_labels(
            true,
            true,
            Some(120),
            0,
            5,
            false,
            LONG_ABSENCE_MINUTES + 60,
            14,
            false,
        );
        let composite_b =
            active_composite_rule_labels(false, false, None, 0, 5, false, 0, 2, false);
        let missing: Vec<&'static str> = env
            .iter()
            .chain(data.iter())
            .chain(composite_a.iter())
            .chain(composite_b.iter())
            .filter(|l| !frontend_set.contains(*l))
            .copied()
            .collect();
        assert!(
            missing.is_empty(),
            "panelTypes.ts PROMPT_RULE_DESCRIPTIONS missing entries for backend \
             labels: {:?}. Add a {{title, summary, nature}} row for each.",
            missing
        );
    }

    #[test]
    fn proactive_rules_contextual_count_matches_label_count() {
        // The match-by-label refactor (Iter 87) means the number of contextual rules
        // pushed must equal env_labels.len() + data_labels.len(). If a future helper
        // returns a label proactive_rules doesn't recognize, the fallback "(规则文本待补)"
        // catches it but this test pins down the healthy path.
        let mut inputs = base_inputs();
        // Trip every contextual rule.
        inputs.wake_hint = "（用户的电脑在大约 60 秒前刚从休眠唤醒。）";
        inputs.is_first_mood = true;
        inputs.pre_quiet_minutes = Some(10);
        inputs.reminders_hint = "你有以下到期的用户提醒：\n· something";
        inputs.plan_hint = "你今天的小目标：\n· something";
        inputs.proactive_history_count = 0;
        inputs.today_speech_count = inputs.chatty_day_threshold;
        inputs.env_spoke_total = 12;
        inputs.env_spoke_with_any = 1;
        // pre-quiet on AND long-idle are mutually exclusive by design (long-idle
        // requires !pre_quiet). Likewise chatty (today_count >= threshold) and
        // long-idle exclude each other. This scenario: pre-quiet on, today=0
        // (no chatty AND first-of-day fires — Iter Cξ added that 6th env label),
        // long-idle won't fire — so we expect:
        // 6 base + 6 env (wake-back/first-mood/first-of-day/pre-quiet/reminders/plan)
        // + 2 data (icebreaker + env-awareness) + 1 composite (engagement-window)
        // = 15. The fingerprint test below covers long-idle / chatty / long-absence
        // in a separate scenario, so combined coverage is still complete.
        inputs.today_speech_count = 0;
        let rules = proactive_rules(&inputs);
        assert_eq!(rules.len(), 6 + 6 + 2 + 1, "rules: {:#?}", rules);
        assert!(!rules.iter().any(|r| r.contains("规则文本待补")));
    }

    #[test]
    fn proactive_rules_baseline_only_pushes_always_on_rules() {
        // With a neutral base_inputs (no contextual triggers), only the 5 always-pushed
        // rules should appear — proves the contextual loop adds nothing when labels are empty.
        let rules = proactive_rules(&base_inputs());
        // Strip the 6 always-on rules: silent / speak / single-line / tools / cache / motion.
        // Wait — there are actually 6 always-on (counted directly from the code). Use that.
        assert_eq!(
            rules.len(),
            6,
            "expected exactly 6 always-on rules in neutral state"
        );
    }

    #[test]
    fn env_awareness_corrective_rule_absent_when_healthy() {
        let mut inputs = base_inputs();
        inputs.env_spoke_total = 12;
        inputs.env_spoke_with_any = 8; // ~67%
        let rules = proactive_rules(&inputs);
        assert!(!rules
            .iter()
            .any(|r| r.contains("最近你开口前几乎都没看环境")));
    }

    #[test]
    fn chatty_mode_tag_disabled_when_threshold_zero() {
        assert_eq!(chatty_mode_tag(0, 0), None);
        assert_eq!(chatty_mode_tag(99, 0), None);
    }

    #[test]
    fn chatty_mode_tag_below_threshold_is_none() {
        assert_eq!(chatty_mode_tag(0, 5), None);
        assert_eq!(chatty_mode_tag(4, 5), None);
    }

    #[test]
    fn chatty_mode_tag_at_or_above_threshold_formats() {
        assert_eq!(chatty_mode_tag(5, 5), Some("chatty=5/5".to_string()));
        assert_eq!(chatty_mode_tag(7, 5), Some("chatty=7/5".to_string()));
    }

    #[test]
    fn append_outcome_tag_handles_empty_and_dash_and_chained() {
        // Iter QG3: shared decision-log reason builder. Empty start, then dash sentinel,
        // then plain chaining — three behaviors the loop and manual-trigger paths both
        // depend on.
        let mut empty = String::new();
        append_outcome_tag(&mut empty, "source=loop");
        assert_eq!(empty, "source=loop");

        let mut dash = "-".to_string();
        append_outcome_tag(&mut dash, "source=manual");
        assert_eq!(dash, "source=manual", "dash placeholder must be replaced");

        let mut chained = "chatty=5/5".to_string();
        append_outcome_tag(&mut chained, "source=loop");
        append_outcome_tag(&mut chained, "rules=icebreaker");
        assert_eq!(chained, "chatty=5/5, source=loop, rules=icebreaker");
    }

    #[test]
    fn record_proactive_outcome_spoke_path_bumps_counters_and_logs_source() {
        use crate::commands::debug::ProcessCounters;
        use crate::decision_log::DecisionLog;
        let counters = ProcessCounters::default();
        let decisions = DecisionLog::new();
        let outcome: Result<ProactiveTurnOutcome, String> = Ok(ProactiveTurnOutcome {
            reply: Some("hello".to_string()),
            tools: vec!["get_active_window".to_string()],
        });
        record_proactive_outcome(&counters, &decisions, "manual", "-", None, &outcome);
        // llm_outcome.spoke bumped, env_tool counted as spoke_with_any.
        assert_eq!(
            counters
                .llm_outcome
                .spoke
                .load(std::sync::atomic::Ordering::Relaxed),
            1
        );
        assert_eq!(
            counters
                .env_tool
                .spoke_total
                .load(std::sync::atomic::Ordering::Relaxed),
            1
        );
        assert_eq!(
            counters
                .env_tool
                .spoke_with_any
                .load(std::sync::atomic::Ordering::Relaxed),
            1
        );
        let snap = decisions.snapshot();
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].kind, "Spoke");
        assert!(snap[0].reason.contains("source=manual"));
        assert!(snap[0].reason.contains("tools=get_active_window"));
    }

    #[test]
    fn record_proactive_outcome_silent_path_bumps_silent_and_tags_loop() {
        use crate::commands::debug::ProcessCounters;
        use crate::decision_log::DecisionLog;
        let counters = ProcessCounters::default();
        let decisions = DecisionLog::new();
        let outcome: Result<ProactiveTurnOutcome, String> = Ok(ProactiveTurnOutcome {
            reply: None,
            tools: vec![],
        });
        record_proactive_outcome(
            &counters,
            &decisions,
            "loop",
            "chatty=5/5",
            Some("rules=chatty"),
            &outcome,
        );
        assert_eq!(
            counters
                .llm_outcome
                .silent
                .load(std::sync::atomic::Ordering::Relaxed),
            1
        );
        // env_tool MUST stay zero on silent — it would skew the env-aware ratio otherwise.
        assert_eq!(
            counters
                .env_tool
                .spoke_total
                .load(std::sync::atomic::Ordering::Relaxed),
            0
        );
        let snap = decisions.snapshot();
        assert_eq!(snap[0].kind, "LlmSilent");
        assert!(snap[0].reason.contains("source=loop"));
        assert!(snap[0].reason.contains("rules=chatty"));
        assert!(snap[0].reason.contains("chatty=5/5"));
    }

    #[test]
    fn record_proactive_outcome_error_path_bumps_error_and_includes_message() {
        use crate::commands::debug::ProcessCounters;
        use crate::decision_log::DecisionLog;
        let counters = ProcessCounters::default();
        let decisions = DecisionLog::new();
        let outcome: Result<ProactiveTurnOutcome, String> = Err("network down".to_string());
        record_proactive_outcome(&counters, &decisions, "manual", "-", None, &outcome);
        assert_eq!(
            counters
                .llm_outcome
                .error
                .load(std::sync::atomic::Ordering::Relaxed),
            1
        );
        let snap = decisions.snapshot();
        assert_eq!(snap[0].kind, "LlmError");
        assert!(snap[0].reason.contains("network down"));
        assert!(snap[0].reason.contains("source=manual"));
    }

    // -- Iter QG4: redact-on-reinjection coverage ---------------------------------

    /// Test-side redact closure that emulates `redact_with_settings` with a fixed
    /// substring list — case-insensitive, single pass. Avoids touching real settings
    /// state so these tests can run in any order.
    fn test_redactor(patterns: &'static [&'static str]) -> impl Fn(&str) -> String {
        move |text: &str| {
            let owned: Vec<String> = patterns.iter().map(|s| (*s).to_string()).collect();
            crate::redaction::redact_text(text, &owned)
        }
    }

    #[test]
    fn format_reminders_hint_redacts_topic_and_title() {
        let items = vec![(
            "23:00".to_string(),
            "跟 SecretCorp 同事喝咖啡".to_string(),
            "secretcorp_coffee".to_string(),
        )];
        let out = format_reminders_hint(&items, &test_redactor(&["SecretCorp"]));
        // Both topic and title must contain the marker; original term must not survive.
        assert!(out.contains("(私人)"));
        assert!(!out.to_lowercase().contains("secretcorp"));
        // Header survives unredacted (no patterns match).
        assert!(out.contains("到期的用户提醒"));
    }

    #[test]
    fn format_reminders_hint_empty_returns_empty_string() {
        let out = format_reminders_hint(&[], &test_redactor(&["irrelevant"]));
        assert_eq!(out, "");
    }

    #[test]
    fn format_plan_hint_redacts_description() {
        let desc = "· 关心 SecretCorp 项目进展 [0/2]";
        let out = format_plan_hint(desc, &test_redactor(&["SecretCorp"]));
        assert!(out.contains("(私人)"));
        assert!(!out.to_lowercase().contains("secretcorp"));
        assert!(out.contains("你今天的小目标"));
    }

    #[test]
    fn format_plan_hint_empty_or_whitespace_returns_empty() {
        let r = test_redactor(&["x"]);
        assert_eq!(format_plan_hint("", &r), "");
        assert_eq!(format_plan_hint("   \n  ", &r), "");
    }

    #[test]
    fn format_proactive_mood_hint_redacts_text() {
        let out = format_proactive_mood_hint(
            "和 SecretCorp 同事开会有点累",
            &test_redactor(&["SecretCorp"]),
        );
        assert!(out.contains("(私人)"));
        assert!(!out.to_lowercase().contains("secretcorp"));
        assert!(out.contains("你上次记录的心情/状态"));
    }

    #[test]
    fn format_proactive_mood_hint_empty_returns_first_time_message() {
        let out = format_proactive_mood_hint("", &test_redactor(&["x"]));
        assert!(out.contains("还没有记录过"));
        assert!(out.contains("第一次"));
    }

    #[test]
    fn chatty_day_threshold_is_user_tunable() {
        // Custom threshold of 10 should hold its boundary regardless of the const.
        let mut inputs = base_inputs();
        inputs.chatty_day_threshold = 10;
        inputs.today_speech_count = 9;
        assert!(!proactive_rules(&inputs)
            .iter()
            .any(|r| r.contains("今天已经聊了不少")));
        inputs.today_speech_count = 10;
        assert!(proactive_rules(&inputs)
            .iter()
            .any(|r| r.contains("今天已经聊了不少")));
    }

    #[test]
    fn plan_hint_appears_in_full_prompt() {
        let mut inputs = base_inputs();
        inputs.plan_hint = "你今天的小目标 / 计划：\n· 关心用户工作进展 [0/2]";
        let p = build_proactive_prompt(&inputs);
        assert!(p.contains("关心用户工作进展"));
    }

    #[test]
    fn pre_quiet_rule_appears_when_set() {
        let mut inputs = base_inputs();
        inputs.pre_quiet_minutes = Some(10);
        let rules = proactive_rules(&inputs);
        assert_eq!(rules.len(), 7);
        assert!(rules
            .iter()
            .any(|r| r.contains("快进入安静时段") && r.contains("10 分钟")));
    }
}
