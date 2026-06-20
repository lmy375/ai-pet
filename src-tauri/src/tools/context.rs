use std::sync::Arc;

use crate::commands::debug::{write_log, LogStore};
use crate::commands::shell::{ShellStore, TaskNotifier};
use crate::config::AiConfig;
use crate::mcp::McpManagerStore;

/// Shared context passed to all tools during execution.
///
/// Besides the shell/log stores, it carries the AI config and MCP store so that
/// tools which themselves run an agentic loop (e.g. `spawn_subagent`) can make
/// LLM calls and reach the same tools. `depth` tracks sub-agent nesting so the
/// spawn tool can be withheld from sub-agents (see `ToolRegistry::new`).
///
/// `session_id` + `notifier` let a backgrounded task tell the UI which
/// conversation to resume when it finishes (`notifier` is `None` for non-UI
/// callers such as Telegram).
pub struct ToolContext {
    pub shell_store: ShellStore,
    pub log_store: LogStore,
    pub config: AiConfig,
    pub mcp_store: McpManagerStore,
    pub depth: usize,
    pub session_id: String,
    pub notifier: Option<Arc<dyn TaskNotifier>>,
    /// App handle, present for UI-backed callers. The `chat` tool needs it to
    /// write the main session, fire a system notification and tell the active
    /// window to refresh. `None` for non-UI callers (e.g. Telegram).
    pub app: Option<tauri::AppHandle>,
    /// True only for scheduled heartbeat sessions. Gates the `chat` tool (offered
    /// only to heartbeats) — see `ToolRegistry::new`.
    pub is_heartbeat: bool,
}

impl ToolContext {
    pub fn new(
        log_store: LogStore,
        shell_store: ShellStore,
        config: AiConfig,
        mcp_store: McpManagerStore,
        session_id: String,
        notifier: Option<Arc<dyn TaskNotifier>>,
        app: Option<tauri::AppHandle>,
        is_heartbeat: bool,
    ) -> Self {
        Self {
            shell_store,
            log_store,
            config,
            mcp_store,
            depth: 0,
            session_id,
            notifier,
            app,
            is_heartbeat,
        }
    }

    pub fn from_states(
        log_store: &tauri::State<'_, LogStore>,
        shell_store: &tauri::State<'_, ShellStore>,
        config: AiConfig,
        mcp_store: McpManagerStore,
        session_id: String,
        notifier: Option<Arc<dyn TaskNotifier>>,
        app: Option<tauri::AppHandle>,
    ) -> Self {
        Self {
            shell_store: ShellStore(shell_store.0.clone()),
            log_store: LogStore(log_store.0.clone()),
            config,
            mcp_store,
            depth: 0,
            session_id,
            notifier,
            app,
            is_heartbeat: false,
        }
    }

    /// A context for a nested sub-agent: same stores/config/session, one level
    /// deeper. The notifier is dropped on purpose — a sub-agent runs silently, so
    /// any background task it spawns internally must NOT push a completion into
    /// the parent conversation. The sub-agent's own completion is delivered by the
    /// parent's `run_or_background`, which still holds the parent notifier.
    pub fn child(&self) -> Self {
        Self {
            shell_store: ShellStore(self.shell_store.0.clone()),
            log_store: LogStore(self.log_store.0.clone()),
            config: self.config.clone(),
            mcp_store: self.mcp_store.clone(),
            depth: self.depth + 1,
            session_id: self.session_id.clone(),
            notifier: None,
            // Sub-agents never speak to the owner directly; drop the app handle
            // and the heartbeat flag so the `chat` tool is unavailable to them.
            app: None,
            is_heartbeat: false,
        }
    }

    pub fn log(&self, msg: &str) {
        write_log(&self.log_store.0, msg);
    }
}
