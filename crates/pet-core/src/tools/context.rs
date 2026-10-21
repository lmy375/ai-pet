use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use crate::config::AiConfig;
use crate::logging::{write_log, LogSession, LogStore};
use crate::mcp::McpStore;
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
    pub mcp_store: McpStore,
    pub depth: usize,
    pub session_id: String,
    /// Which conversation this run's LLM requests are logged under. For the main
    /// chat it is the session itself, so successive turns overwrite one log file
    /// (every request carries the full prior history, so the newest is a superset
    /// of them all). Sub-agents, heartbeats and group runs are independent
    /// conversations that are NOT supersets of the parent, so each mints its own
    /// id — otherwise they'd collapse into, or evict, the parent's row.
    pub log_session: LogSession,
    pub notifier: Option<Arc<dyn TaskNotifier>>,
    /// UI hook for the `chat` tool (notification + conversation refresh),
    /// present only for UI-backed heartbeat runs. `None` for non-UI callers.
    pub chat_hook: Option<Arc<dyn ChatHook>>,
    /// True only for scheduled heartbeat sessions. Gates the `chat` tool (offered
    /// only to heartbeats) — see `ToolRegistry::new`.
    pub is_heartbeat: bool,
    /// The group room this run belongs to, set only for group-chat agent runs.
    /// Gates the `GroupChat` tool (see `ToolRegistry::new`) and gives it the
    /// room to post into. The group worker sets it after construction.
    pub group: Option<Arc<crate::group::GroupSession>>,
    /// The swarm this run belongs to. Main-agent and worker runs both receive
    /// this handle; workers additionally set `swarm_task_id` and
    /// `swarm_worker_id`, which gates claim/update/complete operations.
    pub swarm: Option<Arc<crate::swarm::SwarmSession>>,
    pub swarm_task_id: Option<String>,
    pub swarm_worker_id: Option<String>,
    /// Images a tool wants the model to actually SEE. A tool's String return is
    /// appended as a `tool` role message, which can't carry an image, so tools
    /// like `screenshot` push a data URL here instead; the agent loop drains this
    /// after each round and appends it as a `user` message with an `image_url`
    /// content block (the same multimodal path used for pasted images).
    pub pending_images: Arc<std::sync::Mutex<Vec<String>>>,
    /// Cooperative stop for the whole turn. The agent loop checks it between
    /// LLM stream events and around each tool call; the interface that owns the
    /// turn cancels it when the user aborts (the GUI's `cancel_chat` command).
    /// Sub-agents get a child token, so aborting the parent stops them too.
    pub cancel: CancellationToken,
}

impl ToolContext {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        log_store: LogStore,
        shell_store: ShellStore,
        config: AiConfig,
        mcp_store: McpStore,
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
            log_session: LogSession::chat(session_id.clone()),
            session_id,
            notifier,
            chat_hook,
            is_heartbeat,
            group: None,
            swarm: None,
            swarm_task_id: None,
            swarm_worker_id: None,
            pending_images: Arc::new(std::sync::Mutex::new(Vec::new())),
            cancel: CancellationToken::new(),
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
            // Independent conversation: own log entry so it neither evicts nor
            // merges with the parent's LLM-log row.
            log_session: LogSession::sub(&self.session_id),
            notifier: None,
            // Sub-agents never speak to the owner directly; drop the chat hook
            // and the heartbeat flag so the `chat` tool is unavailable to them.
            chat_hook: None,
            is_heartbeat: false,
            group: None,
            swarm: None,
            swarm_task_id: None,
            swarm_worker_id: None,
            // Fresh queue: a sub-agent's screenshots are consumed by its own loop.
            pending_images: Arc::new(std::sync::Mutex::new(Vec::new())),
            cancel: self.cancel.child_token(),
        }
    }

    pub fn log(&self, msg: &str) {
        write_log(&self.log_store.0, msg);
    }

    /// A context for one tool call within a concurrent batch. Shared stores,
    /// config and cancellation stay shared; images get a private queue so the
    /// agent loop can merge them in tool-call order instead of completion order.
    pub(crate) fn for_tool_call(&self) -> Self {
        Self {
            shell_store: ShellStore(self.shell_store.0.clone()),
            log_store: LogStore(self.log_store.0.clone()),
            config: self.config.clone(),
            mcp_store: self.mcp_store.clone(),
            depth: self.depth,
            session_id: self.session_id.clone(),
            log_session: self.log_session.clone(),
            notifier: self.notifier.clone(),
            chat_hook: self.chat_hook.clone(),
            is_heartbeat: self.is_heartbeat,
            group: self.group.clone(),
            swarm: self.swarm.clone(),
            swarm_task_id: self.swarm_task_id.clone(),
            swarm_worker_id: self.swarm_worker_id.clone(),
            pending_images: Arc::new(std::sync::Mutex::new(Vec::new())),
            cancel: self.cancel.clone(),
        }
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

    /// Swarm workers are task-scoped and must never read or modify the Agent's
    /// long-term memory directory. The check is deliberately path-based and is
    /// used by file and shell tools before touching the filesystem.
    pub fn swarm_worker_path_allowed(&self, path: &str) -> bool {
        if self.swarm_task_id.is_none() || self.swarm_worker_id.is_none() {
            return true;
        }
        let Ok(root) = crate::memory::memory_dir(&self.config.agent_id) else {
            return true;
        };
        let path = std::path::Path::new(path);
        if path.starts_with(&root) {
            return false;
        }
        let root_name = root.to_string_lossy();
        !path.to_string_lossy().contains(root_name.as_ref())
            && !path.to_string_lossy().ends_with("MEMORY.md")
            && !path.to_string_lossy().ends_with("USER.md")
            && !path.to_string_lossy().ends_with("HEARTBEAT.md")
            && !path.to_string_lossy().ends_with("SOUL.md")
    }

    pub fn swarm_worker_command_allowed(&self, command: &str) -> bool {
        if self.swarm_task_id.is_none() || self.swarm_worker_id.is_none() {
            return true;
        }
        let lower = command.to_ascii_lowercase();
        let protected = ["memory.md", "user.md", "heartbeat.md", "soul.md", "memory/"];
        !protected.iter().any(|item| lower.contains(item))
    }
}
