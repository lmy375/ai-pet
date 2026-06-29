//! `GroupChat` — post a message into the shared group conversation.
//!
//! Offered only to group-page agent runs (see `ToolRegistry::new`). Calling it
//! appends the agent's message to the group transcript and notifies the panel;
//! the running orchestrator then fans it out to the other agents. This is the
//! ONLY way an agent speaks in the group — a normal reply is private to its own
//! session and invisible to the room.

use tauri::Manager;

use crate::commands::group::GroupStore;
use crate::tools::{tool_error, Tool, ToolContext};

pub struct GroupChatTool;

impl Tool for GroupChatTool {
    fn name(&self) -> &str {
        "GroupChat"
    }

    fn definition(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "GroupChat",
                "description": "Post a message into the shared group chat so the owner and the other agents see it. This is the ONLY way you speak in the group — a normal reply stays private to your own session and nobody else sees it. Call this only when you actually have something worth saying; if the topic doesn't concern you or someone already covered it, stay silent and don't call it. Keep it short and natural, like talking in a group chat, in your own voice.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "message": {
                            "type": "string",
                            "description": "What to say in the group, in your own voice."
                        }
                    },
                    "required": ["message"]
                }
            }
        })
    }

    crate::impl_execute!(group_chat_impl);
}

async fn group_chat_impl(arguments: &str, ctx: &ToolContext) -> String {
    let args = super::parse_args(arguments);
    let message = args["message"].as_str().unwrap_or("").trim().to_string();
    if message.is_empty() {
        return tool_error("missing 'message' parameter");
    }

    let app = match &ctx.app {
        Some(a) => a.clone(),
        None => return tool_error("GroupChat unavailable in this context"),
    };

    let agent_id = ctx.config.agent_id.clone();
    let store = app.state::<GroupStore>();
    crate::commands::group::post_agent_message(&app, &store, &agent_id, &message).await;

    r#"{"status": "posted"}"#.to_string()
}
