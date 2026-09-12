//! CLI application state.
//!
//! Sessions are the SAME files the GUI uses (`<config>/pet/sessions/`), and
//! turns are run by the shared `TurnRunner` — which reloads the session from
//! disk before appending — so a CLI chat and an open GUI window can share one
//! conversation without clobbering each other.

use std::sync::Arc;

use pet_core::logging::LogStore;
use pet_core::mcp::{McpManager, McpManagerStore};
use pet_core::session;
use pet_core::settings::{get_settings, AgentConfig};
use pet_core::shell::ShellStore;
use pet_core::turn::TurnRunner;

pub struct CliApp {
    pub log_store: LogStore,
    pub shell_store: ShellStore,
    pub mcp_store: McpManagerStore,
    /// Runs and persists every chat turn (and injects background-task
    /// completions); the TUI / one-shot printer only observe its events.
    pub turns: Arc<TurnRunner>,
}

impl CliApp {
    /// Start the agent's MCP servers if configured and not yet running (lazy —
    /// the GUI starts every agent's servers at launch; the CLI connects only
    /// the agents actually used). Returns a human-readable status when servers
    /// were (re)connected, `None` when nothing needed doing.
    pub async fn ensure_mcp(&self, agent: &AgentConfig) -> Option<String> {
        if agent.mcp_servers.is_empty() {
            return None;
        }
        {
            let managers = self.mcp_store.lock().await;
            if managers.contains_key(&agent.id) {
                return None;
            }
        }
        let manager = McpManager::start_from_agent(agent).await;
        let mut lines = vec![format!("{} 的 MCP 服务器：", agent.name)];
        for s in manager.statuses() {
            let mark = if s.connected { "✓" } else { "✗" };
            lines.push(format!("  {} {} ({} tools)", mark, s.name, s.tool_count));
        }
        self.mcp_store.lock().await.insert(agent.id.clone(), manager);
        Some(lines.join("\n"))
    }

    /// Best-effort shutdown of every running MCP server (child processes).
    pub async fn shutdown_mcp(&self) {
        let mut managers = self.mcp_store.lock().await;
        for (_, mut m) in managers.drain() {
            m.shutdown().await;
        }
    }

    /// The active session's id, creating a session if there is none (or the
    /// index points at a file that no longer exists).
    pub fn active_session_id(&self) -> Result<String, String> {
        let active_id = session::list_sessions().active_id;
        if !active_id.is_empty() && session::load_session(active_id.clone()).is_ok() {
            return Ok(active_id);
        }
        Ok(session::create_session()?.id)
    }

    /// The active agent's config, if resolvable.
    pub fn active_agent(&self) -> Option<AgentConfig> {
        get_settings().ok().and_then(|s| s.active_agent_config().cloned())
    }
}
