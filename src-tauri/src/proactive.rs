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

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Mutex as TokioMutex;

use crate::commands::chat::{run_chat_pipeline, ChatMessage, CollectingSink};
use crate::commands::debug::{write_log, LogStore};
use crate::commands::memory;
use crate::commands::session;
use crate::commands::settings::{get_settings, get_soul};
use crate::commands::shell::ShellStore;
use crate::config::AiConfig;
use crate::input_idle::user_input_idle_seconds;
use crate::mcp::McpManagerStore;
use crate::tools::ToolContext;

/// Memory category + title where the pet's evolving mood/state is stored. Read on every
/// proactive turn for context, and the LLM is instructed to update it via `memory_edit`
/// after speaking so personality state persists across iterations.
const MOOD_CATEGORY: &str = "ai_insights";
const MOOD_TITLE: &str = "current_mood";

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
}

const SILENT_MARKER: &str = "<silent>";

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

            if !settings.proactive.enabled {
                tokio::time::sleep(Duration::from_secs(interval)).await;
                continue;
            }

            let threshold = settings.proactive.idle_threshold_seconds.max(60);
            let input_idle_min = settings.proactive.input_idle_seconds;
            let cooldown = settings.proactive.cooldown_seconds;
            let clock = app.state::<InteractionClockStore>().inner().clone();
            let snap = clock.snapshot().await;
            let log_store = app.state::<LogStore>().inner().clone();

            // Gate 1: if the previous proactive utterance hasn't been replied to, stay quiet.
            // A real friend doesn't keep talking when ignored.
            if snap.awaiting_user_reply {
                write_log(
                    &log_store.0,
                    "Proactive: skip — awaiting user reply to previous proactive message",
                );
                tokio::time::sleep(Duration::from_secs(interval)).await;
                continue;
            }

            // Gate 2: cooldown since the last proactive utterance, regardless of idle.
            if let (Some(since), min) = (snap.since_last_proactive_seconds, cooldown) {
                if min > 0 && since < min {
                    write_log(
                        &log_store.0,
                        &format!(
                            "Proactive: skip — cooldown ({}s < {}s)",
                            since, min
                        ),
                    );
                    tokio::time::sleep(Duration::from_secs(interval)).await;
                    continue;
                }
            }

            if snap.idle_seconds >= threshold {
                // Don't interrupt if the user is actively at the keyboard/mouse.
                // A value of 0 disables this gate; on non-macOS we get None and
                // proceed (fall back to interaction-time check only).
                let input_idle = user_input_idle_seconds().await;
                let input_ok = match (input_idle_min, input_idle) {
                    (0, _) => true,
                    (_, Some(secs)) => secs >= input_idle_min,
                    (_, None) => true,
                };

                if input_ok {
                    if let Err(e) = run_proactive_turn(&app, snap.idle_seconds, input_idle).await {
                        eprintln!("Proactive turn failed: {}", e);
                    }
                } else {
                    let secs = input_idle.unwrap_or(0);
                    write_log(
                        &log_store.0,
                        &format!(
                            "Proactive: skip — user active (input_idle={}s < {}s)",
                            secs, input_idle_min
                        ),
                    );
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
    let clock = app.state::<InteractionClockStore>().inner().clone();

    let ctx = ToolContext::new(log_store, shell_store);

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

    let mood_hint = match read_current_mood() {
        Some(m) if !m.trim().is_empty() => format!("你上次记录的心情/状态：「{}」。", m.trim()),
        _ => "（还没有记录过你自己的心情/状态。这是第一次。）".to_string(),
    };

    let prompt = format!(
        "[系统提示·主动开口检查]\n\n\
现在是 {time}。距离上次和用户互动已经过去约 {minutes} 分钟。{input_hint}\n\n\
{mood_hint}\n\n\
请判断：作为陪伴用户的 AI 宠物，此时此刻你想主动跟用户说点什么吗？可以是关心、闲聊、提醒、分享想法都行。\n\n\
约束：\n\
- 如果你判断**不打扰**用户更好（比如只是想保持安静），只回复一个标记：`{silent}`，不要其他任何文字。\n\
- 如果决定开口，就直接说话，不要解释自己为什么开口，也不要包含 `{silent}`。\n\
- 只说一句话，简短自然，像伙伴一样。\n\
- 必要时可以调用工具：`get_active_window`（看用户在用什么 app，开口前优先调一次让话题贴合当下）、`memory_search`（翻一下用户偏好）。\n\
- **决定开口后**：请用 `memory_edit` 更新 `{mood_cat}` 类别下 `{mood_title}` 的记忆（不存在就 `create`，存在就 `update`）。description 用一句话写下你此刻的心情、最近在想什么、对用户的牵挂——这样下次主动开口时你能记得自己刚才的状态，让人格保持连贯。沉默时无需更新。",
        time = now_local.format("%Y-%m-%d %H:%M"),
        minutes = idle_minutes,
        input_hint = input_hint,
        mood_hint = mood_hint,
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

    let payload = ProactiveMessage {
        text: reply_trimmed.to_string(),
        timestamp: now_local.format("%Y-%m-%dT%H:%M:%S%.3f").to_string(),
    };
    let _ = app.emit("proactive-message", payload);

    Ok(())
}

/// Read the pet's current mood/state from memory (`ai_insights/current_mood`). Returns the
/// item's description if present, otherwise `None`. The LLM bootstraps this on first
/// proactive turn via `memory_edit` — we never write it from Rust to keep the source of
/// truth in the model's hands.
fn read_current_mood() -> Option<String> {
    let index = memory::memory_list(Some(MOOD_CATEGORY.to_string())).ok()?;
    let cat = index.categories.get(MOOD_CATEGORY)?;
    cat.items
        .iter()
        .find(|i| i.title == MOOD_TITLE)
        .map(|i| i.description.clone())
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
