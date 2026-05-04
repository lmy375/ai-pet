use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use teloxide::dispatching::ShutdownToken;
use teloxide::prelude::*;
use teloxide::types::{ChatAction, Me};
use tokio::sync::Mutex as TokioMutex;

use crate::commands::chat::{
    inject_deadline_context_layer, inject_focus_context_layer, inject_mood_note,
    inject_persona_layer, run_chat_pipeline, trim_to_context, ChatDonePayload, ChatMessage,
    CollectingSink,
};
use crate::commands::debug::{LogStore, ProcessCountersStore};
use crate::commands::session;
use crate::commands::settings::{get_soul, TelegramConfig};
use crate::commands::shell::ShellStore;
use crate::config::AiConfig;
use crate::mcp::McpManagerStore;
use crate::mood::read_mood_for_event;
use crate::tools::ToolContext;

/// A running Telegram bot instance.
pub struct TelegramBot {
    shutdown_token: ShutdownToken,
}

/// Persistent state shared across message handlers.
struct HandlerState {
    allowed_username: String,
    /// Whether the bot's chat pipeline injects the route-A persona layer (Iter 107).
    /// Captured at bot start time from `TelegramConfig.persona_layer_enabled`. Bot
    /// must be restarted for changes to take effect — same lifecycle as bot_token.
    persona_layer_enabled: bool,
    mcp_store: McpManagerStore,
    log_store: LogStore,
    shell_store: ShellStore,
    process_counters: ProcessCountersStore,
    /// Messages for the dedicated Telegram session (kept in memory for fast access).
    session_messages: TokioMutex<Vec<serde_json::Value>>,
    session_id: String,
    /// Used to emit `chat-done` events so the desktop pet's Live2D motion reacts even
    /// when the conversation happened on Telegram.
    app: AppHandle,
}

const TELEGRAM_SESSION_ID: &str = "telegram-bot";
const TELEGRAM_MSG_LIMIT: usize = 4096;

impl TelegramBot {
    pub async fn start(
        config: TelegramConfig,
        mcp_store: McpManagerStore,
        log_store: LogStore,
        shell_store: ShellStore,
        process_counters: ProcessCountersStore,
        app: AppHandle,
    ) -> Result<Self, String> {
        let bot = Bot::new(&config.bot_token);

        // Verify bot token by calling getMe
        let _me: Me = bot
            .get_me()
            .await
            .map_err(|e| format!("Telegram bot auth failed: {}", e))?;

        // Load or create the dedicated Telegram session
        let (session_id, messages) = load_or_create_session();

        let state = Arc::new(HandlerState {
            allowed_username: config
                .allowed_username
                .trim_start_matches('@')
                .to_lowercase(),
            persona_layer_enabled: config.persona_layer_enabled,
            mcp_store,
            log_store,
            shell_store,
            process_counters,
            session_messages: TokioMutex::new(messages),
            session_id,
            app,
        });

        let handler = Update::filter_message()
            .filter_map(|msg: Message| msg.text().map(|t| t.to_string()))
            .endpoint(handle_message);

        let mut dispatcher = Dispatcher::builder(bot, handler)
            .dependencies(dptree::deps![state])
            .enable_ctrlc_handler()
            .build();

        let shutdown_token = dispatcher.shutdown_token();

        tokio::spawn(async move {
            dispatcher.dispatch().await;
        });

        Ok(Self { shutdown_token })
    }

    pub fn stop(&self) {
        // ShutdownToken::shutdown() returns a future, but we just need to signal
        // the shutdown — spawning it is sufficient.
        let token = self.shutdown_token.clone();
        tokio::spawn(async move {
            token
                .shutdown()
                .expect("Failed to shutdown Telegram bot")
                .await;
        });
    }
}

/// Load the dedicated Telegram session, or create one if it doesn't exist.
fn load_or_create_session() -> (String, Vec<serde_json::Value>) {
    match session::load_session(TELEGRAM_SESSION_ID.to_string()) {
        Ok(s) => (s.id, s.messages),
        Err(_) => {
            let soul = get_soul().unwrap_or_default();
            let system_msg = serde_json::json!({ "role": "system", "content": soul });
            let messages = vec![system_msg];

            let now = chrono::Local::now()
                .format("%Y-%m-%dT%H:%M:%S%.3f")
                .to_string();
            let s = session::Session {
                id: TELEGRAM_SESSION_ID.to_string(),
                title: "Telegram".to_string(),
                created_at: now.clone(),
                updated_at: now,
                messages: messages.clone(),
                items: vec![],
            };
            let _ = session::save_session(s);
            (TELEGRAM_SESSION_ID.to_string(), messages)
        }
    }
}

async fn handle_message(
    bot: Bot,
    msg: Message,
    text: String,
    state: Arc<HandlerState>,
) -> ResponseResult<()> {
    // Check allowed username
    let username = msg
        .from
        .as_ref()
        .and_then(|u| u.username.as_ref())
        .map(|u| u.to_lowercase())
        .unwrap_or_default();

    if !state.allowed_username.is_empty() && username != state.allowed_username {
        bot.send_message(
            msg.chat.id,
            "Sorry, you are not authorized to chat with me.",
        )
        .await?;
        return Ok(());
    }

    // Send typing indicator
    let _ = bot.send_chat_action(msg.chat.id, ChatAction::Typing).await;

    // Build ChatMessage list from session history + new user message
    let user_msg = serde_json::json!({ "role": "user", "content": text });

    // Snapshot the full session, then let the shared trim/inject helpers prune to the
    // configured context window. Keeps trim semantics identical between desktop and
    // telegram paths.
    let chat_messages: Vec<serde_json::Value> = {
        let mut session_msgs = state.session_messages.lock().await;
        session_msgs.push(user_msg);
        session_msgs.clone()
    };

    // Convert to ChatMessage structs and inject the same mood-context system note that
    // the desktop chat path uses. This keeps the pet's persona behavior consistent
    // regardless of which surface the user is talking through.
    let chat_messages: Vec<ChatMessage> = chat_messages
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect();
    let max_context = AiConfig::from_settings()
        .map(|c| c.max_context_messages)
        .unwrap_or(50);
    let chat_messages = trim_to_context(chat_messages, max_context);
    let chat_messages = inject_mood_note(chat_messages);
    // Iter 107: optionally inject the route-A persona layer (companionship days /
    // persona summary / mood trend) into the Telegram chat path too. Gated on
    // `telegram.persona_layer_enabled` (captured at bot start) so users who prefer
    // terse Telegram chat can opt out without affecting the desktop chat path.
    let chat_messages = if state.persona_layer_enabled {
        inject_persona_layer(chat_messages).await
    } else {
        chat_messages
    };
    // Iter R71: focus-context parity with reactive chat — telegram users
    // can also ask "我今天怎么样" and the AI now has stats to answer
    // coherently. Modality-agnostic data (today/week aggregates) so cross-
    // device makes sense; recent_speech (R9) deliberately *not* injected
    // because those bubbles were desktop-only — telegram user didn't see
    // them and citing would be confusing.
    let chat_messages = inject_focus_context_layer(chat_messages);
    // Iter R79: deadline parity — telegram user can also ask "我有什么
    // deadline" and AI now has the data. butler_tasks is modality-agnostic
    // (lives in user's persistent memory, not surface-bound).
    let chat_messages = inject_deadline_context_layer(chat_messages);

    // Run the LLM pipeline
    let reply_text = match AiConfig::from_settings() {
        Ok(config) => {
            let ctx = ToolContext::new(
                state.log_store.clone(),
                state.shell_store.clone(),
                state.process_counters.clone(),
            );
            let sink = CollectingSink::new();
            match run_chat_pipeline(chat_messages, &sink, &config, &state.mcp_store, &ctx).await {
                Ok(text) => {
                    // Emit chat-done with the post-turn mood snapshot so the desktop pet's
                    // Live2D motion reflects state changes even when the user was chatting
                    // via Telegram. Same payload shape as the chat tauri command.
                    let (mood, motion) = read_mood_for_event(&ctx, "Telegram");
                    let payload = ChatDonePayload {
                        mood,
                        motion,
                        timestamp: chrono::Local::now()
                            .format("%Y-%m-%dT%H:%M:%S%.3f")
                            .to_string(),
                    };
                    let _ = state.app.emit("chat-done", payload);
                    text
                }
                Err(e) => format!("Error: {}", e),
            }
        }
        Err(e) => format!("Config error: {}", e),
    };

    // Save assistant message to session
    {
        let assistant_msg = serde_json::json!({ "role": "assistant", "content": reply_text });
        let mut session_msgs = state.session_messages.lock().await;
        session_msgs.push(assistant_msg);

        // Persist to disk
        let now = chrono::Local::now()
            .format("%Y-%m-%dT%H:%M:%S%.3f")
            .to_string();
        let items: Vec<serde_json::Value> = session_msgs
            .iter()
            .filter_map(|m| {
                let role = m["role"].as_str()?;
                let content = m["content"].as_str()?;
                match role {
                    "user" => Some(serde_json::json!({ "type": "user", "content": content })),
                    "assistant" => {
                        Some(serde_json::json!({ "type": "assistant", "content": content }))
                    }
                    _ => None,
                }
            })
            .collect();

        let first_user = items.iter().find(|i| i["type"] == "user");
        let title = first_user
            .and_then(|i| i["content"].as_str())
            .map(|c| {
                let t = c.chars().take(20).collect::<String>();
                if c.len() > 20 {
                    format!("{}...", t)
                } else {
                    t
                }
            })
            .unwrap_or_else(|| "Telegram".to_string());

        let s = session::Session {
            id: state.session_id.clone(),
            title,
            created_at: String::new(), // preserved by backend
            updated_at: now,
            messages: session_msgs.clone(),
            items,
        };
        let _ = session::save_session(s);
    }

    // Send reply (split if exceeds Telegram limit)
    if reply_text.len() <= TELEGRAM_MSG_LIMIT {
        bot.send_message(msg.chat.id, &reply_text).await?;
    } else {
        for chunk in split_message(&reply_text, TELEGRAM_MSG_LIMIT) {
            bot.send_message(msg.chat.id, chunk).await?;
        }
    }

    Ok(())
}

fn split_message(text: &str, max_len: usize) -> Vec<&str> {
    let mut result = Vec::new();
    let mut start = 0;
    while start < text.len() {
        let end = std::cmp::min(start + max_len, text.len());
        // Try to split at a newline or space boundary
        let split_at = if end == text.len() {
            end
        } else {
            text[start..end]
                .rfind('\n')
                .or_else(|| text[start..end].rfind(' '))
                .map(|pos| start + pos + 1)
                .unwrap_or(end)
        };
        result.push(&text[start..split_at]);
        start = split_at;
    }
    result
}
