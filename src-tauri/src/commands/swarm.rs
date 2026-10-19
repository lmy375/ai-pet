//! Tauri bridge for the task-oriented swarm runtime.

use std::sync::Arc;

use serde_json::json;
use tauri::{AppHandle, Emitter, State};

use pet_core::chat::{StreamEvent, UserTurn};
use pet_core::logging::LogStore;
use pet_core::mcp::McpStore;
use pet_core::shell::ShellStore;
use pet_core::swarm::{SwarmConfig, SwarmEvents, SwarmRuntime, SwarmSnapshot};

pub struct SwarmStore(pub Arc<SwarmRuntime>);

struct TauriSwarmEvents {
    app: AppHandle,
}

impl SwarmEvents for TauriSwarmEvents {
    fn snapshot(&self, state: &SwarmSnapshot) {
        let _ = self.app.emit("swarm-state", state.clone());
    }

    fn main_stream(&self, swarm_id: &str, event: &StreamEvent) {
        let _ = self.app.emit(
            "swarm-main-stream",
            json!({ "swarmId": swarm_id, "event": event }),
        );
    }

    fn main_done(&self, swarm_id: &str) {
        let _ = self
            .app
            .emit("swarm-main-done", json!({ "swarmId": swarm_id }));
    }

    fn worker_stream(&self, swarm_id: &str, worker_id: &str, event: &StreamEvent) {
        let _ = self.app.emit(
            "swarm-worker-stream",
            json!({ "swarmId": swarm_id, "workerId": worker_id, "event": event }),
        );
    }

    fn worker_status(&self, swarm_id: &str, worker_id: &str, status: &str) {
        let _ = self.app.emit(
            "swarm-worker-status",
            json!({ "swarmId": swarm_id, "workerId": worker_id, "status": status }),
        );
    }
}

pub fn new_swarm_store(
    app: AppHandle,
    mcp_store: McpStore,
    log_store: LogStore,
    shell_store: ShellStore,
) -> SwarmStore {
    let events = Arc::new(TauriSwarmEvents { app });
    let rt = tauri::async_runtime::handle().inner().clone();
    SwarmStore(SwarmRuntime::new(
        events,
        mcp_store,
        log_store,
        shell_store,
        rt,
    ))
}

#[tauri::command]
pub async fn swarm_list(store: State<'_, SwarmStore>) -> Result<Vec<SwarmSnapshot>, String> {
    Ok(store.0.list().await)
}

#[tauri::command]
pub async fn swarm_create(
    title: String,
    main_agent_id: String,
    store: State<'_, SwarmStore>,
) -> Result<SwarmSnapshot, String> {
    store.0.create(title, main_agent_id).await
}

#[tauri::command]
pub async fn swarm_load(id: String, store: State<'_, SwarmStore>) -> Result<SwarmSnapshot, String> {
    store.0.load(&id).await
}

#[tauri::command]
pub async fn swarm_configure(
    id: String,
    config: SwarmConfig,
    store: State<'_, SwarmStore>,
) -> Result<(), String> {
    store.0.update_config(&id, config).await
}

#[tauri::command]
pub async fn swarm_create_task(
    id: String,
    title: String,
    detail: String,
    parent_task_id: Option<String>,
    store: State<'_, SwarmStore>,
) -> Result<pet_core::swarm::SwarmTask, String> {
    store
        .0
        .create_task(&id, title, detail, parent_task_id)
        .await
}

#[tauri::command]
pub async fn swarm_start(id: String, store: State<'_, SwarmStore>) -> Result<(), String> {
    store.0.start(&id).await
}

#[tauri::command]
pub async fn swarm_end(id: String, store: State<'_, SwarmStore>) -> Result<(), String> {
    store.0.end(&id).await
}

#[tauri::command]
pub async fn swarm_stop(id: String, store: State<'_, SwarmStore>) -> Result<(), String> {
    store.0.stop(&id).await
}

#[tauri::command]
pub async fn swarm_send_main(
    id: String,
    turn: UserTurn,
    store: State<'_, SwarmStore>,
) -> Result<(), String> {
    store.0.send_main(&id, turn).await
}

#[tauri::command]
pub async fn swarm_cancel_main(id: String, store: State<'_, SwarmStore>) -> Result<(), String> {
    store.0.cancel_main(&id).await
}
