//! Multi-agent group chat — multiple independent rooms.
//!
//! All participating agents + the owner share ONE transcript per room
//! (`GroupState.transcript`). Each agent additionally keeps its own private LLM
//! context (`GroupAgentState`), so it sees the group conversation but
//! reasons/uses tools in isolation — the Agent sub-tabs render those private
//! sessions. Rooms are fully independent: their own members, transcript, agent
//! contexts and pause state, persisted as one file each.
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
//!   control (`set_paused`), which aborts every in-flight worker.
//!
//! The orchestrator is interface-agnostic: all UI delivery goes through the
//! [`GroupEvents`] trait (the Tauri layer emits window events; the CLI prints).

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::Mutex;
use tokio::task::AbortHandle;

use crate::chat::{run_agent_loop, ChatEventSink, ItemBuilder, StreamEvent};
use crate::config::AiConfig;
use crate::logging::LogStore;
use crate::mcp::McpStore;
use crate::prompt;
use crate::session;
use crate::settings::get_settings;
use crate::shell::ShellStore;
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
    pub id: String,
    #[serde(default)]
    pub title: String,
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
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub updated_at: i64,
    /// Abort handles for the currently-running per-agent workers. In-memory only
    /// (a worker can't survive a restart). Also the source of truth for "is this
    /// agent running" (so we never spawn two workers for one agent).
    #[serde(skip)]
    pub aborts: HashMap<String, AbortHandle>,
}

/// Lightweight room record for the sidebar list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupSummary {
    pub id: String,
    pub title: String,
    pub members: Vec<String>,
    pub paused: bool,
    pub updated_at: i64,
}

/// Where group-chat activity is delivered to the user. Implemented per
/// interface: the Tauri layer forwards each callback as a window event
/// (`group-message`, `group-stream`, …); the CLI prints to the terminal.
/// Every callback carries the room id so interfaces can ignore other rooms.
pub trait GroupEvents: Send + Sync {
    /// A new line landed in a room's transcript (owner or agent).
    fn message(&self, group_id: &str, msg: &GroupMessage);
    /// A stream event from one agent's private run (chunk/tool/error/…).
    fn stream(&self, group_id: &str, agent_id: &str, event: &StreamEvent);
    /// Group lines injected into an agent's private context before a run
    /// (display items for the Agent sub-tab; injection isn't a sink event).
    fn injected(&self, group_id: &str, agent_id: &str, items: &[Value]);
    /// An agent's worker finished one run (clears its "running" indicator, even
    /// on transport-error paths where the sink emitted neither done nor error).
    fn agent_done(&self, group_id: &str, agent_id: &str);
}

/// A live room: its shared state, the event sink and the stores its agent
/// workers execute with.
pub struct GroupSession {
    pub state: Mutex<GroupState>,
    events: Arc<dyn GroupEvents>,
    mcp_store: McpStore,
    log_store: LogStore,
    shell_store: ShellStore,
    /// Tombstone: a deleted room must not be written back by an in-flight run.
    deleted: AtomicBool,
}

/// Owns every room. Interfaces construct one at startup and share it via
/// `Arc`; sessions are loaded from disk on first use.
pub struct GroupRuntime {
    sessions: Mutex<HashMap<String, Arc<GroupSession>>>,
    deleted: Mutex<HashSet<String>>,
    events: Arc<dyn GroupEvents>,
    mcp_store: McpStore,
    log_store: LogStore,
    shell_store: ShellStore,
}

impl GroupRuntime {
    pub fn new(
        events: Arc<dyn GroupEvents>,
        mcp_store: McpStore,
        log_store: LogStore,
        shell_store: ShellStore,
    ) -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            deleted: Mutex::new(HashSet::new()),
            events,
            mcp_store,
            log_store,
            shell_store,
        }
    }

    /// All rooms, newest activity first.
    pub fn list(&self) -> Vec<GroupSummary> {
        let Ok(dir) = groups_dir() else { return Vec::new() };
        let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
        let mut out: Vec<GroupSummary> = entries
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("json") {
                    return None;
                }
                let state: GroupState =
                    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
                if state.id.is_empty() {
                    return None;
                }
                Some(summary(&state))
            })
            .collect();
        out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        out
    }

    /// Create a room and make it available immediately.
    pub async fn create(&self, title: &str, members: Vec<String>) -> Result<GroupState, String> {
        let title = title.trim();
        if title.is_empty() {
            return Err("群聊名称不能为空".to_string());
        }
        let id = uuid::Uuid::new_v4().to_string();
        let now = now_ms();
        let mut state = GroupState {
            id: id.clone(),
            title: title.to_string(),
            created_at: now,
            updated_at: now,
            ..GroupState::default()
        };
        apply_members(&mut state, members);
        save_state(&state);

        let session = Arc::new(GroupSession {
            state: Mutex::new(state.clone()),
            events: self.events.clone(),
            mcp_store: self.mcp_store.clone(),
            log_store: LogStore(self.log_store.0.clone()),
            shell_store: ShellStore(self.shell_store.0.clone()),
            deleted: AtomicBool::new(false),
        });
        self.sessions.lock().await.insert(id, session);
        Ok(state)
    }

    /// Snapshot one room (for rendering a freshly-opened group view).
    pub async fn load(&self, id: &str) -> Result<GroupState, String> {
        Ok(self.session(id).await?.state.lock().await.clone())
    }

    pub async fn configure(
        &self,
        id: &str,
        title: &str,
        members: Vec<String>,
    ) -> Result<(), String> {
        let title = title.trim();
        if title.is_empty() {
            return Err("群聊名称不能为空".to_string());
        }
        let session = self.session(id).await?;
        let mut s = session.state.lock().await;
        s.title = title.to_string();
        apply_members(&mut s, members);
        s.updated_at = now_ms();
        session.save(&s);
        Ok(())
    }

    /// Change membership only (title untouched).
    pub async fn set_members(&self, id: &str, members: Vec<String>) -> Result<(), String> {
        let session = self.session(id).await?;
        let mut s = session.state.lock().await;
        apply_members(&mut s, members);
        s.updated_at = now_ms();
        session.save(&s);
        Ok(())
    }

    pub async fn delete(&self, id: &str) -> Result<(), String> {
        // Keep the sessions lock for the whole operation so a concurrent
        // `session()` can't load a fresh room while its file is being removed.
        let mut sessions = self.sessions.lock().await;
        let removed = sessions.remove(id);
        self.deleted.lock().await.insert(id.to_string());
        if let Some(session) = &removed {
            session.deleted.store(true, Ordering::SeqCst);
            let mut s = session.state.lock().await;
            for (_, handle) in s.aborts.drain() {
                handle.abort();
            }
        }
        match remove_state(id) {
            Ok(()) => Ok(()),
            Err(e) => {
                // The file is still there: keep serving the room instead of
                // leaving an unopenable entry in the list.
                if let Some(session) = removed {
                    session.deleted.store(false, Ordering::SeqCst);
                    sessions.insert(id.to_string(), session);
                }
                self.deleted.lock().await.remove(id);
                Err(e)
            }
        }
    }

    /// Pause (stop ALL in-flight loops) or resume a room.
    pub async fn set_paused(&self, id: &str, paused: bool) -> Result<(), String> {
        let session = self.session(id).await?;
        session.set_paused(paused).await;
        Ok(())
    }

    /// Clear a room's transcript and every agent's private context (keeps
    /// membership). Aborts any in-flight workers and clears the paused state.
    pub async fn reset(&self, id: &str) -> Result<(), String> {
        self.session(id).await?.reset().await;
        Ok(())
    }

    /// The owner sends a message into a room.
    pub async fn send(&self, id: &str, content: &str) -> Result<(), String> {
        self.session(id).await?.send(content).await
    }

    /// First room (newest activity), creating a default one when none exists.
    /// Used by interfaces that expose a single group room (the CLI).
    pub async fn ensure_default(&self) -> Result<String, String> {
        if let Some(first) = self.list().first() {
            return Ok(first.id.clone());
        }
        Ok(self.create("群聊", Vec::new()).await?.id)
    }

    async fn session(&self, id: &str) -> Result<Arc<GroupSession>, String> {
        let mut sessions = self.sessions.lock().await;
        if self.deleted.lock().await.contains(id) {
            return Err("群聊不存在".to_string());
        }
        if let Some(session) = sessions.get(id) {
            return Ok(session.clone());
        }
        let state = load_state(id)?;
        let session = Arc::new(GroupSession {
            state: Mutex::new(state),
            events: self.events.clone(),
            mcp_store: self.mcp_store.clone(),
            log_store: LogStore(self.log_store.0.clone()),
            shell_store: ShellStore(self.shell_store.0.clone()),
            deleted: AtomicBool::new(false),
        });
        sessions.insert(id.to_string(), session.clone());
        Ok(session)
    }
}

impl GroupSession {
    /// The owner sends a message into the room, waking every idle member
    /// (unless paused — then it just queues until resume).
    pub async fn send(self: &Arc<Self>, content: &str) -> Result<(), String> {
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

        let group_id = {
            let mut s = self.state.lock().await;
            s.transcript.push(msg.clone());
            s.updated_at = msg.ts;
            wake_agents(self, &mut s);
            self.save(&s);
            s.id.clone()
        };

        self.events.message(&group_id, &msg);
        Ok(())
    }

    /// Append an agent's message to the transcript (the `GroupChat` tool's
    /// effect) and wake the OTHER idle agents so they can react.
    pub async fn post_agent_message(self: &Arc<Self>, agent_id: &str, content: &str) {
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

        let group_id = {
            let mut s = self.state.lock().await;
            s.transcript.push(msg.clone());
            s.updated_at = msg.ts;
            // Wake the other idle agents (skips this one — it's still running).
            wake_agents(self, &mut s);
            self.save(&s);
            s.id.clone()
        };

        self.events.message(&group_id, &msg);
    }

    async fn set_paused(self: &Arc<Self>, paused: bool) {
        let mut s = self.state.lock().await;
        s.paused = paused;
        s.updated_at = now_ms();
        if paused {
            for (_, handle) in s.aborts.drain() {
                handle.abort();
            }
            self.save(&s);
        } else {
            self.save(&s);
            wake_agents(self, &mut s);
        }
    }

    async fn reset(&self) {
        let mut s = self.state.lock().await;
        for (_, handle) in s.aborts.drain() {
            handle.abort();
        }
        s.transcript.clear();
        for st in s.agents.values_mut() {
            *st = GroupAgentState::default();
        }
        s.paused = false;
        s.updated_at = now_ms();
        self.save(&s);
    }

    /// Persist unless the room has been deleted (an in-flight run must not
    /// resurrect its file).
    fn save(&self, state: &GroupState) {
        if !self.deleted.load(Ordering::SeqCst) {
            save_state(state);
        }
    }
}

fn summary(state: &GroupState) -> GroupSummary {
    GroupSummary {
        id: state.id.clone(),
        title: state.title.clone(),
        members: state.members.clone(),
        paused: state.paused,
        updated_at: state.updated_at,
    }
}

/// Apply a membership set: create private state for new members, drop it for
/// removed ones, and abort workers that belong to removed agents.
fn apply_members(s: &mut GroupState, ids: Vec<String>) {
    for id in &ids {
        s.agents.entry(id.clone()).or_default();
    }
    s.agents.retain(|id, _| ids.contains(id));
    let removed: Vec<String> = s
        .aborts
        .keys()
        .filter(|id| !ids.contains(id))
        .cloned()
        .collect();
    for id in removed {
        if let Some(handle) = s.aborts.remove(&id) {
            handle.abort();
        }
    }
    s.members = ids;
}

fn groups_dir() -> Result<PathBuf, String> {
    let dir = crate::common::config_dir()?.join("groups");
    std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create groups dir: {e}"))?;
    Ok(dir)
}

/// Room ids are generated UUIDs. Refuse anything else so a client-supplied id
/// can never escape the groups directory.
fn group_path(id: &str) -> Result<PathBuf, String> {
    if id.is_empty()
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err("群聊不存在".to_string());
    }
    Ok(groups_dir()?.join(format!("{id}.json")))
}

fn load_state(id: &str) -> Result<GroupState, String> {
    let path = group_path(id)?;
    let content = std::fs::read_to_string(&path).map_err(|_| "群聊不存在".to_string())?;
    let mut state: GroupState =
        serde_json::from_str(&content).map_err(|e| format!("群聊数据损坏: {e}"))?;
    if state.id.is_empty() {
        state.id = id.to_string();
    }
    Ok(state)
}

fn save_state(state: &GroupState) {
    if let Ok(path) = group_path(&state.id) {
        if let Ok(json) = serde_json::to_string_pretty(state) {
            let _ = std::fs::write(path, json);
        }
    }
}

fn remove_state(id: &str) -> Result<(), String> {
    let path = group_path(id)?;
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("删除群聊失败：{e}")),
    }
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
fn wake_agents(session: &Arc<GroupSession>, s: &mut GroupState) {
    if s.paused {
        return;
    }
    let tlen = s.transcript.len();
    let members = s.members.clone();
    for id in members {
        if s.aborts.contains_key(&id) || !has_pending(s, &id, tlen) {
            continue;
        }
        let session = session.clone();
        let worker_agent = id.clone();
        let handle = tokio::spawn(async move { agent_worker(session, worker_agent).await });
        s.aborts.insert(id, handle.abort_handle());
    }
}

// ---------------------------------------------------------------------------
// Per-agent worker
// ---------------------------------------------------------------------------

/// A scheduled agent reaction, prepared under the state lock and run unlocked.
struct AgentRun {
    agent_id: String,
    group_title: String,
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
fn prepare_run(session: &GroupSession, s: &mut GroupState, agent_id: &str, tlen: usize) -> Option<AgentRun> {
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
        st.messages
            .push(crate::llm::store_message(&crate::llm::user_message(&line, &[])));
        let mut item = session::user_item(&line, &[]);
        item["ts"] = json!(m.ts);
        st.items.push(item.clone());
        injected_items.push(item);
    }
    st.consumed_upto = tlen;
    let messages = st.messages.clone();
    let group_title = s.title.clone();
    session.save(s);
    Some(AgentRun { agent_id: agent_id.to_string(), group_title, config, messages, injected_items })
}

/// One agent's worker: drains pending → runs one coalesced loop → repeats until
/// nothing is pending, then exits. Re-entry is guarded by `GroupState.aborts`; the
/// "no pending → exit" decision is atomic with that map so a message arriving as
/// the worker winds down is never lost (either this worker sees it and loops, or
/// `wake_agents` spawns a fresh one).
async fn agent_worker(session: Arc<GroupSession>, agent_id: String) {
    loop {
        let run = {
            let mut s = session.state.lock().await;
            if session.deleted.load(Ordering::SeqCst) {
                s.aborts.remove(&agent_id);
                return;
            }
            let tlen = s.transcript.len();
            if s.paused || !has_pending(&s, &agent_id, tlen) {
                s.aborts.remove(&agent_id);
                session.save(&s);
                return;
            }
            prepare_run(&session, &mut s, &agent_id, tlen)
        };

        let Some(run) = run else {
            // No usable config — its pending messages were consumed in prepare_run.
            let mut s = session.state.lock().await;
            s.aborts.remove(&agent_id);
            session.save(&s);
            return;
        };

        run_one_agent(&session, run).await;
        // Loop: messages may have arrived during the run — coalesce and continue.
    }
}

/// Run one agent's reaction (no state lock held during the LLM call) and persist
/// the result into its private session.
async fn run_one_agent(session: &Arc<GroupSession>, run: AgentRun) {
    let group_id = {
        let s = session.state.lock().await;
        s.id.clone()
    };

    // Surface the incoming group lines to the Agent sub-tab before the agent
    // starts reasoning (injection isn't a sink event).
    session.events.injected(&group_id, &run.agent_id, &run.injected_items);

    let mut conv = run.messages.clone();
    prompt::prepend_group_system_messages(&mut conv, &run.agent_id, &run.group_title);

    let mut ctx = ToolContext::new(
        LogStore(session.log_store.0.clone()),
        ShellStore(session.shell_store.0.clone()),
        run.config.clone(),
        session.mcp_store.clone(),
        format!("group:{group_id}:{}", run.agent_id),
        None, // group runs never inject background-task completions into main chat
        None, // no chat hook — group agents speak via the GroupChat tool
        false, // not a heartbeat
    );
    ctx.group = Some(session.clone());
    ctx.log_session = crate::logging::LogSession::group(&group_id, &run.agent_id);

    let sink = GroupSink::new(session.events.clone(), group_id.clone(), run.agent_id.clone());

    let result = run_agent_loop(conv, &sink, &run.config, &session.mcp_store, &ctx).await;

    {
        let mut s = session.state.lock().await;
        if let Some(st) = s.agents.get_mut(&run.agent_id) {
            match result {
                Ok((_text, full_conv)) => {
                    // Drop the (fresh-each-run) leading system messages before storing.
                    st.messages = full_conv
                        .into_iter()
                        .skip_while(crate::llm::is_system_message)
                        .collect();
                }
                Err(e) => {
                    eprintln!("group agent {} failed: {}", run.agent_id, e);
                }
            }
            st.items.extend(sink.take_items());
            s.updated_at = now_ms();
            // NOTE: do NOT advance `consumed_upto` here. `prepare_run` already moved
            // it to this run's start length; messages that arrived from OTHER agents
            // DURING this run must stay unconsumed so the worker's next iteration
            // injects them. (The agent's own posts are skipped by `has_pending` /
            // `prepare_run`'s self filter, so they don't trigger a needless re-run.)
        }
        session.save(&s);
    }

    // Clears the Agent sub-tab's "running" indicator even on a transport-error
    // path where the sink emitted neither done nor error.
    session.events.agent_done(&group_id, &run.agent_id);
}

// ---------------------------------------------------------------------------
// GroupSink — streams an agent's run to the interface + captures display items
// ---------------------------------------------------------------------------

/// A `ChatEventSink` for a group agent run: forwards every event to the
/// interface via `GroupEvents::stream` (tagged with room + agent id) AND builds
/// the agent's display items for persistence (via the shared `ItemBuilder`, so
/// persisted items match what the live listener rendered).
struct GroupSink {
    events: Arc<dyn GroupEvents>,
    group_id: String,
    agent_id: String,
    items: std::sync::Mutex<ItemBuilder>,
}

impl GroupSink {
    fn new(events: Arc<dyn GroupEvents>, group_id: String, agent_id: String) -> Self {
        Self { events, group_id, agent_id, items: std::sync::Mutex::new(ItemBuilder::default()) }
    }

    fn emit(&self, event: StreamEvent) {
        self.events.stream(&self.group_id, &self.agent_id, &event);
    }

    /// Take the accumulated display items (after the run finishes).
    fn take_items(&self) -> Vec<Value> {
        self.items.lock().unwrap().take_items()
    }
}

impl ChatEventSink for GroupSink {
    fn send_chunk(&self, text: &str) {
        self.items.lock().unwrap().chunk(text);
        self.emit(StreamEvent::Chunk { text: text.to_string() });
    }

    fn send_reasoning(&self, text: &str) {
        // Forward chain-of-thought to the interface (the group reducer ignores it
        // for now); never fold it into the persisted message — it's not the answer.
        self.emit(StreamEvent::Reasoning { text: text.to_string() });
    }

    fn send_tool_start(&self, name: &str, arguments: &str) {
        self.items.lock().unwrap().tool_start(name, arguments);
        self.emit(StreamEvent::ToolStart { name: name.to_string(), arguments: arguments.to_string() });
    }

    fn send_tool_result(&self, name: &str, result: &str) {
        self.items.lock().unwrap().tool_result(name, result);
        self.emit(StreamEvent::ToolResult { name: name.to_string(), result: result.to_string() });
    }

    fn send_image(&self, data_url: &str) {
        self.items.lock().unwrap().image(data_url);
        self.emit(StreamEvent::Image { data_url: data_url.to_string() });
    }

    fn send_usage(&self, _prompt_tokens: u64, _total_tokens: u64, _context_window: u32) {
        // The group view has no per-agent context ring; ignore.
    }

    fn send_done(&self) {
        self.items.lock().unwrap().done();
        self.emit(StreamEvent::Done {});
    }

    fn send_error(&self, message: &str) {
        self.items.lock().unwrap().error(message);
        self.emit(StreamEvent::Error { message: message.to_string() });
    }
}
