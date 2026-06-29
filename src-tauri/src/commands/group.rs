//! Multi-agent group chat.
//!
//! All participating agents + the owner share ONE transcript (`GroupState.transcript`).
//! Each agent additionally keeps its own private LLM context (`GroupAgentState`),
//! so it sees the group conversation but reasons/uses tools in isolation — the
//! Agent sub-tabs render those private sessions.
//!
//! Concurrency model:
//! - Agents react **concurrently** with each other — a new message wakes every
//!   idle member, each in its own background worker.
//! - Each agent processes its messages **serially**: while its worker is running,
//!   new transcript messages just accumulate (they advance the transcript, not its
//!   `consumed_upto`). When the current run finishes, the worker drains ALL pending
//!   messages at once and feeds them into a single coalesced LLM loop — never one
//!   loop per message.
//! - Agents speak by calling the `GroupChat` tool (`post_agent_message`), which
//!   appends to the transcript and wakes the OTHER idle agents. This cascades
//!   freely — there's no round cap. The owner stops runaway chatter with the pause
//!   control (`group_set_paused`), which aborts every in-flight worker.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::Mutex;
use tokio::task::AbortHandle;

use crate::commands::chat::{run_agent_loop, ChatEventSink, StreamEvent};
use crate::commands::debug::LogStore;
use crate::commands::prompt;
use crate::commands::session;
use crate::commands::settings::get_settings;
use crate::commands::shell::ShellStore;
use crate::config::AiConfig;
use crate::mcp::McpManagerStore;
use crate::tools::ToolContext;

/// One line in the shared group transcript.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupMessage {
    pub id: String,
    /// "human" or "agent".
    pub speaker_kind: String,
    /// The author agent's id (only for `speaker_kind == "agent"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    /// Display name of the speaker (agent name, or the owner label).
    pub name: String,
    pub content: String,
    /// Epoch millis.
    pub ts: i64,
}

/// One agent's private state in the group.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GroupAgentState {
    /// Running LLM context (NO system messages — those are rebuilt each run).
    #[serde(default)]
    pub messages: Vec<Value>,
    /// Display items (ChatItem JSON) for the Agent sub-tab.
    #[serde(default)]
    pub items: Vec<Value>,
    /// How far into `transcript` this agent has already consumed.
    #[serde(default)]
    pub consumed_upto: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GroupState {
    #[serde(default)]
    pub transcript: Vec<GroupMessage>,
    /// Agent ids participating in the group (user-selected subset).
    #[serde(default)]
    pub members: Vec<String>,
    #[serde(default)]
    pub agents: HashMap<String, GroupAgentState>,
    /// When true, no workers run and the owner has stopped the room. Persisted so
    /// reopening the panel reflects it.
    #[serde(default)]
    pub paused: bool,
    /// Abort handles for the currently-running per-agent workers. In-memory only
    /// (a worker can't survive a restart). Also the source of truth for "is this
    /// agent running" (so we never spawn two workers for one agent).
    #[serde(skip)]
    pub aborts: HashMap<String, AbortHandle>,
}

/// Tauri-managed handle to the group state (mirrors `McpManagerStore`).
pub struct GroupStore(pub Arc<Mutex<GroupState>>);

fn group_path() -> Result<PathBuf, String> {
    let dir = crate::common::config_dir()?.join("group");
    std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create group dir: {e}"))?;
    Ok(dir.join("state.json"))
}

fn load_state() -> GroupState {
    group_path()
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|c| serde_json::from_str(&c).ok())
        .unwrap_or_default()
}

fn save_state(state: &GroupState) {
    if let Ok(p) = group_path() {
        if let Ok(json) = serde_json::to_string_pretty(state) {
            let _ = std::fs::write(p, json);
        }
    }
}

/// Build the managed store, seeded from disk. Called once in `lib.rs`.
pub fn new_group_store() -> GroupStore {
    GroupStore(Arc::new(Mutex::new(load_state())))
}

/// `HH:MM` for the injected `[time] name:` prefix.
fn fmt_hm(ts_ms: i64) -> String {
    use chrono::TimeZone;
    chrono::Local
        .timestamp_millis_opt(ts_ms)
        .single()
        .map(|dt| dt.format("%H:%M").to_string())
        .unwrap_or_default()
}

fn now_ms() -> i64 {
    chrono::Local::now().timestamp_millis()
}

/// Broadcast a new transcript message to the windows (only the panel's group
/// view listens). Global emit — the pet window simply ignores it.
fn emit_group_message(app: &AppHandle, msg: &GroupMessage) {
    let _ = app.emit("group-message", msg.clone());
}

/// Does `agent_id` have unconsumed messages authored by someone else?
fn has_pending(s: &GroupState, agent_id: &str, tlen: usize) -> bool {
    let consumed = s.agents.get(agent_id).map(|a| a.consumed_upto).unwrap_or(0);
    if consumed >= tlen {
        return false;
    }
    s.transcript[consumed..tlen]
        .iter()
        .any(|m| m.agent_id.as_deref() != Some(agent_id))
}

/// Spawn a worker for every member that has pending messages and isn't already
/// running. No-op while paused. Agents run concurrently; each agent's own runs are
/// serialized by its single worker. Must be called while holding the state lock.
fn wake_agents(app: &AppHandle, store: &Arc<Mutex<GroupState>>, s: &mut GroupState) {
    if s.paused {
        return;
    }
    let tlen = s.transcript.len();
    let members = s.members.clone();
    for id in members {
        if s.aborts.contains_key(&id) || !has_pending(s, &id, tlen) {
            continue;
        }
        let app2 = app.clone();
        let store2 = store.clone();
        let id2 = id.clone();
        let handle = tokio::spawn(async move { agent_worker(app2, store2, id2).await });
        s.aborts.insert(id, handle.abort_handle());
    }
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn group_load(store: State<'_, GroupStore>) -> Result<GroupState, String> {
    Ok(store.0.lock().await.clone())
}

/// Set the participating agents. Creates private state for new members and drops
/// state for removed ones.
#[tauri::command]
pub async fn group_set_members(ids: Vec<String>, store: State<'_, GroupStore>) -> Result<(), String> {
    let mut s = store.0.lock().await;
    for id in &ids {
        s.agents.entry(id.clone()).or_default();
    }
    s.agents.retain(|k, _| ids.contains(k));
    s.members = ids;
    save_state(&s);
    Ok(())
}

/// Pause (stop ALL in-flight loops) or resume the group. Pausing aborts every
/// running worker; resuming wakes any agent with pending messages.
#[tauri::command]
pub async fn group_set_paused(paused: bool, app: AppHandle, store: State<'_, GroupStore>) -> Result<(), String> {
    {
        let mut s = store.0.lock().await;
        s.paused = paused;
        if paused {
            for (_, h) in s.aborts.drain() {
                h.abort();
            }
            save_state(&s);
        } else {
            save_state(&s);
            wake_agents(&app, &store.0, &mut s);
        }
    }
    let _ = app.emit(if paused { "group-paused" } else { "group-resumed" }, ());
    Ok(())
}

/// Clear the transcript and every agent's private context (keeps membership).
/// Aborts any in-flight workers and clears the paused state.
#[tauri::command]
pub async fn group_reset(app: AppHandle, store: State<'_, GroupStore>) -> Result<(), String> {
    {
        let mut s = store.0.lock().await;
        for (_, h) in s.aborts.drain() {
            h.abort();
        }
        s.transcript.clear();
        for st in s.agents.values_mut() {
            *st = GroupAgentState::default();
        }
        s.paused = false;
        save_state(&s);
    }
    let _ = app.emit("group-reset", ());
    Ok(())
}

/// The owner sends a message into the group, waking every idle member (unless
/// paused — then it just queues until resume).
#[tauri::command]
pub async fn group_send(content: String, app: AppHandle, store: State<'_, GroupStore>) -> Result<(), String> {
    let content = content.trim().to_string();
    if content.is_empty() {
        return Err("消息为空".to_string());
    }
    let msg = GroupMessage {
        id: uuid::Uuid::new_v4().to_string(),
        speaker_kind: "human".to_string(),
        agent_id: None,
        name: "主人".to_string(),
        content,
        ts: now_ms(),
    };

    {
        let mut s = store.0.lock().await;
        s.transcript.push(msg.clone());
        wake_agents(&app, &store.0, &mut s);
        save_state(&s);
    }

    emit_group_message(&app, &msg);
    Ok(())
}

/// Append an agent's message to the transcript (the `GroupChat` tool's effect)
/// and wake the OTHER idle agents so they can react.
pub async fn post_agent_message(app: &AppHandle, store: &GroupStore, agent_id: &str, content: &str) {
    let name = get_settings()
        .ok()
        .and_then(|s| s.agent(agent_id).map(|a| a.name.clone()))
        .unwrap_or_else(|| agent_id.to_string());
    let msg = GroupMessage {
        id: uuid::Uuid::new_v4().to_string(),
        speaker_kind: "agent".to_string(),
        agent_id: Some(agent_id.to_string()),
        name,
        content: content.to_string(),
        ts: now_ms(),
    };

    {
        let mut s = store.0.lock().await;
        s.transcript.push(msg.clone());
        // Wake the other idle agents (skips this one — it's still running).
        wake_agents(app, &store.0, &mut s);
        save_state(&s);
    }

    emit_group_message(app, &msg);
}

// ---------------------------------------------------------------------------
// Per-agent worker
// ---------------------------------------------------------------------------

/// A scheduled agent reaction, prepared under the state lock and run unlocked.
struct AgentRun {
    agent_id: String,
    config: AiConfig,
    /// The agent's private context (incoming messages already injected), minus
    /// the system messages (added fresh per run).
    messages: Vec<Value>,
    /// Display items for the messages injected this run, so the Agent sub-tab can
    /// render the incoming group lines live (injection isn't a sink event).
    injected_items: Vec<Value>,
}

/// Coalesce ALL of the agent's pending (unconsumed, non-self) messages into its
/// context and return a single run. Advances `consumed_upto` to `tlen`. Returns
/// `None` if the agent has no usable config (then its pending messages are marked
/// consumed so the worker doesn't spin on them).
fn prepare_run(s: &mut GroupState, agent_id: &str, tlen: usize) -> Option<AgentRun> {
    let consumed = s.agents.get(agent_id).map(|a| a.consumed_upto).unwrap_or(0);
    let foreign: Vec<GroupMessage> = s.transcript[consumed..tlen]
        .iter()
        .filter(|m| m.agent_id.as_deref() != Some(agent_id))
        .cloned()
        .collect();

    let settings = get_settings().ok();
    let config = settings
        .as_ref()
        .and_then(|st| st.agent(agent_id))
        .and_then(|a| AiConfig::from_agent(a).ok());
    let Some(config) = config else {
        if let Some(st) = s.agents.get_mut(agent_id) {
            st.consumed_upto = tlen;
        }
        return None;
    };

    let st = s.agents.entry(agent_id.to_string()).or_default();
    let mut injected_items = Vec::new();
    for m in &foreign {
        let line = format!("[{}] {}: {}", fmt_hm(m.ts), m.name, m.content);
        st.messages.push(json!({ "role": "user", "content": line }));
        let mut item = session::user_item(&line, &[]);
        item["ts"] = json!(m.ts);
        st.items.push(item.clone());
        injected_items.push(item);
    }
    st.consumed_upto = tlen;
    let messages = st.messages.clone();
    save_state(s);
    Some(AgentRun { agent_id: agent_id.to_string(), config, messages, injected_items })
}

/// One agent's worker: drains pending → runs one coalesced loop → repeats until
/// nothing is pending, then exits. Re-entry is guarded by `GroupState.aborts`; the
/// "no pending → exit" decision is atomic with that map so a message arriving as
/// the worker winds down is never lost (either this worker sees it and loops, or
/// `wake_agents` spawns a fresh one).
async fn agent_worker(app: AppHandle, store: Arc<Mutex<GroupState>>, agent_id: String) {
    let mcp_store = app.state::<McpManagerStore>().inner().clone();
    let log_store = LogStore(app.state::<LogStore>().inner().0.clone());
    let shell_store = ShellStore(app.state::<ShellStore>().inner().0.clone());

    loop {
        let run = {
            let mut s = store.lock().await;
            let tlen = s.transcript.len();
            if s.paused || !has_pending(&s, &agent_id, tlen) {
                s.aborts.remove(&agent_id);
                save_state(&s);
                return;
            }
            prepare_run(&mut s, &agent_id, tlen)
        };

        let Some(run) = run else {
            // No usable config — its pending messages were consumed in prepare_run.
            let mut s = store.lock().await;
            s.aborts.remove(&agent_id);
            save_state(&s);
            return;
        };

        run_one_agent(&app, &store, &mcp_store, &log_store, &shell_store, run).await;
        // Loop: messages may have arrived during the run — coalesce and continue.
    }
}

/// Run one agent's reaction (no state lock held during the LLM call) and persist
/// the result into its private session.
async fn run_one_agent(
    app: &AppHandle,
    store: &Arc<Mutex<GroupState>>,
    mcp_store: &McpManagerStore,
    log_store: &LogStore,
    shell_store: &ShellStore,
    run: AgentRun,
) {
    // Surface the incoming group lines to the Agent sub-tab before the agent
    // starts reasoning (injection isn't a sink event).
    let _ = app.emit(
        "group-injected",
        json!({ "agentId": run.agent_id, "items": run.injected_items }),
    );

    let mut conv = run.messages.clone();
    prompt::prepend_group_system_messages(&mut conv, &run.agent_id);

    let mut ctx = ToolContext::new(
        LogStore(log_store.0.clone()),
        ShellStore(shell_store.0.clone()),
        run.config.clone(),
        mcp_store.clone(),
        format!("group:{}", run.agent_id),
        None, // group runs never inject background-task completions into main chat
        Some(app.clone()),
        false, // not a heartbeat
    );
    ctx.is_group = true;
    ctx.log_session = format!("group:{}:{}", run.agent_id, uuid::Uuid::new_v4());

    let sink = GroupSink::new(app.clone(), run.agent_id.clone());

    let result = run_agent_loop(conv, &sink, &run.config, mcp_store, &ctx).await;

    {
        let mut s = store.lock().await;
        if let Some(st) = s.agents.get_mut(&run.agent_id) {
            match result {
                Ok((_text, full_conv)) => {
                    // Drop the (fresh-each-run) leading system messages before storing.
                    st.messages = full_conv
                        .into_iter()
                        .skip_while(|m| m.get("role").and_then(|r| r.as_str()) == Some("system"))
                        .collect();
                }
                Err(e) => {
                    eprintln!("group agent {} failed: {}", run.agent_id, e);
                }
            }
            st.items.extend(sink.take_items());
            // NOTE: do NOT advance `consumed_upto` here. `prepare_run` already moved
            // it to this run's start length; messages that arrived from OTHER agents
            // DURING this run must stay unconsumed so the worker's next iteration
            // injects them. (The agent's own posts are skipped by `has_pending` /
            // `prepare_run`'s self filter, so they don't trigger a needless re-run.)
        }
        save_state(&s);
    }

    // Clears the Agent sub-tab's "running" indicator even on a transport-error
    // path where the sink emitted neither done nor error.
    let _ = app.emit("group-agent-done", json!({ "agentId": run.agent_id }));
}

// ---------------------------------------------------------------------------
// GroupSink — streams an agent's run to the panel + captures display items
// ---------------------------------------------------------------------------

/// In-progress display state, mirroring `useChat`'s stream reducer so the
/// persisted `items` match what the live `group-stream` listener renders.
#[derive(Default)]
struct SinkInner {
    /// Accumulated assistant text not yet committed.
    accumulated: String,
    /// Tool calls in the current (not-yet-flushed) group.
    tool_calls: Vec<Value>,
    /// Committed display items.
    items: Vec<Value>,
}

impl SinkInner {
    fn flush_tool_calls(&mut self) {
        if self.tool_calls.is_empty() {
            return;
        }
        let calls = std::mem::take(&mut self.tool_calls);
        self.items.push(json!({
            "type": "tool",
            "content": "",
            "toolCalls": calls,
            "ts": now_ms(),
        }));
    }

    fn commit_text(&mut self) {
        let text = std::mem::take(&mut self.accumulated);
        if text.trim().is_empty() {
            return;
        }
        self.items.push(json!({
            "type": "assistant",
            "content": text,
            "ts": now_ms(),
        }));
    }
}

/// A `ChatEventSink` for a group agent run: forwards every event to the panel as
/// a `group-stream` Tauri event (tagged with the agent id) AND builds the
/// agent's display items for persistence.
struct GroupSink {
    app: AppHandle,
    agent_id: String,
    inner: std::sync::Mutex<SinkInner>,
}

impl GroupSink {
    fn new(app: AppHandle, agent_id: String) -> Self {
        Self { app, agent_id, inner: std::sync::Mutex::new(SinkInner::default()) }
    }

    fn emit(&self, event: StreamEvent) {
        let _ = self.app.emit(
            "group-stream",
            json!({ "agentId": self.agent_id, "event": event }),
        );
    }

    /// Take the accumulated display items (after the run finishes).
    fn take_items(&self) -> Vec<Value> {
        std::mem::take(&mut self.inner.lock().unwrap().items)
    }
}

impl ChatEventSink for GroupSink {
    fn send_chunk(&self, text: &str) {
        {
            let mut inner = self.inner.lock().unwrap();
            inner.flush_tool_calls();
            inner.accumulated.push_str(text);
        }
        self.emit(StreamEvent::Chunk { text: text.to_string() });
    }

    fn send_tool_start(&self, name: &str, arguments: &str) {
        {
            let mut inner = self.inner.lock().unwrap();
            // Commit any assistant text streamed before this tool call.
            inner.commit_text();
            inner.tool_calls.push(json!({
                "name": name,
                "arguments": arguments,
                "isRunning": false,
            }));
        }
        self.emit(StreamEvent::ToolStart { name: name.to_string(), arguments: arguments.to_string() });
    }

    fn send_tool_result(&self, name: &str, result: &str) {
        {
            let mut inner = self.inner.lock().unwrap();
            // Attach to the first tool call of that name still missing a result.
            if let Some(tc) = inner
                .tool_calls
                .iter_mut()
                .find(|tc| tc["name"] == json!(name) && tc.get("result").is_none())
            {
                tc["result"] = json!(result);
            }
        }
        self.emit(StreamEvent::ToolResult { name: name.to_string(), result: result.to_string() });
    }

    fn send_image(&self, data_url: &str) {
        {
            let mut inner = self.inner.lock().unwrap();
            inner.flush_tool_calls();
            inner.items.push(json!({
                "type": "assistant",
                "content": "",
                "images": [data_url],
                "ts": now_ms(),
            }));
        }
        self.emit(StreamEvent::Image { data_url: data_url.to_string() });
    }

    fn send_usage(&self, _prompt_tokens: u64, _total_tokens: u64, _context_window: u32) {
        // The group view has no per-agent context ring; ignore.
    }

    fn send_done(&self) {
        {
            let mut inner = self.inner.lock().unwrap();
            inner.flush_tool_calls();
            inner.commit_text();
        }
        self.emit(StreamEvent::Done {});
    }

    fn send_error(&self, message: &str) {
        {
            let mut inner = self.inner.lock().unwrap();
            inner.flush_tool_calls();
            inner.items.push(json!({
                "type": "error",
                "content": message,
                "ts": now_ms(),
            }));
        }
        self.emit(StreamEvent::Error { message: message.to_string() });
    }
}
