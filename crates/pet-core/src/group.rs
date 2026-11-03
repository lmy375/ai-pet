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
//! - Agents speak by emitting `<public>…</public>` text. Each complete block is
//!   appended to the transcript (`post_agent_message`), which wakes the OTHER idle
//!   agents. Text outside the tags — reasoning, tool calls, private notes — never
//!   reaches the room. This cascades freely; there's no round cap. The owner stops
//!   runaway chatter with the pause control (`set_paused`), which cancels every
//!   in-flight worker.
//!
//! The orchestrator is interface-agnostic: all UI delivery goes through the
//! [`GroupEvents`] trait (the Tauri layer emits window events; the CLI prints).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::chat::{run_agent, AgentRun as ChatAgentRun, ChatEventSink, StreamEvent};
use crate::common::now_ms;
use crate::config::AiConfig;
use crate::group_public::{Piece, PublicFilter};
use crate::item::{ChatItem, ItemBuilder};
use crate::logging::LogStore;
use crate::mcp::McpStore;
use crate::registry::{LiveSession, SessionRegistry, SessionState};
use crate::session::Conversation;
use crate::system_prompt::SystemPrompt;
use crate::settings::get_settings;
use crate::shell::ShellStore;
use crate::tools::{RunKind, ToolContext, ToolStores};

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
    /// The agent's private LLM context (NO system messages — those are rebuilt
    /// each run) and the display items for the Agent sub-tab.
    #[serde(flatten)]
    pub conv: Conversation,
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
    /// What this room is about. Injected into every member's system prompt.
    #[serde(default)]
    pub topic: String,
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
    /// Tokens for the currently-running per-agent workers. In-memory only (a
    /// worker can't survive a restart). Also the source of truth for "is this
    /// agent running" (so we never spawn two workers for one agent). Each token
    /// is handed to the worker's `ToolContext`, so stop/pause also interrupts the
    /// tools an agent is running, not just its outer task.
    #[serde(skip)]
    pub cancels: HashMap<String, CancellationToken>,
}

/// Lightweight room record for the sidebar list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupSummary {
    pub id: String,
    pub title: String,
    pub topic: String,
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
    fn injected(&self, group_id: &str, agent_id: &str, items: &[ChatItem]);
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
    /// The room's file, written atomically by [`LiveSession::persist`].
    path: PathBuf,
    /// Tombstone: a deleted room must not be written back by an in-flight run.
    deleted: AtomicBool,
}

impl LiveSession<GroupState> for GroupSession {
    fn deleted(&self) -> &AtomicBool {
        &self.deleted
    }

    fn path(&self) -> &Path {
        &self.path
    }

    async fn cancel_all(&self) {
        let mut s = self.state.lock().await;
        for (_, token) in s.cancels.drain() {
            token.cancel();
        }
    }
}

/// Owns every room. Interfaces construct one at startup and share it via
/// `Arc`; sessions are loaded from disk on first use.
pub struct GroupRuntime {
    registry: SessionRegistry<GroupState, GroupSession>,
}

impl GroupRuntime {
    pub fn new(
        events: Arc<dyn GroupEvents>,
        mcp_store: McpStore,
        log_store: LogStore,
        shell_store: ShellStore,
    ) -> Self {
        let registry = SessionRegistry::new(crate::store::data_dir("groups"), move |path, state| {
            Arc::new(GroupSession {
                state: Mutex::new(state),
                events: events.clone(),
                mcp_store: mcp_store.clone(),
                log_store: LogStore(log_store.0.clone()),
                shell_store: shell_store.clone(),
                path,
                deleted: AtomicBool::new(false),
            })
        });
        Self { registry }
    }

    /// All rooms, newest activity first.
    pub fn list(&self) -> Vec<GroupSummary> {
        let mut out: Vec<GroupSummary> = self.registry.list().iter().map(summary).collect();
        out.sort_by_key(|a| std::cmp::Reverse(a.updated_at));
        out
    }

    /// Create a room and make it available immediately.
    pub async fn create(
        &self,
        title: &str,
        topic: &str,
        members: Vec<String>,
    ) -> Result<GroupState, String> {
        let title = title.trim();
        if title.is_empty() {
            return Err("群聊名称不能为空".to_string());
        }
        let id = uuid::Uuid::new_v4().to_string();
        let now = now_ms();
        let mut state = GroupState {
            id: id.clone(),
            title: title.to_string(),
            topic: topic.trim().to_string(),
            created_at: now,
            updated_at: now,
            ..GroupState::default()
        };
        apply_members(&mut state, members);
        self.registry.create(state.clone()).await?;
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
        topic: &str,
        members: Vec<String>,
    ) -> Result<(), String> {
        let title = title.trim();
        if title.is_empty() {
            return Err("群聊名称不能为空".to_string());
        }
        let session = self.session(id).await?;
        let mut s = session.state.lock().await;
        s.title = title.to_string();
        s.topic = topic.trim().to_string();
        apply_members(&mut s, members);
        s.updated_at = now_ms();
        session.persist(&s);
        Ok(())
    }

    /// Change membership only (title untouched).
    pub async fn set_members(&self, id: &str, members: Vec<String>) -> Result<(), String> {
        let session = self.session(id).await?;
        let mut s = session.state.lock().await;
        apply_members(&mut s, members);
        s.updated_at = now_ms();
        session.persist(&s);
        Ok(())
    }

    pub async fn delete(&self, id: &str) -> Result<(), String> {
        self.registry.delete(id).await
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
        Ok(self.create("群聊", "", Vec::new()).await?.id)
    }

    async fn session(&self, id: &str) -> Result<Arc<GroupSession>, String> {
        self.registry.get(id).await
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

        // The owner is never muted: a paused room still records the message
        // (it just stays queued — `wake_agents` is a no-op while paused).
        let event = msg.clone();
        let group_id = {
            let mut s = self.state.lock().await;
            append_and_wake(self, &mut s, msg)
        };

        self.events.message(&group_id, &event);
        Ok(())
    }

    /// Append a completed `<public>` block to the transcript and wake the OTHER
    /// idle agents so they can react. The group sink calls this once per closed
    /// block, in generation order.
    pub async fn post_agent_message(self: &Arc<Self>, agent_id: &str, content: &str) {
        if self.deleted.load(Ordering::SeqCst) {
            return;
        }
        let content = content.trim();
        if content.is_empty() {
            return;
        }
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

        let event = msg.clone();
        let group_id = {
            let mut s = self.state.lock().await;
            // Guards unique to agent speech (the owner's `send` has neither):
            // pause mutes a run that was already winding down, and a member
            // removed mid-run must not speak. The tombstone is checked at entry.
            if s.paused || !s.members.iter().any(|m| m == agent_id) {
                return;
            }
            append_and_wake(self, &mut s, msg)
        };

        self.events.message(&group_id, &event);
    }

    async fn set_paused(self: &Arc<Self>, paused: bool) {
        let mut s = self.state.lock().await;
        s.paused = paused;
        s.updated_at = now_ms();
        if paused {
            for (_, token) in s.cancels.drain() {
                token.cancel();
            }
            self.persist(&s);
        } else {
            self.persist(&s);
            wake_agents(self, &mut s);
        }
    }

    async fn reset(&self) {
        let mut s = self.state.lock().await;
        for (_, token) in s.cancels.drain() {
            token.cancel();
        }
        s.transcript.clear();
        for st in s.agents.values_mut() {
            *st = GroupAgentState::default();
        }
        s.paused = false;
        s.updated_at = now_ms();
        self.persist(&s);
    }

}

fn summary(state: &GroupState) -> GroupSummary {
    GroupSummary {
        id: state.id.clone(),
        title: state.title.clone(),
        topic: state.topic.clone(),
        members: state.members.clone(),
        paused: state.paused,
        updated_at: state.updated_at,
    }
}

/// Apply a membership set: create private state for new members, drop it for
/// removed ones, and cancel workers that belong to removed agents.
fn apply_members(s: &mut GroupState, ids: Vec<String>) {
    for id in &ids {
        s.agents.entry(id.clone()).or_default();
    }
    s.agents.retain(|id, _| ids.contains(id));
    let removed: Vec<String> = s
        .cancels
        .keys()
        .filter(|id| !ids.contains(id))
        .cloned()
        .collect();
    for id in removed {
        if let Some(token) = s.cancels.remove(&id) {
            token.cancel();
        }
    }
    s.members = ids;
}

/// Disk access for group rooms, keeping the group-specific error wording.
impl SessionState for GroupState {
    fn id(&self) -> String {
        self.id.clone()
    }

    fn read(path: &Path) -> Result<Self, String> {
        // Only "absent" becomes 群聊不存在; a permission/IO failure is reported
        // as itself so it isn't mistaken for a deleted room.
        let content = crate::store::read_text(path)?.ok_or_else(|| "群聊不存在".to_string())?;
        let mut state: GroupState =
            serde_json::from_str(&content).map_err(|e| format!("群聊数据损坏: {e}"))?;
        if state.id.is_empty() {
            state.id = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
        }
        Ok(state)
    }

    fn not_found(_id: &str) -> String {
        "群聊不存在".to_string()
    }

    fn delete_error(err: std::io::Error) -> String {
        format!("删除群聊失败：{err}")
    }
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

/// Push `msg` into the room transcript, wake every idle member and persist the
/// room. Returns the room id so the caller can emit the message event.
///
/// Shared by owner speech (`send`) and agent speech (`post_agent_message`). The
/// guards that differ stay in the callers: agent speech is muted by `paused`,
/// removal and the tombstone, while owner speech is always recorded (a paused
/// room just queues it — `wake_agents` is a no-op while paused).
fn append_and_wake(session: &Arc<GroupSession>, s: &mut GroupState, msg: GroupMessage) -> String {
    s.updated_at = msg.ts;
    s.transcript.push(msg);
    wake_agents(session, s);
    session.persist(s);
    s.id.clone()
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
        if s.cancels.contains_key(&id) || !has_pending(s, &id, tlen) {
            continue;
        }
        let session = session.clone();
        let worker_agent = id.clone();
        let cancel = CancellationToken::new();
        s.cancels.insert(id, cancel.clone());
        tokio::spawn(async move { agent_worker(session, worker_agent, cancel).await });
    }
}

// ---------------------------------------------------------------------------
// Per-agent worker
// ---------------------------------------------------------------------------

/// A scheduled agent reaction, prepared under the state lock and run unlocked.
struct AgentRun {
    agent_id: String,
    group_title: String,
    group_topic: String,
    /// Display names of the AI members. The owner is added when building the prompt.
    group_members: Vec<String>,
    config: AiConfig,
    /// The agent's private context (incoming messages already injected), minus
    /// the system messages (added fresh per run).
    messages: Vec<Value>,
    /// Display items for the messages injected this run, so the Agent sub-tab can
    /// render the incoming group lines live (injection isn't a sink event).
    injected_items: Vec<ChatItem>,
}

/// The tag name the model sees for one speaker, e.g. `<chat-小宠-message>`.
/// The speaker's display name is kept for readability; whitespace and angle
/// brackets become `-` so the wrapper can never be broken by a name.
fn chat_tag(name: &str) -> String {
    let speaker: String = name
        .chars()
        .map(|c| if c.is_whitespace() || c == '<' || c == '>' { '-' } else { c })
        .collect();
    format!("chat-{speaker}-message")
}

/// The line injected into an agent's private context. Every group line is
/// wrapped in `<chat-{speaker}-message>…</chat-{speaker}-message>` so the model
/// can tell speakers apart without the old `[HH:MM] name:` prefix.
fn injected_line(m: &GroupMessage) -> String {
    let tag = chat_tag(&m.name);
    format!("<{tag}>{}</{tag}>", m.content)
}

/// The display item for an injected line. It carries the sender explicitly —
/// the Agent sub-tab renders it as a chat message instead of raw tag text.
fn injected_item(m: &GroupMessage) -> ChatItem {
    ChatItem::group(
        &m.content,
        &m.speaker_kind,
        m.agent_id.clone(),
        &m.name,
        m.ts,
    )
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
        let line = injected_line(m);
        st.conv
            .messages
            .push(crate::llm::store_message(&crate::llm::user_message(&line, &[])));
        let item = injected_item(m);
        st.conv.items.push(item.clone());
        injected_items.push(item);
    }
    st.consumed_upto = tlen;
    let messages = st.conv.messages.clone();
    let group_title = s.title.clone();
    let group_topic = s.topic.clone();
    let group_members = s
        .members
        .iter()
        .map(|id| {
            settings
                .as_ref()
                .and_then(|st| st.agent(id))
                .map(|a| a.name.clone())
                .unwrap_or_else(|| id.clone())
        })
        .collect();
    session.persist(s);
    Some(AgentRun {
        agent_id: agent_id.to_string(),
        group_title,
        group_topic,
        group_members,
        config,
        messages,
        injected_items,
    })
}

/// One agent's worker: drains pending → runs one coalesced loop → repeats until
/// nothing is pending, then exits. Re-entry is guarded by `GroupState.cancels`; the
/// "no pending → exit" decision is atomic with that map so a message arriving as
/// the worker winds down is never lost (either this worker sees it and loops, or
/// `wake_agents` spawns a fresh one).
async fn agent_worker(session: Arc<GroupSession>, agent_id: String, cancel: CancellationToken) {
    loop {
        let run = {
            let mut s = session.state.lock().await;
            // Stop/pause/delete cancelled this worker, or it was removed from the
            // room: exit without touching the token map (the canceller drained it).
            if cancel.is_cancelled() {
                return;
            }
            if session.deleted.load(Ordering::SeqCst) {
                s.cancels.remove(&agent_id);
                return;
            }
            let tlen = s.transcript.len();
            if s.paused || !s.members.contains(&agent_id) || !has_pending(&s, &agent_id, tlen) {
                s.cancels.remove(&agent_id);
                session.persist(&s);
                return;
            }
            prepare_run(&session, &mut s, &agent_id, tlen)
        };

        let Some(run) = run else {
            // No usable config — its pending messages were consumed in prepare_run.
            let mut s = session.state.lock().await;
            s.cancels.remove(&agent_id);
            session.persist(&s);
            return;
        };

        run_one_agent(&session, run, &cancel).await;
        // Loop: messages may have arrived during the run — coalesce and continue.
    }
}

/// Run one agent's reaction (no state lock held during the LLM call) and persist
/// the result into its private session. `cancel` is the worker's stop token: it
/// reaches the tools through `ToolContext`, and a cancelled run must not post any
/// further room output (same as the old hard abort).
async fn run_one_agent(session: &Arc<GroupSession>, run: AgentRun, cancel: &CancellationToken) {
    let group_id = {
        let s = session.state.lock().await;
        s.id.clone()
    };

    // Surface the incoming group lines to the Agent sub-tab before the agent
    // starts reasoning (injection isn't a sink event).
    session.events.injected(&group_id, &run.agent_id, &run.injected_items);

    let conv = run.messages.clone();

    let ctx = ToolContext::for_session(
        ToolStores::new(
            LogStore(session.log_store.0.clone()),
            session.shell_store.clone(),
            session.mcp_store.clone(),
        ),
        run.config.clone(),
        format!("group:{group_id}:{}", run.agent_id),
    )
    .with_kind(RunKind::Group)
    .with_log_session(crate::logging::LogSession::group(&group_id, &run.agent_id))
    .with_cancel(cancel.clone());

    // Completed `<public>` blocks go through a channel to a poster task. That
    // keeps room delivery in generation order while the LLM run continues, and
    // lets the sync sink hand off without blocking on the async room lock.
    let (public_tx, mut public_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let poster = {
        let session = session.clone();
        let agent_id = run.agent_id.clone();
        tokio::spawn(async move {
            while let Some(message) = public_rx.recv().await {
                session.post_agent_message(&agent_id, &message).await;
            }
        })
    };

    let sink = GroupSink::new(
        session.events.clone(),
        group_id.clone(),
        run.agent_id.clone(),
        public_tx,
        cancel.clone(),
    );

    let result = run_agent(ChatAgentRun {
        conv,
        turn: None,
        prompt: SystemPrompt::Group {
            agent_id: &run.agent_id,
            title: &run.group_title,
            topic: &run.group_topic,
            members: &run.group_members,
        },
        sink: &sink,
        config: &run.config,
        mcp_store: &session.mcp_store,
        ctx: &ctx,
        max_turns: None,
    })
    .await;
    let items = sink.take_items();
    // Closing the sender lets the poster finish, then wait for every block it
    // still holds before persisting this run's display items.
    drop(sink);
    let _ = poster.await;

    {
        let mut s = session.state.lock().await;
        // A cancelled run's output belongs to the interrupted loop: drop it
        // instead of writing a half-turn back (same effect as the old abort).
        if !cancel.is_cancelled() {
            if let Some(st) = s.agents.get_mut(&run.agent_id) {
                match result {
                    Ok(outcome) => st.conv.messages = outcome.messages,
                    Err(e) => {
                        eprintln!("group agent {} failed: {}", run.agent_id, e);
                    }
                }
                st.conv.items.extend(items);
                s.updated_at = now_ms();
                // NOTE: do NOT advance `consumed_upto` here. `prepare_run` already moved
                // it to this run's start length; messages that arrived from OTHER agents
                // DURING this run must stay unconsumed so the worker's next iteration
                // injects them. (The agent's own posts are skipped by `has_pending` /
                // `prepare_run`'s self filter, so they don't trigger a needless re-run.)
            }
        }
        session.persist(&s);
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
///
/// Text is additionally run through a [`PublicFilter`]: only complete
/// `<public>…</public>` blocks are posted to the room, while the agent's private
/// sub-tab still sees the full text (without the tag markers).
struct GroupSink {
    events: Arc<dyn GroupEvents>,
    group_id: String,
    agent_id: String,
    items: std::sync::Mutex<ItemBuilder>,
    public: std::sync::Mutex<PublicState>,
    /// Completed blocks, posted in order by the task started in `run_one_agent`.
    public_tx: tokio::sync::mpsc::UnboundedSender<String>,
    /// This run's stop token. A cancelled run drops its pending public output
    /// instead of flushing it into the room.
    cancel: CancellationToken,
}

#[derive(Default)]
struct PublicState {
    filter: PublicFilter,
    /// The block currently being generated.
    buffer: String,
}

impl GroupSink {
    fn new(
        events: Arc<dyn GroupEvents>,
        group_id: String,
        agent_id: String,
        public_tx: tokio::sync::mpsc::UnboundedSender<String>,
        cancel: CancellationToken,
    ) -> Self {
        Self {
            events,
            group_id,
            agent_id,
            items: std::sync::Mutex::new(ItemBuilder::default()),
            public: std::sync::Mutex::new(PublicState::default()),
            public_tx,
            cancel,
        }
    }

    fn emit(&self, event: StreamEvent) {
        self.events.stream(&self.group_id, &self.agent_id, &event);
    }

    /// Consume the filter's output: private text is kept for this agent,
    /// closed public blocks are queued for the room.
    fn handle_pieces(&self, pieces: Vec<Piece>) {
        let mut display = String::new();
        let mut completed = Vec::new();
        {
            let mut state = self.public.lock().unwrap();
            for piece in pieces {
                match piece {
                    Piece::Private(text) => display.push_str(&text),
                    Piece::Public(text) => {
                        state.buffer.push_str(&text);
                        display.push_str(&text);
                    }
                    Piece::PublicEnd => {
                        let message = std::mem::take(&mut state.buffer);
                        let message = message.trim();
                        if !message.is_empty() {
                            completed.push(message.to_string());
                        }
                    }
                }
            }
        }

        // Both channels see the same tag-free text: the agent's own sub-tab is a
        // private view of the full stream, the room gets only the blocks above.
        if !display.is_empty() {
            self.items.lock().unwrap().chunk(&display);
            self.emit(StreamEvent::Chunk { text: display });
        }
        for message in completed {
            let _ = self.public_tx.send(message);
        }
    }

    /// Take the accumulated display items (after the run finishes).
    fn take_items(&self) -> Vec<ChatItem> {
        self.items.lock().unwrap().take_items()
    }
}

impl ChatEventSink for GroupSink {
    fn items(&self) -> Option<&std::sync::Mutex<ItemBuilder>> {
        Some(&self.items)
    }

    fn emit(&self, event: StreamEvent) {
        self.events.stream(&self.group_id, &self.agent_id, &event);
    }

    /// Text goes through the `<public>` filter first, so only complete room
    /// blocks are posted while the private sub-tab keeps the full text.
    fn send_chunk(&self, text: &str) {
        let pieces = self.public.lock().unwrap().filter.push(text);
        self.handle_pieces(pieces);
    }

    fn send_usage(&self, _total_tokens: u64, _context_window: u32) {
        // The group view has no per-agent context ring; ignore.
    }

    fn send_done(&self) {
        // Stop/pause/remove/delete interrupted this run: do not flush the
        // unterminated block, the room must not gain new output (the old hard
        // abort dropped it too). `agent_done` still clears the UI indicator.
        if self.cancel.is_cancelled() {
            return;
        }
        // An unterminated `<public>` still counts as public: the model simply
        // ran out of output. Flushing here also releases any held tag prefix.
        let pieces = self.public.lock().unwrap().filter.finish();
        self.handle_pieces(pieces);
        self.items.lock().unwrap().done();
        self.emit(StreamEvent::Done {});
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct RecordingEvents {
        chunks: std::sync::Mutex<String>,
        messages: std::sync::Mutex<Vec<String>>,
    }

    impl GroupEvents for RecordingEvents {
        fn message(&self, _group_id: &str, msg: &GroupMessage) {
            self.messages.lock().unwrap().push(msg.content.clone());
        }

        fn stream(&self, _group_id: &str, _agent_id: &str, event: &StreamEvent) {
            if let StreamEvent::Chunk { text } = event {
                self.chunks.lock().unwrap().push_str(text);
            }
        }

        fn injected(&self, _group_id: &str, _agent_id: &str, _items: &[ChatItem]) {}

        fn agent_done(&self, _group_id: &str, _agent_id: &str) {}
    }

    /// The injected line is an XML-ish wrapper named after the speaker, so the
    /// model can tell who said what without the old `[HH:MM] name:` prefix.
    #[test]
    fn injected_lines_are_wrapped_in_sender_tags() {
        let human = GroupMessage {
            id: "1".into(),
            speaker_kind: "human".into(),
            agent_id: None,
            name: "主人".into(),
            content: "在吗".into(),
            ts: 1,
        };
        assert_eq!(injected_line(&human), "<chat-主人-message>在吗</chat-主人-message>");

        let agent = GroupMessage {
            id: "2".into(),
            speaker_kind: "agent".into(),
            agent_id: Some("owen".into()),
            name: "小 宠".into(),
            content: "晚上好".into(),
            ts: 2,
        };
        assert_eq!(injected_line(&agent), "<chat-小-宠-message>晚上好</chat-小-宠-message>");
    }

    /// The display item carries the sender separately, so the Agent sub-tab can
    /// render a chat row without parsing the tag back out of the content.
    #[test]
    fn injected_items_carry_the_sender_for_the_ui() {
        let agent = GroupMessage {
            id: "2".into(),
            speaker_kind: "agent".into(),
            agent_id: Some("owen".into()),
            name: "小宠".into(),
            content: "晚上好".into(),
            ts: 42,
        };
        let ChatItem::Group { content, sender_kind, sender_id, sender_name, ts, .. } =
            injected_item(&agent)
        else {
            panic!("expected a group item")
        };
        assert_eq!(content, "晚上好");
        assert_eq!(sender_kind, "agent");
        assert_eq!(sender_id.as_deref(), Some("owen"));
        assert_eq!(sender_name, "小宠");
        assert_eq!(ts, 42);
    }

    /// The room gets complete `<public>` blocks only, in generation order —
    /// including a block whose opening tag was split across chunks and a block
    /// whose closing tag was. The agent's private view keeps all the text, with
    /// the markers stripped.
    #[tokio::test]
    async fn the_sink_posts_only_closed_public_blocks_in_order() {
        let events = Arc::new(RecordingEvents::default());
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let sink = GroupSink::new(
            events.clone(),
            "g".into(),
            "a".into(),
            tx,
            CancellationToken::new(),
        );

        sink.send_chunk("thinking <pub");
        sink.send_chunk("lic>one</public> between");
        sink.send_chunk(" <public>two</public");
        sink.send_chunk(">");
        sink.send_done();
        drop(sink);

        let mut posted = Vec::new();
        while let Ok(message) = rx.try_recv() {
            posted.push(message);
        }
        assert_eq!(posted, vec!["one", "two"]);
        assert_eq!(events.chunks.lock().unwrap().as_str(), "thinking one between two");
    }

    /// A stopped/paused run must not flush its unfinished block into the room:
    /// the cancellation token is what the old hard abort used to guarantee by
    /// dropping the whole future.
    #[tokio::test]
    async fn a_cancelled_sink_drops_pending_public_output() {
        let events = Arc::new(RecordingEvents::default());
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let cancel = CancellationToken::new();
        let sink = GroupSink::new(events, "g".into(), "a".into(), tx, cancel.clone());

        sink.send_chunk("<public>never posted");
        cancel.cancel();
        sink.send_done();
        drop(sink);

        assert!(rx.try_recv().is_err());
    }

    /// Thinking is part of the Agent sub-tab's standard chat rendering, so it
    /// must reach the persisted item (not only the live stream).
    #[tokio::test]
    async fn the_sink_persists_reasoning_on_the_assistant_item() {
        let events = Arc::new(RecordingEvents::default());
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let sink = GroupSink::new(events, "g".into(), "a".into(), tx, CancellationToken::new());

        sink.send_reasoning("let me think");
        sink.send_chunk("answer");
        sink.send_done();

        let items = sink.take_items();
        assert_eq!(items.len(), 1);
        let ChatItem::Assistant { content, reasoning, .. } = &items[0] else {
            panic!("expected an assistant item")
        };
        assert_eq!(content, "answer");
        assert_eq!(reasoning.as_deref(), Some("let me think"));
    }

    /// A group file exactly as the previous struct wrote it: each agent's
    /// `messages`/`items` sit next to `consumed_upto`, not under `conv`.
    #[test]
    fn disk_format_group_json_keeps_agent_messages_and_items_at_top_level() {
        let disk = r#"{
            "id": "g-1",
            "title": "闲聊",
            "topic": "",
            "transcript": [
                { "id": "m1", "speaker_kind": "human", "name": "主人", "content": "Hello", "ts": 1 }
            ],
            "members": ["owen"],
            "agents": {
                "owen": {
                    "messages": [
                        { "content": [{ "Text": "<chat-主人-message>Hello</chat-主人-message>" }], "options": null, "role": "User" }
                    ],
                    "items": [
                        { "id": "i1", "ts": 1, "type": "group", "content": "Hello", "senderKind": "human", "senderName": "主人" }
                    ],
                    "consumed_upto": 1
                }
            },
            "paused": true,
            "created_at": 1,
            "updated_at": 2
        }"#;

        let g: GroupState = serde_json::from_str(disk).expect("old group files must parse");
        let owen = &g.agents["owen"];
        assert_eq!(owen.conv.messages.len(), 1);
        assert_eq!(owen.conv.items.len(), 1);
        assert_eq!(owen.consumed_upto, 1);

        let written = serde_json::to_value(&g).unwrap();
        assert_eq!(written["agents"]["owen"]["messages"][0]["role"], "User");
        assert_eq!(written["agents"]["owen"]["items"][0]["type"], "group");
        assert!(written["agents"]["owen"].get("conv").is_none());
    }
}

