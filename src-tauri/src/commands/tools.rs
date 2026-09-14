//! The tools the model is offered: what a chat turn actually gets right now
//! (`list_available_tools`, for the context-ring popover) and the full built-in
//! catalog the owner can switch off or re-describe (`list_tools`, for settings).
//!
//! Both go through `ToolRegistry` rather than their own list, so what the UI
//! shows is what a turn sends.

use pet_core::mcp::McpStore;
use pet_core::prompts;
use pet_core::settings::{self, get_settings};
use pet_core::tools::{builtin_catalog, ToolPolicy, ToolRegistry, ToolScope};
use serde::Serialize;
use tauri::{Emitter, State};

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
/// no `chat` tool). Tools the owner switched off are absent here too.
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
    let registry = ToolRegistry::new(
        mcp_defs,
        ToolPolicy {
            include_web_search: !settings.search_api_key.trim().is_empty(),
            ..ToolPolicy::from_config()
        },
    );
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

/// One built-in tool as the settings page shows it.
#[derive(Serialize)]
pub struct ToolEntry {
    pub name: String,
    /// The description in force (the owner's rewrite if there is one).
    pub description: String,
    /// The built-in text, so "restore default" has something to show.
    pub default_description: String,
    pub customized: bool,
    pub enabled: bool,
    /// The context gate this tool sits behind — a switched-on tool is still only
    /// offered to runs that gate allows.
    pub scope: ToolScope,
}

/// Every built-in tool, switched off ones included. The catalog is the whole
/// set; `enabled` and `description` are the owner's layer on top of it.
#[tauri::command]
pub fn list_tools() -> Result<Vec<ToolEntry>, String> {
    let disabled = get_settings()?.tools.disabled;
    let overrides = prompts::tool_descriptions();
    Ok(builtin_catalog()
        .into_iter()
        .map(|tool| {
            let custom = overrides.get(&tool.name);
            ToolEntry {
                description: custom.unwrap_or(&tool.description).clone(),
                customized: custom.is_some(),
                enabled: !disabled.contains(&tool.name),
                default_description: tool.description,
                name: tool.name,
                scope: tool.scope,
            }
        })
        .collect())
}

/// Switch one tool on or off for every agent. Takes effect on the next turn —
/// the registry is rebuilt per run.
#[tauri::command]
pub fn set_tool_enabled(app: tauri::AppHandle, name: String, enabled: bool) -> Result<(), String> {
    let mut s = settings::get_settings()?;
    s.tools.disabled.retain(|d| *d != name);
    if !enabled {
        s.tools.disabled.push(name);
        s.tools.disabled.sort();
    }
    settings::save_settings(&s)?;
    let _ = app.emit("settings-changed", ());
    Ok(())
}

#[tauri::command]
pub fn save_tool_description(name: String, content: String) -> Result<(), String> {
    prompts::save_tool_description(&name, &content)
}

#[tauri::command]
pub fn reset_tool_description(name: String) -> Result<(), String> {
    prompts::reset_tool_description(&name)
}
