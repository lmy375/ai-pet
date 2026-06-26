use crate::mcp::{McpManagerStore, McpServerStatus};
use crate::commands::settings::get_settings;
use crate::mcp::McpManager;
use crate::tools::ToolRegistry;
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

/// List the tools available to a normal panel chat turn (built-in + connected
/// MCP). Mirrors how `run_agent_loop` builds its registry: depth 0 (so
/// `spawn_subagent` is offered) and not a heartbeat (so no `chat` tool).
#[tauri::command]
pub async fn list_available_tools(
    mcp_store: State<'_, McpManagerStore>,
) -> Result<Vec<ToolInfo>, String> {
    let mcp_defs = {
        let manager = mcp_store.lock().await;
        manager.definitions()
    };
    // Mirror the agent loop: web_search is listed only when a Tavily key is set.
    let web_search_enabled = crate::commands::settings::get_settings()
        .map(|s| !s.search_api_key.trim().is_empty())
        .unwrap_or(false);
    let registry = ToolRegistry::new(mcp_defs, 0, false, web_search_enabled);
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

#[tauri::command]
pub async fn get_mcp_status(mcp_store: State<'_, McpManagerStore>) -> Result<Vec<McpServerStatus>, String> {
    let manager = mcp_store.lock().await;
    Ok(manager.statuses().to_vec())
}

#[tauri::command]
pub async fn reconnect_mcp(mcp_store: State<'_, McpManagerStore>) -> Result<Vec<McpServerStatus>, String> {
    let settings = get_settings()?;
    let mut manager = mcp_store.lock().await;
    manager.shutdown().await;
    *manager = McpManager::start_from_settings(&settings).await;
    Ok(manager.statuses().to_vec())
}
