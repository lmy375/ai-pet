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

/// Pure-data gates that don't need IO. Returns `Err(action)` with the final LoopAction to
/// short-circuit the tick, or `Ok(())` signaling "all sync gates passed, caller should run
/// the input-idle gate next". Inputs:
/// - `hour`: local 24-hour clock (0–23), injected for testability
/// - `focus_active`: macOS Focus state, `None` means unknown/non-macOS (gate is no-op)
fn evaluate_pre_input_idle(
    cfg: &crate::commands::settings::ProactiveConfig,
    snap: &ClockSnapshot,
    hour: u8,
    focus_active: Option<bool>,
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
    if let Some(since) = snap.since_last_proactive_seconds {
        if cfg.cooldown_seconds > 0 && since < cfg.cooldown_seconds {
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
    // (this is the common idle-yet case, not worth logging on every tick).
    let threshold = cfg.idle_threshold_seconds.max(60);
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

    if let Err(action) = evaluate_pre_input_idle(cfg, &snap, hour, focus_active) {
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

    let mood_hint = match read_current_mood_parsed() {
        Some((text, _)) if !text.trim().is_empty() => {
            format!("你上次记录的心情/状态：「{}」。", text.trim())
        }
        _ => "（还没有记录过你自己的心情/状态。这是第一次。）".to_string(),
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

    let prompt = format!(
        "[系统提示·主动开口检查]\n\n\
现在是 {time}（{period}）。距离上次和用户互动已经过去约 {minutes} 分钟。{input_hint}\n\n\
{mood_hint}\n\
{focus_hint}\n\
请判断：作为陪伴用户的 AI 宠物，此时此刻你想主动跟用户说点什么吗？可以是关心、闲聊、提醒、分享想法都行。\n\n\
约束：\n\
- 如果你判断**不打扰**用户更好（比如只是想保持安静），只回复一个标记：`{silent}`，不要其他任何文字。\n\
- 如果决定开口，就直接说话，不要解释自己为什么开口，也不要包含 `{silent}`。\n\
- 只说一句话，简短自然，像伙伴一样。\n\
- 必要时可以调用工具：`get_active_window`（看用户在用什么 app，开口前优先调一次让话题贴合当下）、`get_upcoming_events`（看用户接下来几小时有没有日程，可用于提醒类话题，记得日程是私人内容不要原样念出）、`get_weather`（看下天气当作闲聊话题，偶尔用一次就好不要每次都查）、`memory_search`（翻一下用户偏好）。\n\
- 这三个环境工具（`get_active_window` / `get_weather` / `get_upcoming_events`）每次调用都有真实的 IO 成本，并且**同一次主动开口检查内重复调用同样的参数会拿到完全一样的结果**——所以一次足够了，不要为了「再确认一下」反复调，相信首次返回值直接做判断。\n\
- **决定开口后**：请用 `memory_edit` 更新 `{mood_cat}` 类别下 `{mood_title}` 的记忆（不存在就 `create`，存在就 `update`）。description 必须以这种格式开头：`[motion: X] 你此刻的心情和想法`，其中 X 是你想做的 Live2D 动作分组，从这四个里选一个：`Tap`（开心/活泼/兴奋）、`Flick`（想分享/有兴致/活力）、`Flick3`（焦虑/烦躁/不安）、`Idle`（平静/低落/累/沉静）。前缀后面才是自由文字。例：`[motion: Tap] 看用户在专心写代码，有点替他高兴`。沉默时无需更新。",
        time = now_local.format("%Y-%m-%d %H:%M"),
        period = period,
        minutes = idle_minutes,
        input_hint = input_hint,
        mood_hint = mood_hint,
        focus_hint = focus_hint,
        silent = SILENT_MARKER,
        mood_cat = MOOD_CATEGORY,
        mood_title = MOOD_TITLE,
    );

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
        let action = evaluate_pre_input_idle(&c, &snap(9999, false, None), NOON, None);
        assert_eq!(action.unwrap_err(), LoopAction::Silent { reason: "disabled" });
    }

    #[test]
    fn awaiting_user_reply_skips_with_log() {
        let action =
            evaluate_pre_input_idle(&cfg(), &snap(9999, true, None), NOON, None).unwrap_err();
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
            evaluate_pre_input_idle(&c, &snap(9999, false, Some(60)), NOON, None).unwrap_err();
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
        let result = evaluate_pre_input_idle(&c, &snap(9999, false, Some(0)), NOON, None);
        assert!(result.is_ok(), "expected Ok, got {:?}", result.unwrap_err());
    }

    #[test]
    fn cooldown_elapsed_passes() {
        let mut c = cfg();
        c.cooldown_seconds = 1800;
        let result = evaluate_pre_input_idle(&c, &snap(9999, false, Some(2000)), NOON, None);
        assert!(result.is_ok());
    }

    #[test]
    fn idle_below_threshold_silent() {
        let c = cfg(); // threshold=60
        let action = evaluate_pre_input_idle(&c, &snap(30, false, None), NOON, None).unwrap_err();
        assert_eq!(action, LoopAction::Silent { reason: "idle_below_threshold" });
    }

    #[test]
    fn idle_threshold_clamped_to_60_minimum() {
        let mut c = cfg();
        c.idle_threshold_seconds = 10; // user-set absurdly low, should clamp up to 60
        let action = evaluate_pre_input_idle(&c, &snap(30, false, None), NOON, None).unwrap_err();
        assert_eq!(action, LoopAction::Silent { reason: "idle_below_threshold" },
            "30s should still be below the clamped 60s");
    }

    #[test]
    fn all_sync_gates_pass_returns_ok() {
        let result = evaluate_pre_input_idle(&cfg(), &snap(9999, false, None), NOON, None);
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
        let action = evaluate_pre_input_idle(&c, &snap(9999, false, None), 3, None).unwrap_err();
        assert_eq!(action, LoopAction::Silent { reason: "quiet_hours" });
    }

    #[test]
    fn quiet_hours_passes_outside_window() {
        let mut c = cfg();
        c.quiet_hours_start = 23;
        c.quiet_hours_end = 7;
        let result = evaluate_pre_input_idle(&c, &snap(9999, false, None), 14, None);
        assert!(result.is_ok());
    }

    #[test]
    fn quiet_hours_disabled_does_not_block() {
        let c = cfg(); // both = 0 → disabled
        let result = evaluate_pre_input_idle(&c, &snap(9999, false, None), 3, None);
        assert!(result.is_ok(), "disabled quiet hours shouldn't gate");
    }

    // ---- focus-mode gate ----

    #[test]
    fn focus_mode_active_skips_when_respected() {
        let action = evaluate_pre_input_idle(&cfg(), &snap(9999, false, None), NOON, Some(true))
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
        let result = evaluate_pre_input_idle(&c, &snap(9999, false, None), NOON, Some(true));
        assert!(result.is_ok(), "user opted out of focus respect");
    }

    #[test]
    fn focus_mode_inactive_passes() {
        let result = evaluate_pre_input_idle(&cfg(), &snap(9999, false, None), NOON, Some(false));
        assert!(result.is_ok());
    }

    #[test]
    fn focus_mode_unknown_passes() {
        // Non-macOS or unreadable file → None → don't block (fail open).
        let result = evaluate_pre_input_idle(&cfg(), &snap(9999, false, None), NOON, None);
        assert!(result.is_ok());
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
