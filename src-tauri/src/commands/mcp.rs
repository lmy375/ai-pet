use pet_core::mcp::{McpServerStatus, McpStore};
use pet_core::settings::{get_settings, AppSettings, McpServerConfig};
use tauri::State;

/// Connection status of every MCP server in the global pool. The settings UI
/// shows the whole list; the per-agent view filters it by the agent's `mcp`
/// names.
#[tauri::command]
pub async fn get_mcp_status(mcp_store: State<'_, McpStore>) -> Result<Vec<McpServerStatus>, String> {
    Ok(mcp_store.lock().await.statuses())
}

/// Drop every connection and reconnect the servers any agent references. Global
/// because the connections are: one server, one process, shared by every agent
/// that lists it.
#[tauri::command]
pub async fn reconnect_mcp(mcp_store: State<'_, McpStore>) -> Result<Vec<McpServerStatus>, String> {
    let settings = get_settings()?;
    let servers = referenced(&settings);
    let mut hub = mcp_store.lock().await;
    hub.reconnect(&servers).await;
    Ok(hub.statuses())
}

/// Bring one server's connection in line with its saved config: connect it if
/// it is enabled and someone references it, stop it otherwise. This is what the
/// settings switch calls, so flipping it takes effect immediately instead of
/// waiting for the next reconnect.
#[tauri::command]
pub async fn sync_mcp_server(
    name: String,
    mcp_store: State<'_, McpStore>,
) -> Result<Vec<McpServerStatus>, String> {
    let settings = get_settings()?;
    let wanted = referenced(&settings)
        .into_iter()
        .find(|(n, _)| *n == name)
        .map(|(n, c)| (n, c));
    let mut hub = mcp_store.lock().await;
    match wanted {
        Some(server) => hub.ensure(&[server]).await,
        None => hub.disconnect(&name).await,
    }
    Ok(hub.statuses())
}

/// The (name, config) pairs the pool should be running: enabled and referenced.
fn referenced(settings: &AppSettings) -> Vec<(&str, &McpServerConfig)> {
    settings
        .referenced_mcp_servers()
        .into_iter()
        .filter_map(|n| settings.mcp_servers.get_key_value(&n))
        .map(|(n, c)| (n.as_str(), c))
        .collect()
}
