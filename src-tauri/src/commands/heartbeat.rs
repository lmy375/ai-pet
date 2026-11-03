//! Start of the scheduled heartbeat loop. The loop itself (tick, per-agent
//! counters, context fork, task registration) lives in `pet_core::heartbeat`;
//! this only supplies the Tauri chat hook and the app runtime.

use pet_core::logging::LogStore;
use pet_core::mcp::McpStore;
use pet_core::shell::ShellStore;
use pet_core::tools::ChatHook;
use std::sync::Arc;

/// Spawn the heartbeat scheduler on the app's async runtime.
pub fn start_scheduler(
    app: tauri::AppHandle,
    log_store: LogStore,
    shell_store: ShellStore,
    mcp_store: McpStore,
) {
    let chat_hook: Arc<dyn ChatHook> =
        Arc::new(crate::commands::chat::TauriChatHook { app });
    tauri::async_runtime::spawn(pet_core::heartbeat::run_scheduler(
        log_store,
        shell_store,
        mcp_store,
        chat_hook,
    ));
}