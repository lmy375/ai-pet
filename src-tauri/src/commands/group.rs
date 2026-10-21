//! Tauri side of the multi-agent group chat: thin commands over
//! `pet_core::group` plus the window-event implementation of `GroupEvents`.
//! Every event carries the room id so the panel can ignore other rooms.

use std::sync::Arc;

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, State};

use pet_core::chat::StreamEvent;
use pet_core::group::{GroupEvents, GroupMessage, GroupRuntime, GroupState, GroupSummary};
use pet_core::logging::LogStore;
use pet_core::mcp::McpStore;
use pet_core::shell::ShellStore;

/// Tauri-managed handle to the group runtime.
pub struct GroupStore(pub Arc<GroupRuntime>);

/// Broadcasts group activity to the windows (only the panel's group view
/// listens). Global emits — the pet window simply ignores them.
struct TauriGroupEvents {
    app: AppHandle,
}

impl GroupEvents for TauriGroupEvents {
    fn message(&self, group_id: &str, msg: &GroupMessage) {
        let _ = self
            .app
            .emit("group-message", json!({ "groupId": group_id, "message": msg }));
    }

    fn stream(&self, group_id: &str, agent_id: &str, event: &StreamEvent) {
        let _ = self.app.emit(
            "group-stream",
            json!({ "groupId": group_id, "agentId": agent_id, "event": event }),
        );
    }

    fn injected(&self, group_id: &str, agent_id: &str, items: &[Value]) {
        let _ = self.app.emit(
            "group-injected",
            json!({ "groupId": group_id, "agentId": agent_id, "items": items }),
        );
    }

    fn agent_done(&self, group_id: &str, agent_id: &str) {
        let _ = self.app.emit(
            "group-agent-done",
            json!({ "groupId": group_id, "agentId": agent_id }),
        );
    }
}

/// Build the managed store. Called once in `lib.rs` setup (the event sink needs
/// the app handle).
pub fn new_group_store(
    app: AppHandle,
    mcp_store: McpStore,
    log_store: LogStore,
    shell_store: ShellStore,
) -> GroupStore {
    let events = Arc::new(TauriGroupEvents { app });
    GroupStore(Arc::new(GroupRuntime::new(events, mcp_store, log_store, shell_store)))
}

#[tauri::command]
pub async fn group_list(store: State<'_, GroupStore>) -> Result<Vec<GroupSummary>, String> {
    Ok(store.0.list())
}

#[tauri::command]
pub async fn group_create(
    title: String,
    topic: String,
    members: Vec<String>,
    store: State<'_, GroupStore>,
) -> Result<GroupState, String> {
    store.0.create(&title, &topic, members).await
}

#[tauri::command]
pub async fn group_load(id: String, store: State<'_, GroupStore>) -> Result<GroupState, String> {
    store.0.load(&id).await
}

/// Rename a room, set its topic and its participating agents. Creates private
/// state for new members and drops state for removed ones.
#[tauri::command]
pub async fn group_configure(
    id: String,
    title: String,
    topic: String,
    members: Vec<String>,
    store: State<'_, GroupStore>,
) -> Result<(), String> {
    store.0.configure(&id, &title, &topic, members).await
}

#[tauri::command]
pub async fn group_delete(id: String, store: State<'_, GroupStore>) -> Result<(), String> {
    store.0.delete(&id).await
}

/// Pause (stop ALL in-flight loops) or resume a room.
#[tauri::command]
pub async fn group_set_paused(
    id: String,
    paused: bool,
    app: AppHandle,
    store: State<'_, GroupStore>,
) -> Result<(), String> {
    store.0.set_paused(&id, paused).await?;
    let _ = app.emit(
        if paused { "group-paused" } else { "group-resumed" },
        json!({ "groupId": id }),
    );
    Ok(())
}

/// Clear a room's transcript and every agent's private context (keeps membership).
#[tauri::command]
pub async fn group_reset(
    id: String,
    app: AppHandle,
    store: State<'_, GroupStore>,
) -> Result<(), String> {
    store.0.reset(&id).await?;
    let _ = app.emit("group-reset", json!({ "groupId": id }));
    Ok(())
}

/// The owner sends a message into a room.
#[tauri::command]
pub async fn group_send(
    id: String,
    content: String,
    store: State<'_, GroupStore>,
) -> Result<(), String> {
    store.0.send(&id, &content).await
}
