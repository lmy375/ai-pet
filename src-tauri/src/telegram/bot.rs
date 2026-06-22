use base64::Engine;
use std::sync::Arc;
use teloxide::dispatching::ShutdownToken;
use teloxide::net::Download;
use teloxide::prelude::*;
use teloxide::types::{ChatAction, InputFile, Me};
use tokio::sync::Mutex as TokioMutex;

use crate::commands::chat::{ChatEventSink, ChatMessage, run_chat_pipeline};
use crate::commands::debug::LogStore;
use crate::commands::session;
use crate::commands::settings::TelegramConfig;
use crate::commands::shell::ShellStore;
use crate::config::AiConfig;
use crate::mcp::McpManagerStore;
use crate::tools::ToolContext;

/// A running Telegram bot instance.
pub struct TelegramBot {
    shutdown_token: ShutdownToken,
}

/// Persistent state shared across message handlers.
struct HandlerState {
    allowed_username: String,
    mcp_store: McpManagerStore,
    log_store: LogStore,
    shell_store: ShellStore,
    /// Messages for the dedicated Telegram session (kept in memory for fast access).
    /// This is the model-facing transcript (role/content), persisted as `messages`.
    session_messages: TokioMutex<Vec<serde_json::Value>>,
    /// The display transcript (ChatItem shape), persisted as `items` and kept
    /// independent of `session_messages` — exactly like the panel. Tool-produced
    /// images (e.g. `screenshot`) live here as their own bubbles but never enter
    /// `session_messages`, so they show in history without bloating model context.
    session_items: TokioMutex<Vec<serde_json::Value>>,
    session_id: String,
}

const TELEGRAM_SESSION_ID: &str = "telegram-bot";
const MAX_CONTEXT_MESSAGES: usize = 50;
const TELEGRAM_MSG_LIMIT: usize = 4096;

impl TelegramBot {
    pub async fn start(
        config: TelegramConfig,
        mcp_store: McpManagerStore,
        log_store: LogStore,
        shell_store: ShellStore,
    ) -> Result<Self, String> {
        let bot = Bot::new(&config.bot_token);

        // Verify bot token by calling getMe
        let _me: Me = bot.get_me().await.map_err(|e| format!("Telegram bot auth failed: {}", e))?;

        // Load or create the dedicated Telegram session
        let (session_id, messages, items) = load_or_create_session();

        let state = Arc::new(HandlerState {
            allowed_username: config.allowed_username.trim_start_matches('@').to_lowercase(),
            mcp_store,
            log_store,
            shell_store,
            session_messages: TokioMutex::new(messages),
            session_items: TokioMutex::new(items),
            session_id,
        });

        // Handle every message; `handle_message` extracts text/caption + photos
        // itself and ignores messages with neither (stickers, etc.).
        let handler = Update::filter_message().endpoint(handle_message);

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
            token.shutdown().expect("Failed to shutdown Telegram bot").await;
        });
    }
}

/// Load the dedicated Telegram session, or create one (seeded with the shared
/// SOUL system message, identical to panel/pet sessions) if it doesn't exist.
fn load_or_create_session() -> (String, Vec<serde_json::Value>, Vec<serde_json::Value>) {
    match session::load_session(TELEGRAM_SESSION_ID.to_string()) {
        Ok(s) => (s.id, s.messages, s.items),
        Err(_) => match session::new_seeded_session(TELEGRAM_SESSION_ID.to_string(), "Telegram".to_string()) {
            Ok(s) => (s.id, s.messages, s.items),
            Err(_) => (TELEGRAM_SESSION_ID.to_string(), vec![session::soul_system_message()], vec![]),
        },
    }
}

async fn handle_message(
    bot: Bot,
    msg: Message,
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
        bot.send_message(msg.chat.id, "Sorry, you are not authorized to chat with me.")
            .await?;
        return Ok(());
    }

    // Text comes from a plain text message or a photo's caption.
    let text = msg
        .text()
        .or_else(|| msg.caption())
        .map(|t| t.to_string())
        .unwrap_or_default();

    // Telegram delivers a photo as several pre-scaled `PhotoSize`s; take the
    // largest and inline it as a base64 data URL (the same multimodal format the
    // panel uses for pasted images). Albums arrive as separate messages, one
    // photo each, so a message carries at most one photo here.
    let mut image_urls: Vec<String> = Vec::new();
    if let Some(photos) = msg.photo() {
        if let Some(largest) = photos.iter().max_by_key(|p| p.width * p.height) {
            match download_photo_as_data_url(&bot, &largest.file.id).await {
                Ok(url) => image_urls.push(url),
                Err(e) => {
                    crate::commands::debug::write_log(
                        &state.log_store.0,
                        &format!("Telegram: photo download failed: {}", e),
                    );
                    bot.send_message(msg.chat.id, "Sorry, I couldn't download that image.")
                        .await?;
                    return Ok(());
                }
            }
        }
    }

    // Nothing actionable (e.g. a sticker, location, or other unsupported type).
    if text.is_empty() && image_urls.is_empty() {
        return Ok(());
    }

    // Send typing indicator
    let _ = bot.send_chat_action(msg.chat.id, ChatAction::Typing).await;

    // Build the user message. Plain text stays a bare string (unchanged
    // behavior); with images it becomes an OpenAI multimodal content array.
    let user_content = if image_urls.is_empty() {
        serde_json::Value::String(text.clone())
    } else {
        let mut parts: Vec<serde_json::Value> = Vec::new();
        if !text.is_empty() {
            parts.push(serde_json::json!({ "type": "text", "text": text }));
        }
        for url in &image_urls {
            parts.push(serde_json::json!({ "type": "image_url", "image_url": { "url": url } }));
        }
        serde_json::Value::Array(parts)
    };
    let user_msg = serde_json::json!({ "role": "user", "content": user_content });

    // Mirror the user turn into the display transcript (ChatItem shape).
    state.session_items.lock().await.push(
        serde_json::json!({ "type": "user", "content": text, "images": image_urls }),
    );

    let chat_messages = {
        let mut session_msgs = state.session_messages.lock().await;
        session_msgs.push(user_msg);

        // Build messages for LLM: system prompt + last N messages
        let msgs = &*session_msgs;
        let context_msgs: Vec<serde_json::Value> = if msgs.len() > MAX_CONTEXT_MESSAGES + 1 {
            // Always include system message (first) + last N
            let mut ctx = vec![msgs[0].clone()];
            ctx.extend_from_slice(&msgs[msgs.len() - MAX_CONTEXT_MESSAGES..]);
            ctx
        } else {
            msgs.clone()
        };
        context_msgs
    };

    // Convert to ChatMessage structs
    let chat_messages: Vec<ChatMessage> = chat_messages
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect();

    // Run the LLM pipeline. The sink collects any images a tool surfaces (e.g.
    // `screenshot`) so we can send them back as photos after the text reply.
    let sink = TelegramSink::new();
    let reply_text = match AiConfig::from_settings() {
        Ok(config) => {
            // Telegram has no UI window, so background-task completions can't be
            // auto-pushed there (notifier = None); tasks still run and are checkable.
            let ctx = ToolContext::new(
                state.log_store.clone(),
                state.shell_store.clone(),
                config.clone(),
                state.mcp_store.clone(),
                String::new(),
                None,
                None,
                false,
            );
            match run_chat_pipeline(chat_messages, &sink, &config, &state.mcp_store, &ctx).await {
                Ok(text) => text,
                Err(e) => format!("Error: {}", e),
            }
        }
        Err(e) => format!("Config error: {}", e),
    };
    let reply_images = sink.take_images();

    // Append the assistant turn to both transcripts, then persist.
    {
        let mut session_msgs = state.session_messages.lock().await;
        session_msgs
            .push(serde_json::json!({ "role": "assistant", "content": reply_text.clone() }));

        let mut items = state.session_items.lock().await;
        // Tool-produced images (screenshots) render as assistant bubbles — they
        // come from the pet, not the user — and never enter `session_msgs` (the
        // model already saw them in-loop).
        for url in &reply_images {
            items.push(serde_json::json!({ "type": "assistant", "content": "", "images": [url] }));
        }
        // An empty final text means the response was carried entirely by images;
        // skip the empty assistant bubble (matches the panel's behavior).
        if !reply_text.is_empty() {
            items.push(serde_json::json!({ "type": "assistant", "content": reply_text }));
        }

        // Title from the first user item's text (fall back to "Telegram").
        let title = items
            .iter()
            .find(|i| i["type"] == "user")
            .and_then(|i| i["content"].as_str())
            .filter(|c| !c.is_empty())
            .map(|c| {
                let t = c.chars().take(20).collect::<String>();
                if c.chars().count() > 20 { format!("{}...", t) } else { t }
            })
            .unwrap_or_else(|| "Telegram".to_string());

        let s = session::Session {
            id: state.session_id.clone(),
            title,
            created_at: String::new(), // preserved by backend
            updated_at: crate::common::iso_now(),
            messages: session_msgs.clone(),
            items: items.clone(),
        };
        let _ = session::save_session(s);
    }

    // Send the text reply (split if it exceeds Telegram's per-message limit).
    // Skip if empty and we have images — the photos carry the response.
    if !reply_text.is_empty() {
        if reply_text.len() <= TELEGRAM_MSG_LIMIT {
            bot.send_message(msg.chat.id, &reply_text).await?;
        } else {
            for chunk in split_message(&reply_text, TELEGRAM_MSG_LIMIT) {
                bot.send_message(msg.chat.id, chunk).await?;
            }
        }
    } else if reply_images.is_empty() {
        bot.send_message(msg.chat.id, "(no response)").await?;
    }

    // Send any images the pipeline produced as photos.
    for data_url in &reply_images {
        match data_url_to_bytes(data_url) {
            Some(bytes) => {
                bot.send_photo(msg.chat.id, InputFile::memory(bytes)).await?;
            }
            None => {
                crate::commands::debug::write_log(
                    &state.log_store.0,
                    "Telegram: failed to decode outgoing image data URL",
                );
            }
        }
    }

    Ok(())
}

/// Fetch a Telegram file by id and inline it as a base64 `data:` URL. Telegram
/// re-encodes uploaded photos to JPEG, so the MIME is always `image/jpeg`.
async fn download_photo_as_data_url(bot: &Bot, file_id: &str) -> Result<String, String> {
    let file = bot
        .get_file(file_id.to_string())
        .await
        .map_err(|e| format!("getFile: {}", e))?;
    let mut buf: Vec<u8> = Vec::new();
    bot.download_file(&file.path, &mut buf)
        .await
        .map_err(|e| format!("download: {}", e))?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&buf);
    Ok(format!("data:image/jpeg;base64,{}", b64))
}

/// Decode the base64 payload of a `data:...;base64,...` URL into raw bytes.
fn data_url_to_bytes(data_url: &str) -> Option<Vec<u8>> {
    let comma = data_url.find(',')?;
    base64::engine::general_purpose::STANDARD
        .decode(&data_url[comma + 1..])
        .ok()
}

/// A no-op chat sink that captures images a tool surfaces during the pipeline
/// (via `send_image`) so the Telegram handler can forward them as photos. All
/// other events are discarded — the final assistant text is returned by
/// `run_chat_pipeline` directly.
struct TelegramSink {
    images: std::sync::Mutex<Vec<String>>,
}

impl TelegramSink {
    fn new() -> Self {
        Self { images: std::sync::Mutex::new(Vec::new()) }
    }
    fn take_images(&self) -> Vec<String> {
        std::mem::take(&mut *self.images.lock().unwrap())
    }
}

impl ChatEventSink for TelegramSink {
    fn send_chunk(&self, _text: &str) {}
    fn send_tool_start(&self, _name: &str, _arguments: &str) {}
    fn send_tool_result(&self, _name: &str, _result: &str) {}
    fn send_image(&self, data_url: &str) {
        self.images.lock().unwrap().push(data_url.to_string());
    }
    fn send_done(&self) {}
    fn send_error(&self, _message: &str) {}
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
