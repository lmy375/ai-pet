//! Tauri side of the multi-agent group chat: thin commands over
//! `pet_core::group` plus the window-event implementation of `GroupEvents`.

use std::sync::Arc;

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, State};

use pet_core::chat::StreamEvent;
use pet_core::group::{self, GroupEvents, GroupMessage, GroupRuntime, GroupState};
use pet_core::logging::LogStore;
use pet_core::mcp::McpManagerStore;
use pet_core::shell::ShellStore;

/// Tauri-managed handle to the group runtime.
pub struct GroupStore(pub Arc<GroupRuntime>);

/// Broadcasts group activity to the windows (only the panel's group view
/// listens). Global emits — the pet window simply ignores them.
struct TauriGroupEvents {
    app: AppHandle,
}

impl GroupEvents for TauriGroupEvents {
    fn message(&self, msg: &GroupMessage) {
        let _ = self.app.emit("group-message", msg.clone());
    }

    fn stream(&self, agent_id: &str, event: &StreamEvent) {
        let _ = self.app.emit("group-stream", json!({ "agentId": agent_id, "event": event }));
    }

    fn injected(&self, agent_id: &str, items: &[Value]) {
        let _ = self.app.emit("group-injected", json!({ "agentId": agent_id, "items": items }));
    }

    fn agent_done(&self, agent_id: &str) {
        let _ = self.app.emit("group-agent-done", json!({ "agentId": agent_id }));
    }
}

/// Build the managed store, seeded from disk. Called once in `lib.rs` setup
/// (the event sink needs the app handle).
pub fn new_group_store(
    app: AppHandle,
    mcp_store: McpManagerStore,
    log_store: LogStore,
    shell_store: ShellStore,
) -> GroupStore {
    let events = Arc::new(TauriGroupEvents { app });
    GroupStore(Arc::new(GroupRuntime::new(events, mcp_store, log_store, shell_store)))
}

#[tauri::command]
pub async fn group_load(store: State<'_, GroupStore>) -> Result<GroupState, String> {
    Ok(group::load(&store.0).await)
}

/// Set the participating agents. Creates private state for new members and drops
/// state for removed ones.
#[tauri::command]
pub async fn group_set_members(ids: Vec<String>, store: State<'_, GroupStore>) -> Result<(), String> {
    group::set_members(&store.0, ids).await;
    Ok(())
}

/// Pause (stop ALL in-flight loops) or resume the group.
#[tauri::command]
pub async fn group_set_paused(
    paused: bool,
    app: AppHandle,
    store: State<'_, GroupStore>,
) -> Result<(), String> {
    group::set_paused(&store.0, paused).await;
    let _ = app.emit(if paused { "group-paused" } else { "group-resumed" }, ());
    Ok(())
}

/// Clear the transcript and every agent's private context (keeps membership).
#[tauri::command]
pub async fn group_reset(app: AppHandle, store: State<'_, GroupStore>) -> Result<(), String> {
    group::reset(&store.0).await;
    let _ = app.emit("group-reset", ());
    Ok(())
}

/// The owner sends a message into the group.
#[tauri::command]
pub async fn group_send(content: String, store: State<'_, GroupStore>) -> Result<(), String> {
    group::send_user_message(&store.0, &content).await
}
