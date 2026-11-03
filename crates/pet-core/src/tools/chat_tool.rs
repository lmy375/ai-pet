//! `chat` — a proactive message from the pet into the owner's main chat session.
//!
//! Offered only to scheduled heartbeat sessions (see `ToolRegistry::new`), which
//! run silently in the background and have no streamed reply the owner can see.
//! Calling it appends a pet message to the currently-active session on disk,
//! fires a native system notification, and tells the active window to refresh.

use crate::item::{ChatItem, MessageRange};
use crate::session;
use crate::tools::{parse_args, tool_error, Tool, ToolContext, ToolSpec};

pub struct ChatTool;

#[derive(serde::Deserialize)]
struct ChatArgs {
    message: String,
}

impl Tool for ChatTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "chat",
            description: "Send a message to the owner in their main chat conversation, as the pet. Use this when a heartbeat needs to reach the owner — a reminder, a heads-up, or just proactively saying something. The message is inserted into the active conversation AND a system notification pops up, so the owner sees it even if the app isn't focused. This is the ONLY way a heartbeat reaches the owner; a normal reply is invisible to them. Keep it short and in the pet's voice. Don't call it when there's nothing worth interrupting the owner for.",
            schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "message": {
                        "type": "string",
                        "description": "The message to send to the owner, in the pet's voice."
                    }
                },
                "required": ["message"]
            }),
        }
    }

    crate::impl_execute!(chat_impl);
}

async fn chat_impl(arguments: &str, ctx: &ToolContext) -> String {
    let args: ChatArgs = match parse_args(arguments) {
        Ok(args) => args,
        Err(e) => return e,
    };
    let message = args.message.trim().to_string();
    if message.is_empty() {
        return tool_error("missing 'message' parameter");
    }

    let hook = match &ctx.chat_hook {
        Some(h) => h.clone(),
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
    sess.conv.messages.push(crate::llm::store_message(
        &genai::chat::ChatMessage::assistant(message.clone()),
    ));
    let mut item = ChatItem::assistant(&message, None, &[]);
    item.set_messages_range(Some(MessageRange {
        start: crate::session::stored_message_count(&sess.conv.messages) - 1,
        len: 1,
    }));
    sess.conv.items.push(item);
    sess.updated_at = crate::common::iso_now();
    sess.created_at = String::new(); // preserved by save_session

    if let Err(e) = session::save_session(sess) {
        return tool_error(format!("failed to save session: {}", e));
    }

    // UI side effects (system notification + tell the active view to reload)
    // are the interface layer's job — see `ChatHook`.
    hook.on_chat_inserted(&id, &message);

    r#"{"status": "sent"}"#.to_string()
}
