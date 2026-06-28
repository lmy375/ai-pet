//! `chat` — a proactive message from the pet into the owner's main chat session.
//!
//! Offered only to scheduled heartbeat sessions (see `ToolRegistry::new`), which
//! run silently in the background and have no streamed reply the owner can see.
//! Calling it appends a pet message to the currently-active session on disk,
//! fires a native system notification, and tells the active window to refresh.

use crate::commands::session;
use crate::tools::{tool_error, Tool, ToolContext};

pub struct ChatTool;

impl Tool for ChatTool {
    fn name(&self) -> &str {
        "chat"
    }

    fn definition(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "chat",
                "description": "Send a message to the owner in their main chat conversation, as the pet. Use this when a heartbeat needs to reach the owner — a reminder, a heads-up, or just proactively saying something. The message is inserted into the active conversation AND a system notification pops up, so the owner sees it even if the app isn't focused. This is the ONLY way a heartbeat reaches the owner; a normal reply is invisible to them. Keep it short and in the pet's voice. Don't call it when there's nothing worth interrupting the owner for.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "message": {
                            "type": "string",
                            "description": "The message to send to the owner, in the pet's voice."
                        }
                    },
                    "required": ["message"]
                }
            }
        })
    }

    crate::impl_execute!(chat_impl);
}

async fn chat_impl(arguments: &str, ctx: &ToolContext) -> String {
    let args = super::parse_args(arguments);
    let message = args["message"].as_str().unwrap_or("").trim().to_string();
    if message.is_empty() {
        return tool_error("missing 'message' parameter");
    }

    let app = match &ctx.app {
        Some(a) => a.clone(),
        None => return tool_error("chat tool unavailable in this context"),
    };

    // Target the currently-active conversation. If there isn't one yet, there's
    // nowhere to deliver — report it rather than guessing.
    let index = session::list_sessions();
    let id = index.active_id;
    if id.is_empty() {
        return tool_error("no active session to send to");
    }
    let mut sess = match session::load_session(id.clone()) {
        Ok(s) => s,
        Err(e) => return tool_error(format!("failed to load session: {}", e)),
    };

    // Append a pet message to BOTH the raw LLM messages (so the owner's next turn
    // sees what the pet said) and the rendered items (so the UI shows it).
    let ts = chrono::Local::now().timestamp_millis();
    sess.messages.push(serde_json::json!({ "role": "assistant", "content": message }));
    let mut item = session::assistant_item(&message, &[]);
    item["ts"] = serde_json::json!(ts);
    sess.items.push(item);
    sess.updated_at = crate::common::iso_now();
    sess.created_at = String::new(); // preserved by save_session

    if let Err(e) = session::save_session(sess) {
        return tool_error(format!("failed to save session: {}", e));
    }

    // Native system notification so the owner sees it even when the app is in
    // the background.
    {
        use tauri_plugin_notification::NotificationExt;
        if let Err(e) = app.notification().builder().title("宠物").body(&message).show() {
            ctx.log(&format!("chat: failed to show notification: {}", e));
        }
    }

    // Tell the active window to reload the conversation so the message appears
    // immediately (routed like background-finished — to whichever window the
    // owner is looking at; the other picks it up on next focus).
    {
        use tauri::Emitter;
        let label = crate::commands::window::active_window_label(&app);
        if let Err(e) = app.emit_to(&label, "chat-inserted", serde_json::json!({ "sessionId": id })) {
            ctx.log(&format!("chat: failed to emit chat-inserted: {}", e));
        }
    }

    r#"{"status": "sent"}"#.to_string()
}
