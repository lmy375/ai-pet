//! CLI application state + the single-agent chat turn.
//!
//! Sessions are the SAME files the GUI uses (`<config>/pet/sessions/`), and each
//! turn reloads the active session from disk before appending — the same
//! reload-before-send rule the two GUI windows follow — so a CLI chat and an
//! open GUI window can share one conversation without clobbering each other.

use std::sync::Arc;

use pet_core::chat::{run_chat_pipeline, ChatEventSink, ChatMessage};
use pet_core::config::AiConfig;
use pet_core::logging::LogStore;
use pet_core::mcp::{McpManager, McpManagerStore};
use pet_core::session::{self, ContextUsage, Session};
use pet_core::settings::{get_settings, AgentConfig};
use pet_core::shell::{ShellStore, TaskCompletion, TaskNotifier};
use pet_core::tools::ToolContext;

use crate::sink::SessionSink;

const DEFAULT_SESSION_TITLE: &str = "新会话";

/// What starts a turn: the owner typed a message, or a background task finished
/// and its result resumes the conversation.
pub enum TurnInput {
    User(String),
    Completion(TaskCompletion),
}

pub struct CliApp {
    pub log_store: LogStore,
    pub shell_store: ShellStore,
    pub mcp_store: McpManagerStore,
    pub notifier: Arc<dyn TaskNotifier>,
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

    /// Load the active session, creating one if none exists. Reloading from disk
    /// per turn keeps the CLI convergent with a concurrently-open GUI window.
    fn load_active_session(&self) -> Result<Session, String> {
        let active_id = session::list_sessions().active_id;
        if !active_id.is_empty() {
            if let Ok(s) = session::load_session(active_id) {
                return Ok(s);
            }
        }
        session::create_session()
    }

    /// Run one chat turn against the active agent and persist it into the shared
    /// session, mirroring the GUI frontend's transcript bookkeeping. Streaming
    /// goes to `sink`; the sink's accumulated items are what get persisted.
    pub async fn run_chat_turn<S>(&self, input: TurnInput, sink: &S) -> Result<(), String>
    where
        S: ChatEventSink + SessionSink,
    {
        let config = AiConfig::from_settings()?;

        let mut sess = self.load_active_session()?;

        // Append the turn's input to both transcripts (model-facing + display).
        match &input {
            TurnInput::User(text) => {
                sess.messages
                    .push(serde_json::json!({ "role": "user", "content": text }));
                let mut item = session::user_item(text, &[]);
                item["ts"] = serde_json::json!(now_ms());
                sess.items.push(item);
            }
            TurnInput::Completion(c) => {
                let label = if c.label.is_empty() { c.kind.clone() } else { c.label.clone() };
                // Same message shape the GUI injects (chat.bgTaskDoneContent).
                let content = format!("[后台任务完成] {}：\n{}", label, c.result);
                sess.messages
                    .push(serde_json::json!({ "role": "user", "content": content }));
                sess.items.push(serde_json::json!({
                    "type": "notification",
                    "content": format!("后台任务完成：{}", label),
                    "detail": c.result,
                    "ts": now_ms(),
                }));
            }
        }

        let chat_messages: Vec<ChatMessage> = sess
            .messages
            .iter()
            .cloned()
            .filter_map(|v| serde_json::from_value(v).ok())
            .collect();

        let ctx = ToolContext::new(
            LogStore(self.log_store.0.clone()),
            ShellStore(self.shell_store.0.clone()),
            config.clone(),
            self.mcp_store.clone(),
            sess.id.clone(),
            Some(self.notifier.clone()),
            None, // no chat hook — the CLI runs no heartbeats
            false,
        );

        let result = run_chat_pipeline(chat_messages, sink, &config, &self.mcp_store, &ctx).await;

        // Persist what the sink rendered. Assistant text items go back into the
        // model-facing transcript too (one message per committed item — matching
        // the GUI, which commits streamed text on each tool boundary and at done).
        let new_items = sink.take_items();
        for item in &new_items {
            if item["type"] == "assistant" {
                if let Some(text) = item["content"].as_str() {
                    if !text.trim().is_empty() {
                        sess.messages
                            .push(serde_json::json!({ "role": "assistant", "content": text }));
                    }
                }
            }
        }
        sess.items.extend(new_items);

        if let Some((used, total)) = sink.usage() {
            sess.context_usage = Some(ContextUsage { used, total: total as u64 });
        }

        // Derive a title from the first user message while the session is unnamed.
        if sess.title == DEFAULT_SESSION_TITLE || sess.title.is_empty() {
            if let Some(t) = session::derive_title(&sess.items) {
                sess.title = t;
            }
        }

        sess.updated_at = pet_core::common::iso_now();
        sess.created_at = String::new(); // preserved by save_session
        session::save_session(sess)?;

        // A transport-level error never reached the sink as an Error event when
        // run_chat_pipeline returns Err before streaming; surface it.
        result.map(|_| ())
    }

    /// The active agent's config, if resolvable.
    pub fn active_agent(&self) -> Option<AgentConfig> {
        get_settings().ok().and_then(|s| s.active_agent_config().cloned())
    }
}

pub fn now_ms() -> i64 {
    chrono::Local::now().timestamp_millis()
}
