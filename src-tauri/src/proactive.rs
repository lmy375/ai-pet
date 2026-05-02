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

use chrono::Timelike;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Mutex as TokioMutex;

use crate::commands::chat::{run_chat_pipeline, ChatMessage, CollectingSink};
use crate::commands::debug::{write_log, LogStore};
use crate::commands::session;
use crate::commands::settings::{get_settings, get_soul};
use crate::commands::shell::ShellStore;
use crate::config::AiConfig;
use crate::input_idle::user_input_idle_seconds;
use crate::mcp::McpManagerStore;
use crate::mood::{read_current_mood_parsed, read_mood_for_event, MOOD_CATEGORY, MOOD_TITLE};
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

    pub async fn snapshot(&self) -> ClockSnapshot {
        let g = self.inner.lock().await;
        ClockSnapshot {
            idle_seconds: g.last.elapsed().as_secs(),
            since_last_proactive_seconds: g.last_proactive.map(|t| t.elapsed().as_secs()),
            awaiting_user_reply: g.awaiting_user_reply,
        }
    }
}

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

const SILENT_MARKER: &str = "<silent>";

/// What the proactive loop should do this tick. Each variant maps to one outer-loop branch:
/// `Silent` skips quietly, `Skip` logs the reason, `Run` triggers a real proactive turn.
/// All variants now carry a debug reason so the panel can show *why* a tick was silent
/// (disabled / quiet hours / idle short — these used to be indistinguishable in the UI).
#[derive(Debug, PartialEq, Eq)]
enum LoopAction {
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

/// All the variable bits that go into the proactive prompt. Kept as a single struct so
/// the builder function has a clean signature and tests can inject specific values
/// without threading 9 individual arguments.
pub struct PromptInputs<'a> {
    pub time: &'a str,
    pub period: &'a str,
    pub idle_minutes: u64,
    pub input_hint: &'a str,
    pub cadence_hint: &'a str,
    pub mood_hint: &'a str,
    /// Empty string when the gate isn't applicable (no focus active, no recent wake,
    /// no prior speeches). Builder skips empty optional sections automatically.
    pub focus_hint: &'a str,
    pub wake_hint: &'a str,
    pub speech_hint: &'a str,
    /// True when the pet has never recorded a mood entry yet. Lets the rules block add a
    /// "create instead of update" hint so the model bootstraps the file correctly.
    pub is_first_mood: bool,
    /// Minutes until the configured quiet-hours window starts, when within the
    /// look-ahead window (default 15 min). None means "not approaching quiet hours" —
    /// either disabled, already inside, or more than 15 min away.
    pub pre_quiet_minutes: Option<u64>,
}

/// The "约束" rules block of the proactive prompt — extracted into its own builder so
/// adding a rule is just `rules.push(...)` instead of squeezing a line into the middle
/// of a giant template. Rules are now context-aware: certain rules are added only when
/// the corresponding PromptInputs flag/hint is set, so the LLM gets the most useful
/// guidance for *this* moment instead of a static one-size-fits-all list.
pub fn proactive_rules(inputs: &PromptInputs) -> Vec<String> {
    let mut rules: Vec<String> = Vec::with_capacity(8);
    rules.push(format!(
        "- 如果你判断**不打扰**用户更好（比如只是想保持安静），只回复一个标记：`{}`，不要其他任何文字。",
        SILENT_MARKER
    ));
    rules.push(format!(
        "- 如果决定开口，就直接说话，不要解释自己为什么开口，也不要包含 `{}`。",
        SILENT_MARKER
    ));
    rules.push("- 只说一句话，简短自然，像伙伴一样。".into());
    rules.push("- 必要时可以调用工具：`get_active_window`（看用户在用什么 app，开口前优先调一次让话题贴合当下）、`get_upcoming_events`（看用户接下来几小时有没有日程，可用于提醒类话题，记得日程是私人内容不要原样念出）、`get_weather`（看下天气当作闲聊话题，偶尔用一次就好不要每次都查）、`memory_search`（翻一下用户偏好）。".into());
    rules.push("- 这三个环境工具（`get_active_window` / `get_weather` / `get_upcoming_events`）每次调用都有真实的 IO 成本，并且**同一次主动开口检查内重复调用同样的参数会拿到完全一样的结果**——所以一次足够了，不要为了「再确认一下」反复调，相信首次返回值直接做判断。".into());
    rules.push(format!(
        "- **决定开口后**：请用 `memory_edit` 更新 `{cat}` 类别下 `{title}` 的记忆（不存在就 `create`，存在就 `update`）。description 必须以这种格式开头：`[motion: X] 你此刻的心情和想法`，其中 X 是你想做的 Live2D 动作分组，从这四个里选一个：`Tap`（开心/活泼/兴奋）、`Flick`（想分享/有兴致/活力）、`Flick3`（焦虑/烦躁/不安）、`Idle`（平静/低落/累/沉静）。前缀后面才是自由文字。例：`[motion: Tap] 看用户在专心写代码，有点替他高兴`。沉默时无需更新。",
        cat = MOOD_CATEGORY,
        title = MOOD_TITLE,
    ));

    // ---- context-driven rules ----
    if !inputs.wake_hint.trim().is_empty() {
        rules.push(
            "- **用户刚从离开桌子回来**：问候要简短克制，先轻打招呼或简短关心一句，不要立刻提日程/工作类信息密集的话题。"
                .into(),
        );
    }
    if inputs.is_first_mood {
        rules.push(format!(
            "- **第一次开口**：你还没有写过 `{}/{}` 记忆条目，开口后应当用 `memory_edit create` 而非 `update` 来初始化它（按上面格式）。",
            MOOD_CATEGORY, MOOD_TITLE
        ));
    }
    if let Some(mins) = inputs.pre_quiet_minutes {
        rules.push(format!(
            "- **快进入安静时段**：再过约 {} 分钟就到夜里的安静时段了。语气要往收尾靠——简短的晚安/睡前关心比新话题合适。",
            mins
        ));
    }
    rules
}

/// Assemble the proactive prompt from a Vec of sections rather than a giant `format!()`.
/// Adding a new optional hint is now: (a) extend `PromptInputs`, (b) push it via
/// `push_if_nonempty`. Adding a new constraint rule is one push in `proactive_rules`.
pub fn build_proactive_prompt(inputs: &PromptInputs) -> String {
    let mut s: Vec<String> = Vec::with_capacity(20);
    s.push("[系统提示·主动开口检查]".into());
    s.push(String::new());
    s.push(format!(
        "现在是 {}（{}）。距离上次和用户互动已经过去约 {} 分钟。{}",
        inputs.time, inputs.period, inputs.idle_minutes, inputs.input_hint
    ));
    s.push(inputs.cadence_hint.to_string());
    s.push(String::new());
    s.push(inputs.mood_hint.to_string());
    push_if_nonempty(&mut s, inputs.focus_hint);
    push_if_nonempty(&mut s, inputs.wake_hint);
    push_if_nonempty(&mut s, inputs.speech_hint);
    s.push(String::new());
    s.push(
        "请判断：作为陪伴用户的 AI 宠物，此时此刻你想主动跟用户说点什么吗？可以是关心、闲聊、提醒、分享想法都行。".into()
    );
    s.push(String::new());
    s.push("约束：".into());
    s.extend(proactive_rules(inputs));
    s.join("\n")
}

fn push_if_nonempty(sections: &mut Vec<String>, s: &str) {
    if !s.trim().is_empty() {
        sections.push(s.to_string());
    }
}

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
}

#[tauri::command]
pub async fn get_tone_snapshot(
    clock: tauri::State<'_, InteractionClockStore>,
    wake: tauri::State<'_, crate::wake_detector::WakeDetectorStore>,
) -> Result<ToneSnapshot, String> {
    let now = chrono::Local::now();
    let hour = now.hour() as u8;
    let minute = now.minute() as u8;
    let snap = clock.snapshot().await;
    let cadence_min = snap.since_last_proactive_seconds.map(|s| s / 60);
    let cadence = cadence_min.map(|m| idle_tier(m).to_string());
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
    Ok(ToneSnapshot {
        period: period_of_day(hour).to_string(),
        cadence,
        since_last_proactive_minutes: cadence_min,
        wake_seconds_ago: wake_ago,
        mood_text,
        mood_motion,
        pre_quiet_minutes,
    })
}

/// Map an elapsed-minutes count (since the pet last spoke proactively) to a Chinese
/// "cadence" label. Lets the LLM shift register from "continuing a thread" through
/// "checking back in" to "haven't talked in ages" without doing the math itself.
/// Boundaries are conversational, not strict — 16 minutes is still "聊过一会儿".
pub fn idle_tier(minutes: u64) -> &'static str {
    match minutes {
        0..=15 => "刚说过话，话题还热",
        16..=60 => "聊过一会儿了",
        61..=360 => "几小时没说话",
        361..=1440 => "已经隔了大半天",
        _ => "上次聊已经是昨天或更早",
    }
}

/// Map a 24-hour clock value (0–23) to a Chinese period-of-day label. Used in the
/// proactive prompt so the LLM can riff on time-of-day vibes ("早上的咖啡时间到了") rather
/// than just seeing a numeric timestamp. Boundaries match common Chinese conversational
/// usage; the function is `pub` so tests can pin them.
pub fn period_of_day(hour: u8) -> &'static str {
    match hour {
        5..=7 => "清晨",
        8..=10 => "上午",
        11..=12 => "中午",
        13..=16 => "下午",
        17..=18 => "傍晚",
        19..=21 => "晚上",
        _ => "深夜", // 22, 23, 0..=4
    }
}

/// How many minutes until the next quiet-hours boundary, when that boundary is within
/// `look_ahead_minutes`. Returns `None` when:
/// - quiet hours are disabled (start == end)
/// - we're already inside the quiet window (then there's nothing to "approach")
/// - the boundary is more than `look_ahead_minutes` away
///
/// Used to inject a "winding down for the night" rule into the prompt so the pet eases
/// into the quiet window with a gentler tone instead of going from full chatter to a
/// hard silent gate. Pure so tests can pin every interesting (now, start) combination.
pub fn minutes_until_quiet_start(
    now_hour: u8,
    now_minute: u8,
    quiet_start: u8,
    quiet_end: u8,
    look_ahead_minutes: u64,
) -> Option<u64> {
    if quiet_start == quiet_end {
        return None;
    }
    if in_quiet_hours(now_hour, quiet_start, quiet_end) {
        return None;
    }
    let now_total = now_hour as i32 * 60 + now_minute as i32;
    let start_total = quiet_start as i32 * 60;
    let mut delta = start_total - now_total;
    if delta < 0 {
        delta += 24 * 60; // next day's quiet_start
    }
    let delta_u = delta as u64;
    if delta_u <= look_ahead_minutes {
        Some(delta_u)
    } else {
        None
    }
}

/// Returns true if `hour` (0–23) falls inside the quiet window `[start, end)`. Handles the
/// midnight wrap-around case (start > end, e.g. 23:00–07:00). When start == end, the gate
/// is treated as disabled (no quiet hours configured).
fn in_quiet_hours(hour: u8, start: u8, end: u8) -> bool {
    if start == end {
        return false;
    }
    if start < end {
        // Same-day window, e.g. 13–15.
        hour >= start && hour < end
    } else {
        // Wraps past midnight, e.g. 23–7. In quiet if hour >= 23 OR hour < 7.
        hour >= start || hour < end
    }
}

/// Length of the wake-from-sleep grace window (seconds). Within this window after a
/// detected wake, cooldown is treated as elapsed and idle threshold is halved — so the
/// pet is more eager to greet a returning user. Awaiting / focus / quiet gates stay
/// untouched: those reflect user preference, wake doesn't override them.
const WAKE_GRACE_WINDOW_SECS: u64 = 600;

/// True when a wake event happened recently enough to soften the gates.
fn wake_recent(wake_seconds_ago: Option<u64>) -> bool {
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
fn evaluate_pre_input_idle(
    cfg: &crate::commands::settings::ProactiveConfig,
    snap: &ClockSnapshot,
    hour: u8,
    focus_active: Option<bool>,
    wake_seconds_ago: Option<u64>,
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
    let wake_soft = wake_recent(wake_seconds_ago);
    if let Some(since) = snap.since_last_proactive_seconds {
        if !wake_soft && cfg.cooldown_seconds > 0 && since < cfg.cooldown_seconds {
            return Err(LoopAction::Skip(format!(
                "Proactive: skip — cooldown ({}s < {}s)",
                since, cfg.cooldown_seconds
            )));
        }
    }
    // Gate 3: quiet hours. A real friend lets you sleep. Silent skip rather than logged
    // skip — this can happen on every tick during night, no value in spamming logs.
    if in_quiet_hours(hour, cfg.quiet_hours_start, cfg.quiet_hours_end) {
        return Err(LoopAction::Silent { reason: "quiet_hours" });
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
        return Err(LoopAction::Silent { reason: "idle_below_threshold" });
    }
    Ok(())
}

/// Gate 4: input-idle. Don't interrupt while the user is actively at the keyboard/mouse.
/// `input_idle_seconds = 0` disables the gate; `input_idle = None` (non-macOS) is treated
/// as a pass so behavior degrades to "rely on the interaction-time gate only".
fn evaluate_input_idle_gate(
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

/// Evaluate every gate in priority order and return the action this tick should take.
/// Composes the pure pre-input-idle gates with the IO call to query keyboard/mouse idle.
async fn evaluate_loop_tick(
    app: &AppHandle,
    settings: &crate::commands::settings::AppSettings,
) -> LoopAction {
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

    if let Err(action) =
        evaluate_pre_input_idle(cfg, &snap, hour, focus_active, wake_seconds_ago)
    {
        return action;
    }
    let input_idle = user_input_idle_seconds().await;
    evaluate_input_idle_gate(cfg, &snap, input_idle)
}

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
                    &format!("Proactive: wake-from-sleep detected (gap {}s)", gap.as_secs()),
                );
            }

            let action = evaluate_loop_tick(&app, &settings).await;
            // Record before dispatching so even paths that immediately sleep are visible
            // in the panel — the entire point of this log is "why didn't anything happen".
            let decisions = app
                .state::<crate::decision_log::DecisionLogStore>()
                .inner()
                .clone();
            match &action {
                LoopAction::Silent { reason } => {
                    decisions.push("Silent", (*reason).to_string());
                }
                LoopAction::Skip(reason) => {
                    decisions.push("Skip", reason.clone());
                }
                LoopAction::Run { idle_seconds, input_idle_seconds } => {
                    decisions.push(
                        "Run",
                        format!(
                            "idle={}s, input_idle={}",
                            idle_seconds,
                            input_idle_seconds
                                .map(|s| s.to_string())
                                .unwrap_or_else(|| "?".to_string()),
                        ),
                    );
                }
            }

            match action {
                LoopAction::Silent { .. } => {}
                LoopAction::Skip(reason) => {
                    let log_store = app.state::<LogStore>().inner().clone();
                    write_log(&log_store.0, &reason);
                }
                LoopAction::Run { idle_seconds, input_idle_seconds } => {
                    if let Err(e) = run_proactive_turn(&app, idle_seconds, input_idle_seconds).await
                    {
                        eprintln!("Proactive turn failed: {}", e);
                    }
                }
            }

            tokio::time::sleep(Duration::from_secs(interval)).await;
        }
    });
}

/// Build the prompt, ask the LLM, emit the reply, and persist it.
async fn run_proactive_turn(
    app: &AppHandle,
    idle_seconds: u64,
    input_idle_seconds: Option<u64>,
) -> Result<(), String> {
    let config = AiConfig::from_settings()?;
    let mcp_store = app.state::<McpManagerStore>().inner().clone();
    let log_store = app.state::<LogStore>().inner().clone();
    let shell_store = app.state::<ShellStore>().inner().clone();
    let process_counters = app
        .state::<crate::commands::debug::ProcessCountersStore>()
        .inner()
        .clone();
    let clock = app.state::<InteractionClockStore>().inner().clone();

    let ctx = ToolContext::new(log_store, shell_store, process_counters);

    // Try to load the latest session so the proactive turn has the recent context. If none
    // exists yet, fall back to a system-only conversation.
    let (session_id, mut messages) = load_active_session();

    let soul = get_soul().unwrap_or_default();
    let now_local = chrono::Local::now();
    let idle_minutes = idle_seconds / 60;
    let input_hint = match input_idle_seconds {
        Some(secs) => format!("用户键鼠空闲约 {} 秒。", secs),
        None => "（无法读取键鼠空闲信息。）".to_string(),
    };

    let mood_parsed = read_current_mood_parsed();
    let is_first_mood = !matches!(&mood_parsed, Some((text, _)) if !text.trim().is_empty());
    let mood_hint = match &mood_parsed {
        Some((text, _)) if !text.trim().is_empty() => {
            format!("你上次记录的心情/状态：「{}」。", text.trim())
        }
        _ => "（还没有记录过你自己的心情/状态。这是第一次。）".to_string(),
    };

    // Distance since the pet last spoke proactively — different from idle_seconds (which
    // resets on any interaction). Lets the LLM pick a register: continuation vs. casual
    // check-in vs. "haven't talked in ages".
    let cadence_hint = {
        let snap = clock.snapshot().await;
        match snap.since_last_proactive_seconds.map(|s| s / 60) {
            Some(m) => format!("距上次你主动开口约 {} 分钟（{}）。", m, idle_tier(m)),
            None => "你还没有主动开过口，这是第一次。".to_string(),
        }
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
    let speech_hint = {
        let recent = crate::speech_history::recent_speeches(5).await;
        if recent.is_empty() {
            String::new()
        } else {
            let bullets: Vec<String> = recent
                .iter()
                .map(|line| format!("· {}", crate::speech_history::strip_timestamp(line)))
                .collect();
            format!(
                "你最近主动说过的几句话（旧→新），开口前看一眼避免重复：\n{}",
                bullets.join("\n")
            )
        }
    };

    // Surface the user's active Focus mode (if any) so the pet can speak around it. This
    // path normally only runs when the user has unset `respect_focus_mode` — otherwise the
    // gate would have skipped before we got here.
    let focus_hint = match crate::focus_mode::focus_status().await {
        Some(s) if s.active => match &s.name {
            Some(n) => format!("用户当前开着 macOS Focus 模式：「{}」（说明 ta 想专注，开口要克制）。", n),
            None => "用户当前开着某个 macOS Focus 模式（说明 ta 想专注，开口要克制）。".to_string(),
        },
        _ => String::new(),
    };

    let period = period_of_day(now_local.hour() as u8);
    let time_str = now_local.format("%Y-%m-%d %H:%M").to_string();
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
        idle_minutes,
        input_hint: &input_hint,
        cadence_hint: &cadence_hint,
        mood_hint: &mood_hint,
        focus_hint: &focus_hint,
        wake_hint: &wake_hint,
        speech_hint: &speech_hint,
        is_first_mood,
        pre_quiet_minutes,
    });

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

    // Treat empty / silent marker as "do nothing".
    if reply_trimmed.is_empty() || reply_trimmed.contains(SILENT_MARKER) {
        ctx.log(&format!("Proactive: silent (idle={}s)", idle_seconds));
        return Ok(());
    }

    ctx.log(&format!("Proactive: speaking ({} chars, idle={}s)", reply_trimmed.len(), idle_seconds));

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

    let payload = ProactiveMessage {
        text: reply_trimmed.to_string(),
        timestamp: now_local.format("%Y-%m-%dT%H:%M:%S%.3f").to_string(),
        mood: mood_after,
        motion: motion_after,
    };
    let _ = app.emit("proactive-message", payload);

    Ok(())
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
    sess.updated_at = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S%.3f").to_string();
    session::save_session(sess)
}

#[cfg(test)]
mod prompt_tests {
    use super::*;

    fn base_inputs<'a>() -> PromptInputs<'a> {
        PromptInputs {
            time: "2026-05-03 14:30",
            period: "下午",
            idle_minutes: 20,
            input_hint: "用户键鼠空闲约 60 秒。",
            cadence_hint: "距上次你主动开口约 8 分钟（刚说过话，话题还热）。",
            mood_hint: "你上次记录的心情/状态：「平静」。",
            focus_hint: "",
            wake_hint: "",
            speech_hint: "",
            is_first_mood: false,
            pre_quiet_minutes: None,
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
        let bullets = "你最近主动说过的几句话（旧→新），开口前看一眼避免重复：\n· 早上好啊\n· 加油码代码";
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
        assert_eq!(rules.len(), 6, "no context-driven rules without their flags");
    }

    #[test]
    fn pre_quiet_rule_appears_when_set() {
        let mut inputs = base_inputs();
        inputs.pre_quiet_minutes = Some(10);
        let rules = proactive_rules(&inputs);
        assert_eq!(rules.len(), 7);
        assert!(rules.iter().any(|r| r.contains("快进入安静时段") && r.contains("10 分钟")));
    }
}

#[cfg(test)]
mod pre_quiet_tests {
    use super::minutes_until_quiet_start;

    #[test]
    fn within_window_returns_minutes() {
        // 22:50 + quiet 23..7 → 10 min until quiet, look-ahead 15 → Some(10)
        assert_eq!(minutes_until_quiet_start(22, 50, 23, 7, 15), Some(10));
    }

    #[test]
    fn at_window_edge_15_min() {
        assert_eq!(minutes_until_quiet_start(22, 45, 23, 7, 15), Some(15));
    }

    #[test]
    fn outside_window_returns_none() {
        // 16 min before — outside the 15-min look-ahead.
        assert_eq!(minutes_until_quiet_start(22, 44, 23, 7, 15), None);
    }

    #[test]
    fn already_in_quiet_returns_none() {
        // 03:00 inside 23..7 quiet window.
        assert_eq!(minutes_until_quiet_start(3, 0, 23, 7, 15), None);
        // 23:30 also inside.
        assert_eq!(minutes_until_quiet_start(23, 30, 23, 7, 15), None);
    }

    #[test]
    fn disabled_when_start_equals_end() {
        assert_eq!(minutes_until_quiet_start(22, 50, 0, 0, 15), None);
    }

    #[test]
    fn same_day_window() {
        // 13:55 + quiet 14..15 → 5 min.
        assert_eq!(minutes_until_quiet_start(13, 55, 14, 15, 15), Some(5));
    }

    #[test]
    fn past_today_uses_tomorrow() {
        // 07:00 + quiet 23..7 → not in quiet (7 is exclusive end), and quiet_start
        // tomorrow is 23:00, 16h away — way outside look-ahead.
        assert_eq!(minutes_until_quiet_start(7, 0, 23, 7, 15), None);
    }
}

#[cfg(test)]
mod cadence_tests {
    use super::idle_tier;

    #[test]
    fn each_tier_has_a_representative_minute() {
        assert_eq!(idle_tier(0), "刚说过话，话题还热");
        assert_eq!(idle_tier(8), "刚说过话，话题还热");
        assert_eq!(idle_tier(30), "聊过一会儿了");
        assert_eq!(idle_tier(120), "几小时没说话");
        assert_eq!(idle_tier(720), "已经隔了大半天");
        assert_eq!(idle_tier(2000), "上次聊已经是昨天或更早");
    }

    #[test]
    fn boundaries_land_on_expected_side() {
        assert_eq!(idle_tier(15), "刚说过话，话题还热");
        assert_eq!(idle_tier(16), "聊过一会儿了");
        assert_eq!(idle_tier(60), "聊过一会儿了");
        assert_eq!(idle_tier(61), "几小时没说话");
        assert_eq!(idle_tier(360), "几小时没说话");
        assert_eq!(idle_tier(361), "已经隔了大半天");
        assert_eq!(idle_tier(1440), "已经隔了大半天");
        assert_eq!(idle_tier(1441), "上次聊已经是昨天或更早");
    }
}

#[cfg(test)]
mod period_tests {
    use super::period_of_day;

    #[test]
    fn each_bucket_has_a_representative_hour() {
        assert_eq!(period_of_day(6), "清晨");
        assert_eq!(period_of_day(9), "上午");
        assert_eq!(period_of_day(12), "中午");
        assert_eq!(period_of_day(15), "下午");
        assert_eq!(period_of_day(18), "傍晚");
        assert_eq!(period_of_day(20), "晚上");
        assert_eq!(period_of_day(23), "深夜");
        assert_eq!(period_of_day(2), "深夜");
    }

    #[test]
    fn boundaries_land_on_expected_side() {
        // 5:00 transitions night → 清晨; 8:00 transitions 清晨 → 上午; etc.
        assert_eq!(period_of_day(4), "深夜");
        assert_eq!(period_of_day(5), "清晨");
        assert_eq!(period_of_day(7), "清晨");
        assert_eq!(period_of_day(8), "上午");
        assert_eq!(period_of_day(10), "上午");
        assert_eq!(period_of_day(11), "中午");
        assert_eq!(period_of_day(13), "下午");
        assert_eq!(period_of_day(16), "下午");
        assert_eq!(period_of_day(17), "傍晚");
        assert_eq!(period_of_day(19), "晚上");
        assert_eq!(period_of_day(21), "晚上");
        assert_eq!(period_of_day(22), "深夜");
        assert_eq!(period_of_day(0), "深夜");
    }
}

#[cfg(test)]
mod gate_tests {
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
        let action = evaluate_pre_input_idle(&c, &snap(9999, false, None), NOON, None, None);
        assert_eq!(action.unwrap_err(), LoopAction::Silent { reason: "disabled" });
    }

    #[test]
    fn awaiting_user_reply_skips_with_log() {
        let action =
            evaluate_pre_input_idle(&cfg(), &snap(9999, true, None), NOON, None, None).unwrap_err();
        match action {
            LoopAction::Skip(msg) => assert!(msg.contains("awaiting user reply")),
            other => panic!("expected Skip, got {:?}", other),
        }
    }

    #[test]
    fn cooldown_active_skips() {
        let mut c = cfg();
        c.cooldown_seconds = 1800;
        let action =
            evaluate_pre_input_idle(&c, &snap(9999, false, Some(60)), NOON, None, None).unwrap_err();
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
        // Even with a recent proactive turn, cooldown=0 means "no cooldown gate".
        let mut c = cfg();
        c.cooldown_seconds = 0;
        // idle is high enough to pass the next gate, so we should reach Ok.
        let result = evaluate_pre_input_idle(&c, &snap(9999, false, Some(0)), NOON, None, None);
        assert!(result.is_ok(), "expected Ok, got {:?}", result.unwrap_err());
    }

    #[test]
    fn cooldown_elapsed_passes() {
        let mut c = cfg();
        c.cooldown_seconds = 1800;
        let result = evaluate_pre_input_idle(&c, &snap(9999, false, Some(2000)), NOON, None, None);
        assert!(result.is_ok());
    }

    #[test]
    fn idle_below_threshold_silent() {
        let c = cfg(); // threshold=60
        let action = evaluate_pre_input_idle(&c, &snap(30, false, None), NOON, None, None).unwrap_err();
        assert_eq!(action, LoopAction::Silent { reason: "idle_below_threshold" });
    }

    #[test]
    fn idle_threshold_clamped_to_60_minimum() {
        let mut c = cfg();
        c.idle_threshold_seconds = 10; // user-set absurdly low, should clamp up to 60
        let action = evaluate_pre_input_idle(&c, &snap(30, false, None), NOON, None, None).unwrap_err();
        assert_eq!(action, LoopAction::Silent { reason: "idle_below_threshold" },
            "30s should still be below the clamped 60s");
    }

    #[test]
    fn all_sync_gates_pass_returns_ok() {
        let result = evaluate_pre_input_idle(&cfg(), &snap(9999, false, None), NOON, None, None);
        assert!(result.is_ok());
    }

    // ---- in_quiet_hours pure helper ----

    #[test]
    fn quiet_hours_disabled_when_start_equals_end() {
        assert!(!in_quiet_hours(0, 0, 0));
        assert!(!in_quiet_hours(12, 23, 23));
    }

    #[test]
    fn quiet_hours_same_day_window() {
        // 13–15 quiet
        assert!(!in_quiet_hours(12, 13, 15));
        assert!(in_quiet_hours(13, 13, 15));
        assert!(in_quiet_hours(14, 13, 15));
        assert!(!in_quiet_hours(15, 13, 15), "end is exclusive");
    }

    #[test]
    fn quiet_hours_wraps_midnight() {
        // 23–7 quiet (the default)
        assert!(in_quiet_hours(23, 23, 7));
        assert!(in_quiet_hours(0, 23, 7));
        assert!(in_quiet_hours(3, 23, 7));
        assert!(in_quiet_hours(6, 23, 7));
        assert!(!in_quiet_hours(7, 23, 7));
        assert!(!in_quiet_hours(12, 23, 7));
        assert!(!in_quiet_hours(22, 23, 7));
    }

    // ---- quiet hours gate inside evaluate_pre_input_idle ----

    #[test]
    fn quiet_hours_silent_during_window() {
        let mut c = cfg();
        c.quiet_hours_start = 23;
        c.quiet_hours_end = 7;
        // 03:00 — squarely inside the night window.
        let action = evaluate_pre_input_idle(&c, &snap(9999, false, None), 3, None, None).unwrap_err();
        assert_eq!(action, LoopAction::Silent { reason: "quiet_hours" });
    }

    #[test]
    fn quiet_hours_passes_outside_window() {
        let mut c = cfg();
        c.quiet_hours_start = 23;
        c.quiet_hours_end = 7;
        let result = evaluate_pre_input_idle(&c, &snap(9999, false, None), 14, None, None);
        assert!(result.is_ok());
    }

    #[test]
    fn quiet_hours_disabled_does_not_block() {
        let c = cfg(); // both = 0 → disabled
        let result = evaluate_pre_input_idle(&c, &snap(9999, false, None), 3, None, None);
        assert!(result.is_ok(), "disabled quiet hours shouldn't gate");
    }

    // ---- focus-mode gate ----

    #[test]
    fn focus_mode_active_skips_when_respected() {
        let action = evaluate_pre_input_idle(&cfg(), &snap(9999, false, None), NOON, Some(true), None)
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
        let result = evaluate_pre_input_idle(&c, &snap(9999, false, None), NOON, Some(true), None);
        assert!(result.is_ok(), "user opted out of focus respect");
    }

    #[test]
    fn focus_mode_inactive_passes() {
        let result = evaluate_pre_input_idle(&cfg(), &snap(9999, false, None), NOON, Some(false), None);
        assert!(result.is_ok());
    }

    #[test]
    fn focus_mode_unknown_passes() {
        // Non-macOS or unreadable file → None → don't block (fail open).
        let result = evaluate_pre_input_idle(&cfg(), &snap(9999, false, None), NOON, None, None);
        assert!(result.is_ok());
    }

    // ---- wake-from-sleep softening ----

    #[test]
    fn wake_recent_skips_cooldown_gate() {
        // Recent proactive utterance + non-zero cooldown would normally skip, but a fresh
        // wake makes the cooldown irrelevant.
        let mut c = cfg();
        c.cooldown_seconds = 1800;
        let result = evaluate_pre_input_idle(
            &c,
            &snap(9999, false, Some(60)),
            NOON,
            None,
            Some(120), // wake 120s ago — within grace window
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
            Some(700), // > 600 grace window
        )
        .unwrap_err();
        match action {
            LoopAction::Skip(msg) => assert!(msg.contains("cooldown")),
            other => panic!("expected Skip after grace, got {:?}", other),
        }
    }

    #[test]
    fn wake_recent_halves_idle_threshold() {
        // idle_threshold defaults to 60 (clamped); halved still 60. Bump it up to see the
        // halving effect: threshold = 200 → halved 100 → idle 120 should pass.
        let mut c = cfg();
        c.idle_threshold_seconds = 200;
        let result = evaluate_pre_input_idle(
            &c,
            &snap(120, false, None),
            NOON,
            None,
            Some(60),
        );
        assert!(result.is_ok(), "wake should halve threshold so idle 120 passes 200/2 = 100");
    }

    #[test]
    fn wake_idle_floor_60s() {
        // Even with wake softening, threshold floors at 60s. idle 30 still fails.
        let mut c = cfg();
        c.idle_threshold_seconds = 100;
        let action = evaluate_pre_input_idle(
            &c,
            &snap(30, false, None),
            NOON,
            None,
            Some(60),
        )
        .unwrap_err();
        assert_eq!(action, LoopAction::Silent { reason: "idle_below_threshold" });
    }

    #[test]
    fn wake_does_not_bypass_awaiting() {
        // Even after a wake, if there's an unanswered proactive message we still wait.
        // Awaiting is about respect, not time.
        let action = evaluate_pre_input_idle(
            &cfg(),
            &snap(9999, true, None),
            NOON,
            None,
            Some(60),
        )
        .unwrap_err();
        match action {
            LoopAction::Skip(msg) => assert!(msg.contains("awaiting user reply")),
            other => panic!("expected awaiting Skip, got {:?}", other),
        }
    }

    #[test]
    fn wake_does_not_bypass_quiet_hours() {
        // Wake during 03:00 still respects quiet_hours — user explicitly opted into "let
        // me sleep at night".
        let mut c = cfg();
        c.quiet_hours_start = 23;
        c.quiet_hours_end = 7;
        let action = evaluate_pre_input_idle(
            &c,
            &snap(9999, false, None),
            3,
            None,
            Some(60),
        )
        .unwrap_err();
        assert_eq!(action, LoopAction::Silent { reason: "quiet_hours" });
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
        // Non-macOS: user_input_idle_seconds returns None, gate should not block.
        let action = evaluate_input_idle_gate(&cfg(), &snap(9999, false, None), None);
        match action {
            LoopAction::Run { input_idle_seconds, .. } => {
                assert_eq!(input_idle_seconds, None);
            }
            other => panic!("expected Run, got {:?}", other),
        }
    }

    #[test]
    fn input_idle_below_min_skips() {
        // input_idle_min=60, observed=10 — user is actively typing.
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
            LoopAction::Run { idle_seconds, input_idle_seconds } => {
                assert_eq!(idle_seconds, 9999);
                assert_eq!(input_idle_seconds, Some(120));
            }
            other => panic!("expected Run, got {:?}", other),
        }
    }
}
