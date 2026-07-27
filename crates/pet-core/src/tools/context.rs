use std::sync::Arc;

use crate::config::AiConfig;
use crate::logging::{write_log, LogStore};
use crate::mcp::McpManagerStore;
use crate::shell::{ShellStore, TaskNotifier};

/// UI side effects of the heartbeat-only `chat` tool, performed AFTER the core
/// has written the pet's message into the active session on disk: fire a system
/// notification and tell the active view to reload the conversation. Implemented
/// by the Tauri layer; `None` for interfaces without a resident UI.
pub trait ChatHook: Send + Sync {
    fn on_chat_inserted(&self, session_id: &str, message: &str);
}

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
    /// UI hook for the `chat` tool (notification + conversation refresh),
    /// present only for UI-backed heartbeat runs. `None` for non-UI callers.
    pub chat_hook: Option<Arc<dyn ChatHook>>,
    /// True only for scheduled heartbeat sessions. Gates the `chat` tool (offered
    /// only to heartbeats) — see `ToolRegistry::new`.
    pub is_heartbeat: bool,
    /// The group room this run belongs to, set only for group-chat agent runs.
    /// Gates the `GroupChat` tool (see `ToolRegistry::new`) and gives it the
    /// room to post into. The group orchestrator sets it after construction.
    pub group: Option<Arc<crate::group::GroupRuntime>>,
    /// Images a tool wants the model to actually SEE. A tool's String return is
    /// appended as a `tool` role message, which can't carry an image, so tools
    /// like `screenshot` push a data URL here instead; the agent loop drains this
    /// after each round and appends it as a `user` message with an `image_url`
    /// content block (the same multimodal path used for pasted images).
    pub pending_images: Arc<std::sync::Mutex<Vec<String>>>,
}

impl ToolContext {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        log_store: LogStore,
        shell_store: ShellStore,
        config: AiConfig,
        mcp_store: McpManagerStore,
        session_id: String,
        notifier: Option<Arc<dyn TaskNotifier>>,
        chat_hook: Option<Arc<dyn ChatHook>>,
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
            chat_hook,
            is_heartbeat,
            group: None,
            pending_images: Arc::new(std::sync::Mutex::new(Vec::new())),
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
            // Independent conversation: own log group so it neither evicts nor
            // merges with the parent's LLM-log row.
            log_session: format!("{}:sub:{}", self.session_id, uuid::Uuid::new_v4()),
            notifier: None,
            // Sub-agents never speak to the owner directly; drop the chat hook
            // and the heartbeat flag so the `chat` tool is unavailable to them.
            chat_hook: None,
            is_heartbeat: false,
            group: None,
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
