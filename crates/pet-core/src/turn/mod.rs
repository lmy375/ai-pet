//! Backend-owned chat turns.
//!
//! A turn — one user input and everything the pipeline streams back for it,
//! tool rounds included — is started, streamed, persisted and finished HERE,
//! never in an interface. Interfaces are observers: they start a turn with
//! [`TurnRunner::send`], watch it through [`TurnEvents`], and can (re)attach to
//! a running one at any moment with [`TurnRunner::attach`], which returns what
//! has streamed so far. So a window can be switched, hidden, reloaded or
//! closed mid-turn and the turn neither stops nor loses its result, and any
//! number of sessions can run turns at once (one turn per session).
//!
//! Background-task completions are delivered here too (the runner is the
//! [`TaskNotifier`]): it appends the notification to the task's session and
//! resumes that session with a follow-up turn, queued behind a running one. No
//! interface code is involved, so a completion is injected exactly once no
//! matter how many windows are open — or none.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, Weak};

use serde::Serialize;

use crate::chat::{run_agent, AgentRun, ChatEventSink, ChatOutcome, StreamEvent, UserTurn};
use crate::config::AiConfig;
use crate::item::{ChatItem, ItemTail};
use crate::logging::write_log;
use crate::system_prompt::SystemPrompt;
use crate::session::{self, ContextUsage, Session, DEFAULT_SESSION_TITLE};
use crate::shell::{TaskCompletion, TaskNotifier};
use crate::stores::EngineStores;
use crate::tools::{ToolContext, ToolStores};

mod sink;

use sink::{LiveTurn, TurnSink};

/// Where turn activity goes. One global stream for every session; the
/// interface filters by `session_id`. Implemented by the Tauri layer (a `turn`
/// window event) and the CLI (its event channel).
pub trait TurnEvents: Send + Sync {
    fn notice(&self, notice: &TurnNotice);
}

/// What started a turn. Carried on `Started` (and in a snapshot) so an
/// interface that didn't start the turn itself can still echo its opening line.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum TurnOrigin {
    User { text: String },
    Completion { label: String },
}

/// Serialized for the GUI's `turn` event, which `useChat.ts` reads as
/// `sessionId` / `turnId`: `rename_all` alone only renames the variants, so the
/// variant fields need `rename_all_fields` too (see the wire-shape test below).
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase", tag = "kind")]
pub enum TurnNotice {
    /// A turn began. Its opening item(s) are already on disk.
    Started {
        session_id: String,
        turn_id: String,
        origin: TurnOrigin,
        /// Epoch ms the turn began. The runner stamps it (rather than each
        /// listener noting its own arrival time) so a window that joins late
        /// through `attach` shows the same elapsed time as one that was there
        /// from the start.
        started_at: i64,
    },
    /// One stream event. `seq` counts from 1 within the turn so a listener can
    /// tell whether it missed anything (and re-attach if so).
    Stream {
        session_id: String,
        turn_id: String,
        seq: u64,
        event: StreamEvent,
    },
    /// The turn is over and the session file holds its result. Listeners
    /// reload the session and drop their in-progress view.
    Finished { session_id: String, turn_id: String },
}

/// Everything a running turn has produced, for a listener joining late. The
/// items and tail come from the backend's own builder, so the interface never
/// has to fold the event stream itself.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnSnapshot {
    pub turn_id: String,
    pub origin: TurnOrigin,
    /// Epoch ms the turn began — see `TurnNotice::Started`.
    pub started_at: i64,
    pub seq: u64,
    #[serde(flatten)]
    pub tail: ItemTail,
    /// Latest round's context occupancy, and tokens spent by the turn so far.
    pub usage: Option<ContextUsage>,
    pub spent: u64,
}

pub struct TurnRunner {
    pub(super) events: Arc<dyn TurnEvents>,
    stores: EngineStores,
    /// Where turns run. Captured at construction so a turn can be started from
    /// any thread (a background waiter delivering a completion, a sync command).
    rt: tokio::runtime::Handle,
    /// The turn in flight per session id. Presence == busy.
    live: Mutex<HashMap<String, Arc<LiveTurn>>>,
    /// Completions that arrived while their session had a turn running.
    queued: Mutex<HashMap<String, VecDeque<TaskCompletion>>>,
    me: Weak<TurnRunner>,
}

impl TurnRunner {
    pub fn new(
        events: Arc<dyn TurnEvents>,
        stores: EngineStores,
        rt: tokio::runtime::Handle,
    ) -> Arc<Self> {
        Arc::new_cyclic(|me| Self {
            events,
            stores,
            rt,
            live: Mutex::new(HashMap::new()),
            queued: Mutex::new(HashMap::new()),
            me: me.clone(),
        })
    }

    fn log(&self, msg: &str) {
        write_log(&self.stores.log.0, msg);
    }

    /// Start a turn in `session_id` with the owner's input. The user item is
    /// persisted before this returns, so any interface reloading the session
    /// sees it immediately. Fails if the session already has a turn running.
    pub fn send(&self, session_id: &str, turn: UserTurn) -> Result<String, String> {
        self.send_with(session_id, None, None, turn)
    }

    /// Start a turn as a specific agent, with a per-turn observer for its
    /// notices. Telegram bots use this: they serve one agent regardless of
    /// which one is active in the GUI, and need the stream to reply in chat.
    pub fn send_for_agent(
        &self,
        session_id: &str,
        agent_id: &str,
        observer: Arc<dyn TurnEvents>,
        turn: UserTurn,
    ) -> Result<String, String> {
        self.send_with(session_id, Some(agent_id.to_string()), Some(observer), turn)
    }

    fn send_with(
        &self,
        session_id: &str,
        agent_id: Option<String>,
        observer: Option<Arc<dyn TurnEvents>>,
        turn: UserTurn,
    ) -> Result<String, String> {
        // Load before locking: the read is the slow part, and a concurrent
        // `send` for the same session is caught by the busy check below.
        let mut sess = session::load_session(session_id.to_string())?;
        let mut live = self.live.lock().unwrap();
        if live.contains_key(session_id) {
            return Err("A turn is already running in this session".to_string());
        }
        sess.conv.items.push(ChatItem::user(&turn.text, &turn.images));
        let origin = TurnOrigin::User { text: turn.text.clone() };
        self.start(&mut live, sess, turn, origin, agent_id, observer)
    }

    /// Inject a finished background task into the session that spawned it and
    /// resume that session. Queued if a turn is running there; the queue drains
    /// one completion per turn as each finishes.
    pub fn deliver(&self, completion: TaskCompletion) {
        // A completion without a session (a task registered outside any chat
        // turn) goes to the conversation the owner is looking at.
        let session_id = if completion.session_id.is_empty() {
            session::list_sessions().active_id
        } else {
            completion.session_id.clone()
        };
        if session_id.is_empty() {
            self.log(&format!("completion of task {} dropped: no session", completion.task_id));
            return;
        }
        let mut live = self.live.lock().unwrap();
        if live.contains_key(&session_id) {
            self.queued.lock().unwrap().entry(session_id).or_default().push_back(completion);
            return;
        }
        self.start_completion(&mut live, session_id, completion);
    }

    /// Caller holds the `live` lock (so the busy check and the insert are one step).
    fn start_completion(
        &self,
        live: &mut HashMap<String, Arc<LiveTurn>>,
        session_id: String,
        c: TaskCompletion,
    ) {
        let mut sess = match session::load_session(session_id.clone()) {
            Ok(s) => s,
            Err(e) => {
                self.log(&format!("completion of task {} dropped: {e}", c.task_id));
                return;
            }
        };
        let label = if c.label.is_empty() { c.kind.clone() } else { c.label.clone() };
        // Flip the originating tool call from "running in background" to its
        // result, then open the resumption turn with a notification line.
        sess.conv.items = session::apply_completion_to_items(sess.conv.items, &c.task_id, &c.result);
        sess.conv.items.push(ChatItem::notification(&label, &c.result));
        let turn = UserTurn::text(format!("[后台任务完成] {}：\n{}", label, c.result));
        if let Err(e) = self.start(live, sess, turn, TurnOrigin::Completion { label }, None, None) {
            self.log(&format!("completion turn for task {} failed to start: {e}", c.task_id));
        }
    }

    /// Persist the opening item(s), register the turn and run it. Caller holds
    /// the `live` lock and has verified the session is idle.
    fn start(
        &self,
        live: &mut HashMap<String, Arc<LiveTurn>>,
        mut sess: Session,
        turn: UserTurn,
        origin: TurnOrigin,
        agent_id: Option<String>,
        observer: Option<Arc<dyn TurnEvents>>,
    ) -> Result<String, String> {
        if sess.title == DEFAULT_SESSION_TITLE {
            if let Some(t) = session::derive_title(&sess.conv.items) {
                sess.title = t;
            }
        }
        sess.updated_at = crate::common::iso_now();
        let session_id = sess.id.clone();
        let prior_messages = sess.conv.messages.clone();
        session::save_session(sess)?;

        let turn_id = uuid::Uuid::new_v4().to_string();
        let started_at = crate::common::now_ms();
        let lt = Arc::new(LiveTurn::new(turn_id.clone(), origin.clone(), started_at, observer));
        live.insert(session_id.clone(), lt.clone());
        let notice = TurnNotice::Started {
            session_id: session_id.clone(),
            turn_id: turn_id.clone(),
            origin,
            started_at,
        };
        self.events.notice(&notice);
        lt.notify_observer(&notice);

        let me = self.me.upgrade().ok_or_else(|| "turn runner is shutting down".to_string())?;
        self.rt.spawn(async move {
            me.run(session_id, lt, prior_messages, turn, agent_id).await
        });
        Ok(turn_id)
    }

    async fn run(
        self: Arc<Self>,
        session_id: String,
        lt: Arc<LiveTurn>,
        prior_messages: Vec<serde_json::Value>,
        turn: UserTurn,
        agent_id: Option<String>,
    ) {
        let sink = TurnSink::new(self.clone(), session_id.clone(), lt.clone());
        // Config errors (no agent, no key) surface in the transcript like any
        // other failure, instead of failing `send` where only one window sees it.
        // Per-agent turns (Telegram) resolve their own agent, not the active one.
        let config = match &agent_id {
            Some(id) => AiConfig::from_agent_id(id),
            None => AiConfig::from_settings(),
        };
        let outcome = match config {
            Err(e) => {
                sink.send_error(&e);
                Err(e)
            }
            Ok(config) => {
                // Only UI-owned turns resume after a background task; a Telegram
                // bot has no window to push the completion to.
                let notifier: Option<Arc<dyn TaskNotifier>> = if agent_id.is_none() {
                    Some(self.clone() as Arc<dyn TaskNotifier>)
                } else {
                    None
                };
                let ctx = ToolContext::for_session(
                    ToolStores::new(
                        self.stores.log.clone(),
                        self.stores.shell.clone(),
                        self.stores.mcp.clone(),
                    ),
                    config.clone(),
                    session_id.clone(),
                )
                .with_notifier(notifier)
                .with_cancel(lt.cancel.clone());
                let result = run_agent(AgentRun {
                    conv: prior_messages,
                    turn: Some(turn),
                    prompt: SystemPrompt::Chat { agent_id: &config.agent_id },
                    sink: &sink,
                    config: &config,
                    mcp_store: &self.stores.mcp,
                    ctx: &ctx,
                    max_turns: None,
                })
                .await;
                // The LLM layer reports its own failures through the sink before
                // returning them; only a failure that never did gets an item here.
                if let Err(e) = &result {
                    if !sink.saw_error() {
                        sink.send_error(e);
                    }
                }
                result
            }
        };
        self.finish(&session_id, &lt, sink, outcome);
    }

    /// Persist the turn, release the session and emit `Finished` — in that
    /// order, so a listener reloading on `Finished` reads the final file and an
    /// `attach` after it finds nothing running.
    fn finish(
        &self,
        session_id: &str,
        lt: &Arc<LiveTurn>,
        sink: TurnSink,
        outcome: Result<ChatOutcome, String>,
    ) {
        let (items, usage) = sink.take();
        // Reload rather than reuse the snapshot taken at start: the display
        // transcript is appended to (never rewritten), so anything another
        // writer added meanwhile — the heartbeat's `chat` tool, say — survives.
        // The model-facing conversation is what the pipeline returned; on a
        // failed turn it is left as it was, so the failure isn't in context.
        match session::load_session(session_id.to_string()) {
            Ok(mut sess) => {
                sess.conv.items.extend(items);
                if let Ok(o) = &outcome {
                    sess.conv.messages = o.messages.clone();
                }
                if let Some(u) = usage {
                    sess.context_usage = Some(u);
                }
                sess.updated_at = crate::common::iso_now();
                if let Err(e) = session::save_session(sess) {
                    self.log(&format!("failed to save session {session_id}: {e}"));
                }
            }
            // Deleted while it ran: nothing to persist.
            Err(e) => self.log(&format!("turn result for {session_id} dropped: {e}")),
        }
        {
            let mut live = self.live.lock().unwrap();
            if live.get(session_id).is_some_and(|l| Arc::ptr_eq(l, lt)) {
                live.remove(session_id);
            }
        }
        let notice = TurnNotice::Finished {
            session_id: session_id.to_string(),
            turn_id: lt.turn_id.clone(),
        };
        self.events.notice(&notice);
        lt.notify_observer(&notice);

        let next = self.queued.lock().unwrap().get_mut(session_id).and_then(|q| q.pop_front());
        if let Some(c) = next {
            let mut live = self.live.lock().unwrap();
            self.start_completion(&mut live, session_id.to_string(), c);
        }
    }

    /// The running turn in `session_id`. `None` = idle.
    pub fn attach(&self, session_id: &str) -> Option<TurnSnapshot> {
        let lt = self.live.lock().unwrap().get(session_id).cloned()?;
        let state = lt.state.lock().unwrap();
        Some(TurnSnapshot {
            turn_id: lt.turn_id.clone(),
            origin: lt.origin.clone(),
            started_at: lt.started_at,
            seq: state.seq,
            tail: state.items.snapshot(),
            usage: state.usage.clone(),
            spent: state.spent,
        })
    }

    /// Stop the turn running in `session_id` (a no-op when idle). The pipeline
    /// ends the stream with `done`, keeping the partial answer, so the turn
    /// still finishes and persists through the normal path.
    pub fn cancel(&self, session_id: &str) {
        if let Some(lt) = self.live.lock().unwrap().get(session_id) {
            lt.cancel.cancel();
        }
    }

    pub fn is_running(&self, session_id: &str) -> bool {
        self.live.lock().unwrap().contains_key(session_id)
    }

    /// Ids of every session with a turn in flight.
    pub fn running(&self) -> Vec<String> {
        self.live.lock().unwrap().keys().cloned().collect()
    }

    /// True when no turn is running and no completion is waiting to start one.
    pub fn is_idle(&self) -> bool {
        self.live.lock().unwrap().is_empty()
            && self.queued.lock().unwrap().values().all(|q| q.is_empty())
    }
}

impl TaskNotifier for TurnRunner {
    fn notify(&self, completion: &TaskCompletion) {
        self.deliver(completion.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::turn::sink::LiveState;

    fn chunk(t: &str) -> StreamEvent {
        StreamEvent::Chunk { text: t.into() }
    }

    /// The GUI keys every notice on `sessionId` / `turnId` (`useChat.ts`); a
    /// snake_case field silently makes it drop the whole stream.
    #[test]
    fn turn_notice_wire_shape_matches_the_frontend() {
        let n = TurnNotice::Stream {
            session_id: "s1".into(),
            turn_id: "t1".into(),
            seq: 3,
            event: chunk("hi"),
        };
        let v: serde_json::Value = serde_json::to_value(&n).unwrap();
        assert_eq!(
            v,
            serde_json::json!({
                "kind": "stream",
                "sessionId": "s1",
                "turnId": "t1",
                "seq": 3,
                "event": { "event": "chunk", "data": { "text": "hi" } }
            })
        );
        let f = TurnNotice::Finished { session_id: "s1".into(), turn_id: "t1".into() };
        let v: serde_json::Value = serde_json::to_value(&f).unwrap();
        assert_eq!(v, serde_json::json!({ "kind": "finished", "sessionId": "s1", "turnId": "t1" }));

        // `startedAt` is what the running indicator counts elapsed time from;
        // as snake_case it would read as `undefined` and the timer would sit at 0.
        let s = TurnNotice::Started {
            session_id: "s1".into(),
            turn_id: "t1".into(),
            origin: TurnOrigin::User { text: "hi".into() },
            started_at: 1_700_000_000_000,
        };
        let v: serde_json::Value = serde_json::to_value(&s).unwrap();
        assert_eq!(
            v,
            serde_json::json!({
                "kind": "started",
                "sessionId": "s1",
                "turnId": "t1",
                "origin": { "type": "user", "text": "hi" },
                "startedAt": 1_700_000_000_000i64
            })
        );
    }

    /// A late joiner reads the committed items and the streaming tail in one
    /// snapshot — no event replay needed.
    #[test]
    fn snapshot_wire_shape_carries_items_and_the_tail() {
        let mut state = LiveState::default();
        state.items.reasoning("hm");
        state.items.tool_start("bash", "{}");
        state.items.tool_result("bash", "ok");
        state.items.chunk("answer");
        state.seq = 4;
        state.spent = 120;
        state.usage = Some(ContextUsage { used: 120, total: 1000 });

        let snap = TurnSnapshot {
            turn_id: "t1".into(),
            origin: TurnOrigin::Completion { label: "task".into() },
            started_at: 1_700_000_000_000,
            seq: state.seq,
            tail: state.items.snapshot(),
            usage: state.usage,
            spent: state.spent,
        };
        let v = serde_json::to_value(&snap).unwrap();
        assert_eq!(v["turnId"], "t1");
        assert_eq!(v["origin"], serde_json::json!({ "type": "completion", "label": "task" }));
        assert_eq!(v["seq"], 4);
        assert_eq!(v["items"].as_array().unwrap().len(), 2);
        // Thinking committed as its own assistant item when the tool call began;
        // the tool group committed when the answer started streaming.
        assert_eq!(v["items"][0]["reasoning"], "hm");
        assert_eq!(v["items"][1]["type"], "tool");
        assert_eq!(v["items"][1]["toolCalls"][0]["result"], "ok");
        assert_eq!(v["response"], "answer");
        assert_eq!(v["reasoning"], "");
        assert_eq!(v["usage"], serde_json::json!({ "used": 120, "total": 1000 }));
        assert_eq!(v["spent"], 120);
    }
}