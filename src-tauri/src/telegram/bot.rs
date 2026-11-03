use base64::Engine;
use std::sync::{Arc, Mutex};
use teloxide::dispatching::ShutdownToken;
use teloxide::net::Download;
use teloxide::prelude::*;
use teloxide::types::{ChatAction, InputFile, Me};
use tokio::sync::mpsc;

use pet_core::chat::{StreamEvent, UserTurn};
use pet_core::logging::{write_log, LogStore};
use pet_core::session;
use pet_core::settings::TelegramConfig;
use pet_core::turn::{TurnEvents, TurnNotice, TurnRunner};

/// A running Telegram bot instance.
pub struct TelegramBot {
    shutdown_token: ShutdownToken,
}

/// Persistent state shared across message handlers.
struct HandlerState {
    /// Which agent this bot belongs to — turns are resolved for this agent,
    /// regardless of which one is active in the GUI.
    agent_id: String,
    allowed_username: String,
    /// The dedicated `telegram-<agent_id>` session, rewritten through the same
    /// runner the GUI uses so both interfaces serialize on one file.
    session_id: String,
    turns: Arc<TurnRunner>,
    log_store: LogStore,
}

const TELEGRAM_MSG_LIMIT: usize = 4096;

impl TelegramBot {
    pub async fn start(
        agent_id: String,
        config: TelegramConfig,
        turns: Arc<TurnRunner>,
        log_store: LogStore,
    ) -> Result<Self, String> {
        let bot = Bot::new(&config.bot_token);

        // Verify bot token by calling getMe
        let _me: Me = bot.get_me().await.map_err(|e| format!("Telegram bot auth failed: {}", e))?;

        let state = Arc::new(HandlerState {
            session_id: ensure_session(&agent_id)?,
            agent_id,
            allowed_username: config.allowed_username.trim_start_matches('@').to_lowercase(),
            turns,
            log_store,
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
            // shutdown() errors if the dispatcher already stopped (e.g. a double
            // stop, or it exited on its own). That's benign — don't panic the task.
            if let Ok(fut) = token.shutdown() {
                fut.await;
            }
        });
    }
}

/// This agent's dedicated Telegram session, created (seeded with its SOUL
/// system message) when missing. Only the id is resolved here: every turn's
/// transcript and persistence go through the shared [`TurnRunner`].
fn ensure_session(agent_id: &str) -> Result<String, String> {
    let session_id = format!("telegram-{}", agent_id);
    match session::load_session(session_id.clone()) {
        Ok(s) => Ok(s.id),
        Err(_) => session::new_seeded_session(agent_id, session_id, "Telegram".to_string())
            .map(|s| s.id),
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
                    write_log(
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

    let turn = UserTurn { text, images: image_urls };

    // The runner owns the turn: it persists the user item up front, streams the
    // reply to the sink, and reloads/appends/persists the session when done.
    let (wake_tx, mut wake_rx) = mpsc::unbounded_channel();
    let sink = Arc::new(TelegramSink::new(wake_tx));
    if let Err(e) = state
        .turns
        .send_for_agent(&state.session_id, &state.agent_id, sink.clone(), turn)
    {
        bot.send_message(msg.chat.id, format!("Error: {}", e)).await?;
        return Ok(());
    }

    // Wait for the turn to finish; the sink has folded the whole stream.
    while !sink.is_done() {
        if wake_rx.recv().await.is_none() {
            break;
        }
    }
    let reply = sink.take();

    if !reply.reasoning.is_empty() {
        // Thinking is kept in the session's items but never sent to chat —
        // Telegram has no collapsed "thinking" block.
        write_log(
            &state.log_store.0,
            &format!(
                "Telegram: {} chars of reasoning not sent",
                reply.reasoning.chars().count()
            ),
        );
    }

    // Text reply first (split if it exceeds Telegram's per-message limit).
    // Skip if empty and we have images — the photos carry the response.
    if let Some(err) = reply.error {
        bot.send_message(msg.chat.id, format!("Error: {}", err)).await?;
    } else if !reply.text.is_empty() {
        for chunk in split_message(&reply.text, TELEGRAM_MSG_LIMIT) {
            bot.send_message(msg.chat.id, chunk).await?;
        }
    } else if reply.images.is_empty() {
        bot.send_message(msg.chat.id, "(no response)").await?;
    }

    // Send any images the pipeline produced as photos.
    for data_url in &reply.images {
        match data_url_to_bytes(data_url) {
            Some(bytes) => {
                bot.send_photo(msg.chat.id, InputFile::memory(bytes)).await?;
            }
            None => {
                write_log(
                    &state.log_store.0,
                    "Telegram: failed to decode outgoing image data URL",
                );
            }
        }
    }

    Ok(())
}

/// Everything one turn streamed, folded into a single Telegram reply.
#[derive(Default)]
struct TelegramReply {
    /// Accumulated answer chunks, sent when the turn finishes.
    text: String,
    /// Accumulated chain-of-thought. Kept for parity with the panel's fold, but
    /// never sent to chat.
    reasoning: String,
    /// Data URLs of images a tool produced, sent after the answer.
    images: Vec<String>,
    error: Option<String>,
    done: bool,
}

/// Folds one turn's notices for the Telegram handler. Messages aren't streamed
/// (no edit-per-chunk): chunks are accumulated and sent once at `done`.
struct TelegramSink {
    reply: Mutex<TelegramReply>,
    wake: mpsc::UnboundedSender<()>,
}

impl TelegramSink {
    fn new(wake: mpsc::UnboundedSender<()>) -> Self {
        Self { reply: Mutex::new(TelegramReply::default()), wake }
    }

    fn is_done(&self) -> bool {
        self.reply.lock().unwrap().done
    }

    fn take(&self) -> TelegramReply {
        std::mem::take(&mut *self.reply.lock().unwrap())
    }
}

impl TurnEvents for TelegramSink {
    fn notice(&self, notice: &TurnNotice) {
        {
            let mut reply = self.reply.lock().unwrap();
            match notice {
                TurnNotice::Stream { event, .. } => match event {
                    StreamEvent::Chunk { text } => reply.text.push_str(text),
                    StreamEvent::Reasoning { text } => reply.reasoning.push_str(text),
                    StreamEvent::Image { data_url } => reply.images.push(data_url.clone()),
                    StreamEvent::Error { message } => reply.error = Some(message.clone()),
                    _ => {}
                },
                TurnNotice::Finished { .. } => reply.done = true,
                TurnNotice::Started { .. } => {}
            }
        }
        let _ = self.wake.send(());
    }
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

fn split_message(text: &str, max_len: usize) -> Vec<&str> {
    let mut result = Vec::new();
    let mut start = 0;
    while start < text.len() {
        let mut end = std::cmp::min(start + max_len, text.len());
        // `start + max_len` can land mid-codepoint (CJK/emoji are multi-byte);
        // walk back to a char boundary so the slices below never panic.
        while end < text.len() && !text.is_char_boundary(end) {
            end -= 1;
        }
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