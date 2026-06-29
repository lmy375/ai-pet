use crate::commands::settings::get_settings;
use crate::mcp::McpManager;
use crate::mcp::{McpManagerStore, McpServerStatus};
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

/// List the tools available to a normal panel chat turn for `agent_id` (built-in
/// + that agent's connected MCP). Mirrors how `run_agent_loop` builds its
/// registry: depth 0 (so `spawn_subagent` is offered) and not a heartbeat (so no
/// `chat` tool).
#[tauri::command]
pub async fn list_available_tools(
    agent_id: String,
    mcp_store: State<'_, McpManagerStore>,
) -> Result<Vec<ToolInfo>, String> {
    let mcp_defs = {
        let managers = mcp_store.lock().await;
        managers.get(&agent_id).map(|m| m.definitions()).unwrap_or_default()
    };
    // Mirror the agent loop: web_search is listed only when the (global) Tavily
    // key is set.
    let web_search_enabled = get_settings()
        .map(|s| !s.search_api_key.trim().is_empty())
        .unwrap_or(false);
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

#[tauri::command]
pub async fn get_mcp_status(
    agent_id: String,
    mcp_store: State<'_, McpManagerStore>,
) -> Result<Vec<McpServerStatus>, String> {
    let managers = mcp_store.lock().await;
    Ok(managers
        .get(&agent_id)
        .map(|m| m.statuses().to_vec())
        .unwrap_or_default())
}

#[tauri::command]
pub async fn reconnect_mcp(
    agent_id: String,
    mcp_store: State<'_, McpManagerStore>,
) -> Result<Vec<McpServerStatus>, String> {
    let settings = get_settings()?;
    let agent = settings
        .agent(&agent_id)
        .ok_or_else(|| format!("Unknown agent: {}", agent_id))?;
    let fresh = McpManager::start_from_agent(agent).await;
    let statuses = fresh.statuses().to_vec();
    let mut managers = mcp_store.lock().await;
    if let Some(old) = managers.remove(&agent_id) {
        let mut old = old;
        old.shutdown().await;
    }
    managers.insert(agent_id, fresh);
    Ok(statuses)
}
