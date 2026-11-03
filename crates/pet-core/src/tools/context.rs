use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use super::registry::RunKind;
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

/// The long-lived stores every tool call shares. Grouped so a `ToolContext` can
/// be built from one argument instead of three positional ones — the other
/// fields are per-run and belong on the builder below.
#[derive(Clone)]
pub struct ToolStores {
    pub log: LogStore,
    pub shell: ShellStore,
    pub mcp: McpStore,
}

impl ToolStores {
    pub fn new(log: LogStore, shell: ShellStore, mcp: McpStore) -> Self {
        Self { log, shell, mcp }
    }
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
    /// What kind of run this is. Replaces the old `is_heartbeat` flag and feeds
    /// `RunGate`, which decides which tools this run may be offered.
    pub kind: RunKind,
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
    /// The common case: one conversation's run and nothing optional set. Callers
    /// add the parts they need with the `with_*` methods below.
    pub fn for_session(stores: ToolStores, config: AiConfig, session_id: impl Into<String>) -> Self {
        let session_id = session_id.into();
        Self {
            shell_store: stores.shell,
            log_store: stores.log,
            config,
            mcp_store: stores.mcp,
            depth: 0,
            kind: RunKind::Interactive,
            log_session: LogSession::chat(session_id.clone()),
            session_id,
            notifier: None,
            chat_hook: None,
            swarm: None,
            swarm_task_id: None,
            swarm_worker_id: None,
            pending_images: Arc::new(std::sync::Mutex::new(Vec::new())),
            cancel: CancellationToken::new(),
        }
    }

    pub fn with_notifier(mut self, notifier: Option<Arc<dyn TaskNotifier>>) -> Self {
        self.notifier = notifier;
        self
    }

    pub fn with_chat_hook(mut self, chat_hook: Option<Arc<dyn ChatHook>>) -> Self {
        self.chat_hook = chat_hook;
        self
    }

    pub fn with_depth(mut self, depth: usize) -> Self {
        self.depth = depth;
        self
    }

    pub fn with_kind(mut self, kind: RunKind) -> Self {
        self.kind = kind;
        self
    }

    pub fn with_log_session(mut self, log_session: LogSession) -> Self {
        self.log_session = log_session;
        self
    }

    pub fn with_swarm(mut self, swarm: Option<Arc<crate::swarm::SwarmSession>>) -> Self {
        self.swarm = swarm;
        self
    }

    pub fn with_cancel(mut self, cancel: CancellationToken) -> Self {
        self.cancel = cancel;
        self
    }

    /// A context for a nested sub-agent: same stores/config/session, one level
    /// deeper. The notifier is dropped on purpose — a sub-agent runs silently, so
    /// any background task it spawns internally must NOT push a completion into
    /// the parent conversation. The sub-agent's own completion is delivered by the
    /// parent's `run_or_background`, which still holds the parent notifier.
    pub fn child(&self) -> Self {
        Self {
            shell_store: self.shell_store.clone(),
            log_store: self.log_store.clone(),
            config: self.config.clone(),
            mcp_store: self.mcp_store.clone(),
            depth: self.depth + 1,
            kind: RunKind::Interactive,
            session_id: self.session_id.clone(),
            // Independent conversation: own log entry so it neither evicts nor
            // merges with the parent's LLM-log row.
            log_session: LogSession::sub(&self.session_id),
            notifier: None,
            // Sub-agents never speak to the owner directly; drop the chat hook
            // and the heartbeat kind so the `chat` tool is unavailable to them.
            chat_hook: None,
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
            shell_store: self.shell_store.clone(),
            log_store: self.log_store.clone(),
            config: self.config.clone(),
            mcp_store: self.mcp_store.clone(),
            depth: self.depth,
            kind: self.kind,
            session_id: self.session_id.clone(),
            log_session: self.log_session.clone(),
            notifier: self.notifier.clone(),
            chat_hook: self.chat_hook.clone(),
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