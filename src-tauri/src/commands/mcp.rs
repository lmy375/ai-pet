use crate::commands::settings::get_settings;
use crate::mcp::McpManager;
use crate::mcp::{McpManagerStore, McpServerStatus};
use tauri::State;

#[tauri::command]
pub async fn get_mcp_status(
    mcp_store: State<'_, McpManagerStore>,
) -> Result<Vec<McpServerStatus>, String> {
    let manager = mcp_store.lock().await;
    Ok(manager.statuses().to_vec())
}

#[tauri::command]
pub async fn reconnect_mcp(
    mcp_store: State<'_, McpManagerStore>,
) -> Result<Vec<McpServerStatus>, String> {
    let settings = get_settings()?;
    let mut manager = mcp_store.lock().await;
    manager.shutdown().await;
    *manager = McpManager::start_from_settings(&settings).await;
    Ok(manager.statuses().to_vec())
}
