//! The tools the model is offered: what a chat turn actually gets right now
//! (`list_available_tools`, for the context-ring popover) and the full built-in
//! catalog the owner can switch off or re-describe (`list_tools`, for settings).
//!
//! Both go through `ToolRegistry` rather than their own list, so what the UI
//! shows is what a turn sends.

use pet_core::mcp::McpStore;
use pet_core::prompts;
use pet_core::settings::{self, get_settings};
use pet_core::tools::{
    catalog_for_ui, RunGate, ToolCatalogEntry, ToolPolicy, ToolRegistry,
};
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
/// + the MCP servers that agent references). The gate is the one a real
/// interactive run opens at depth 0, so the preview matches the offer.
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
    let gate = RunGate::interactive(!settings.search_api_key.trim().is_empty());
    let registry = ToolRegistry::new(&mcp_store, &servers, ToolPolicy::for_gate(gate)).await;
    let defs = registry.definitions();
    let mut out = Vec::new();
    if let Some(arr) = defs.as_array() {
        for d in arr {
            let name = d["function"]["name"].as_str().unwrap_or("").to_string();
            if name.is_empty() {
                continue;
            }
            let description = d["function"]["description"].as_str().unwrap_or("").to_string();
            let is_mcp = registry.is_mcp(&name);
            out.push(ToolInfo { name, description, is_mcp });
        }
    }
    Ok(out)
}

/// Every built-in tool, switched off ones included, with the owner's overrides
/// and the gate each sits behind.
#[tauri::command]
pub fn list_tools() -> Result<Vec<ToolCatalogEntry>, String> {
    catalog_for_ui()
}

/// Switch one tool on or off for every agent. Takes effect on the next turn —
/// the registry is rebuilt per run.
#[tauri::command]
pub fn set_tool_enabled(app: tauri::AppHandle, name: String, enabled: bool) -> Result<(), String> {
    settings::update(|s| {
        s.tools.disabled.retain(|d| *d != name);
        if !enabled {
            s.tools.disabled.push(name.clone());
            s.tools.disabled.sort();
        }
        Ok(())
    })?;
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