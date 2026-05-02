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
use crate::commands::session;
use crate::commands::settings::{get_settings, get_soul};
use crate::commands::shell::ShellStore;
use crate::config::AiConfig;
use crate::input_idle::user_input_idle_seconds;
use crate::mcp::McpManagerStore;
use crate::tools::ToolContext;

/// Tracks the last interaction time. Updated whenever the user sends a message
/// or the pet speaks (proactively or reactively).
pub struct InteractionClock {
    last: TokioMutex<Instant>,
}

impl InteractionClock {
    pub fn new() -> Self {
        Self { last: TokioMutex::new(Instant::now()) }
    }

    pub async fn touch(&self) {
        *self.last.lock().await = Instant::now();
    }

    pub async fn idle_seconds(&self) -> u64 {
        self.last.lock().await.elapsed().as_secs()
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
            let clock = app.state::<InteractionClockStore>().inner().clone();
            let idle = clock.idle_seconds().await;

            if idle >= threshold {
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
                    if let Err(e) = run_proactive_turn(&app, idle, input_idle).await {
                        eprintln!("Proactive turn failed: {}", e);
                    }
                } else {
                    let secs = input_idle.unwrap_or(0);
                    let log_store = app.state::<LogStore>().inner().clone();
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

    let prompt = format!(
        "[系统提示·主动开口检查]\n\n现在是 {time}。距离上次和用户互动已经过去约 {minutes} 分钟。{input_hint}\n\n\
请判断：作为陪伴用户的 AI 宠物，此时此刻你想主动跟用户说点什么吗？可以是关心、闲聊、提醒、分享想法都行。\n\n\
约束：\n\
- 如果你判断**不打扰**用户更好（比如只是想保持安静），只回复一个标记：`{silent}`，不要其他任何文字。\n\
- 如果决定开口，就直接说话，不要解释自己为什么开口，也不要包含 `{silent}`。\n\
- 只说一句话，简短自然，像伙伴一样。\n\
- 必要时可以调用工具：`get_active_window` 看看用户在用什么 app（开口前优先调一次，让话题更贴合当下），`memory_search` 翻一下记忆里相关的用户偏好。",
        time = now_local.format("%Y-%m-%d %H:%M"),
        minutes = idle_minutes,
        input_hint = input_hint,
        silent = SILENT_MARKER,
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

    clock.touch().await;

    let payload = ProactiveMessage {
        text: reply_trimmed.to_string(),
        timestamp: now_local.format("%Y-%m-%dT%H:%M:%S%.3f").to_string(),
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
