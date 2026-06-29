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
    /// Grouping key for `llm.log` entries (the LLM-log view keeps only the
    /// newest entry per group, since within one group every request carries the
    /// full prior history). For the main chat this equals `session_id`. Sub-agents
    /// and heartbeats run independent conversations that are NOT supersets of the
    /// parent, so each gets its own unique id (see `child()` and the heartbeat
    /// command) — otherwise they'd collapse into, or evict, the parent's row.
    pub log_session: String,
    pub notifier: Option<Arc<dyn TaskNotifier>>,
    /// App handle, present for UI-backed callers. The `chat` tool needs it to
    /// write the main session, fire a system notification and tell the active
    /// window to refresh. `None` for non-UI callers (e.g. Telegram).
    pub app: Option<tauri::AppHandle>,
    /// True only for scheduled heartbeat sessions. Gates the `chat` tool (offered
    /// only to heartbeats) — see `ToolRegistry::new`.
    pub is_heartbeat: bool,
    /// True only for group-chat agent runs. Gates the `GroupChat` tool (offered
    /// only in the group page) — see `ToolRegistry::new`. The group orchestrator
    /// sets it on the context after construction.
    pub is_group: bool,
    /// Images a tool wants the model to actually SEE. A tool's String return is
    /// appended as a `tool` role message, which can't carry an image, so tools
    /// like `screenshot` push a data URL here instead; the agent loop drains this
    /// after each round and appends it as a `user` message with an `image_url`
    /// content block (the same multimodal path used for pasted images).
    pub pending_images: Arc<std::sync::Mutex<Vec<String>>>,
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
            log_session: session_id.clone(),
            session_id,
            notifier,
            app,
            is_heartbeat,
            is_group: false,
            pending_images: Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    /// Like `new`, but clones the shared stores out of Tauri-managed `State`
    /// guards (the UI chat command's path). Not a heartbeat.
    pub fn from_states(
        log_store: &tauri::State<'_, LogStore>,
        shell_store: &tauri::State<'_, ShellStore>,
        config: AiConfig,
        mcp_store: McpManagerStore,
        session_id: String,
        notifier: Option<Arc<dyn TaskNotifier>>,
        app: Option<tauri::AppHandle>,
    ) -> Self {
        Self::new(
            LogStore(log_store.0.clone()),
            ShellStore(shell_store.0.clone()),
            config,
            mcp_store,
            session_id,
            notifier,
            app,
            false,
        )
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
            // Independent conversation: own log group so it neither evicts nor
            // merges with the parent's LLM-log row.
            log_session: format!("{}:sub:{}", self.session_id, uuid::Uuid::new_v4()),
            notifier: None,
            // Sub-agents never speak to the owner directly; drop the app handle
            // and the heartbeat flag so the `chat` tool is unavailable to them.
            app: None,
            is_heartbeat: false,
            is_group: false,
            // Fresh queue: a sub-agent's screenshots are consumed by its own loop.
            pending_images: Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    pub fn log(&self, msg: &str) {
        write_log(&self.log_store.0, msg);
    }

    /// Queue an image (a data URL) for the model to see on the next round.
    pub fn emit_image(&self, data_url: String) {
        if let Ok(mut imgs) = self.pending_images.lock() {
            imgs.push(data_url);
        }
    }

    /// Drain queued images. Called by the agent loop after each tool round.
    pub fn take_images(&self) -> Vec<String> {
        match self.pending_images.lock() {
            Ok(mut imgs) => std::mem::take(&mut *imgs),
            Err(_) => Vec::new(),
        }
    }
}
