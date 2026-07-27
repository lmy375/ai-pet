//! Tauri side of the chat pipeline: the `chat` command plus the Tauri
//! implementations of pet-core's delivery traits (stream sink, background-task
//! notifier, heartbeat chat hook).

use std::sync::Arc;

use tauri::ipc::Channel;
use tauri::State;

use pet_core::chat::{run_chat_pipeline, ChatEventSink, ChatMessage, StreamEvent};
use pet_core::config::AiConfig;
use pet_core::logging::LogStore;
use pet_core::mcp::McpManagerStore;
use pet_core::shell::{ShellStore, TaskCompletion, TaskNotifier};
use pet_core::tools::{ChatHook, ToolContext};

/// Streams pipeline events to the frontend over a Tauri channel (newtype —
/// both the trait and `Channel` are foreign to this crate).
struct ChannelSink(Channel<StreamEvent>);

impl ChatEventSink for ChannelSink {
    fn send_chunk(&self, text: &str) {
        let _ = self.0.send(StreamEvent::Chunk { text: text.to_string() });
    }
    fn send_reasoning(&self, text: &str) {
        let _ = self.0.send(StreamEvent::Reasoning { text: text.to_string() });
    }
    fn send_tool_start(&self, name: &str, arguments: &str) {
        let _ = self.0.send(StreamEvent::ToolStart {
            name: name.to_string(),
            arguments: arguments.to_string(),
        });
    }
    fn send_tool_result(&self, name: &str, result: &str) {
        let _ = self.0.send(StreamEvent::ToolResult {
            name: name.to_string(),
            result: result.to_string(),
        });
    }
    fn send_image(&self, data_url: &str) {
        let _ = self.0.send(StreamEvent::Image { data_url: data_url.to_string() });
    }
    fn send_usage(&self, prompt_tokens: u64, total_tokens: u64, context_window: u32) {
        let _ = self.0.send(StreamEvent::Usage { prompt_tokens, total_tokens, context_window });
    }
    fn send_done(&self) {
        let _ = self.0.send(StreamEvent::Done {});
    }
    fn send_error(&self, message: &str) {
        let _ = self.0.send(StreamEvent::Error { message: message.to_string() });
    }
}

/// Emits background-task completions so the conversation can be resumed
/// automatically (see `useChat`'s `background-finished` listener).
///
/// Targets the ACTIVE window only (pet or panel — they share one conversation),
/// so the completion is injected into the window the user is looking at and never
/// into both. Both windows listen; backend routing guarantees a single delivery.
pub struct TauriNotifier {
    pub app: tauri::AppHandle,
}

impl TaskNotifier for TauriNotifier {
    fn notify(&self, completion: &TaskCompletion) {
        emit_background_finished(&self.app, completion.clone());
    }
}

/// Emit `background-finished` to the active window. Shared with `kill_task`,
/// which fires the same event for a manual cancellation.
pub fn emit_background_finished(app: &tauri::AppHandle, completion: TaskCompletion) {
    use tauri::Emitter;
    let label = crate::commands::window::active_window_label(app);
    // If the target window is gone the task still stays in the store
    // (queryable via check_task_status); log rather than silently drop.
    if let Err(e) = app.emit_to(&label, "background-finished", completion.clone()) {
        eprintln!("failed to emit background-finished for task {}: {}", completion.task_id, e);
    }
}

/// UI side of the heartbeat-only `chat` tool: native system notification plus a
/// `chat-inserted` event so the active window reloads the conversation (routed
/// like `background-finished` — to whichever window the owner is looking at; the
/// other picks it up on next focus).
pub struct TauriChatHook {
    pub app: tauri::AppHandle,
}

impl ChatHook for TauriChatHook {
    fn on_chat_inserted(&self, session_id: &str, message: &str) {
        {
            use tauri_plugin_notification::NotificationExt;
            if let Err(e) = self.app.notification().builder().title("宠物").body(message).show() {
                eprintln!("chat: failed to show notification: {}", e);
            }
        }
        {
            use tauri::Emitter;
            let label = crate::commands::window::active_window_label(&self.app);
            let payload = serde_json::json!({ "sessionId": session_id });
            if let Err(e) = self.app.emit_to(&label, "chat-inserted", payload) {
                eprintln!("chat: failed to emit chat-inserted: {}", e);
            }
        }
    }
}

#[tauri::command]
pub async fn chat(
    messages: Vec<ChatMessage>,
    on_event: Channel<StreamEvent>,
    session_id: String,
    app: tauri::AppHandle,
    log_store: State<'_, LogStore>,
    shell_store: State<'_, ShellStore>,
    mcp_store: State<'_, McpManagerStore>,
) -> Result<(), String> {
    let config = AiConfig::from_settings()?;
    let mcp = mcp_store.inner().clone();
    let notifier: Arc<dyn TaskNotifier> = Arc::new(TauriNotifier { app: app.clone() });
    let ctx = ToolContext::new(
        LogStore(log_store.0.clone()),
        ShellStore(shell_store.0.clone()),
        config.clone(),
        mcp.clone(),
        session_id,
        Some(notifier),
        None, // chat turns aren't heartbeats; no chat hook
        false,
    );
    let sink = ChannelSink(on_event);
    run_chat_pipeline(messages, &sink, &config, &mcp, &ctx).await?;
    Ok(())
}
