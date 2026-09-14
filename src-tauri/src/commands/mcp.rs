use pet_core::mcp::{McpServerStatus, McpStore};
use pet_core::settings::{get_settings, AppSettings, McpServerConfig};
use pet_core::tools::ToolRegistry;
use serde::Serialize;
use tauri::State;

/// A tool exposed to the chat, surfaced to the UI (e.g. the context-ring popover).
#[derive(Serialize)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    /// True if provided by an MCP server (vs. a built-in tool).
    pub is_mcp: bool,
}

/// List the tools available to a normal panel chat turn for `agent_id` (built-in
/// + the MCP servers that agent references). Mirrors how `run_agent_loop` builds
/// its registry: depth 0 (so `spawn_subagent` is offered) and not a heartbeat (so
/// no `chat` tool).
#[tauri::command]
pub async fn list_available_tools(
    agent_id: String,
    mcp_store: State<'_, McpStore>,
) -> Result<Vec<ToolInfo>, String> {
    let settings = get_settings()?;
    let servers = settings
        .agent(&agent_id)
        .map(|a| a.mcp.clone())
        .unwrap_or_default();
    let mcp_defs = mcp_store.lock().await.definitions(&servers);
    // Mirror the agent loop: web_search is listed only when the (global) Tavily
    // key is set.
    let web_search_enabled = !settings.search_api_key.trim().is_empty();
    let registry = ToolRegistry::new(mcp_defs, 0, false, web_search_enabled, false);
    let defs = registry.definitions();
    let mut out = Vec::new();
    if let Some(arr) = defs.as_array() {
        for d in arr {
            let name = d["function"]["name"].as_str().unwrap_or("").to_string();
            if name.is_empty() {
                continue;
            }
            let description = d["function"]["description"].as_str().unwrap_or("").to_string();
            let is_mcp = registry.is_mcp_tool(&name);
            out.push(ToolInfo { name, description, is_mcp });
        }
    }
    Ok(out)
}

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
