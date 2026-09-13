//! Backend-owned chat turns.
//!
//! A turn — one user input and everything the pipeline streams back for it,
//! tool rounds included — is started, streamed, persisted and finished HERE,
//! never in an interface. Interfaces are observers: they start a turn with
//! [`TurnRunner::send`], watch it through [`TurnEvents`], and can (re)attach to
//! a running one at any moment with [`TurnRunner::attach`], which replays what
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
use tokio_util::sync::CancellationToken;

use crate::chat::{run_chat_pipeline, ChatEventSink, ChatOutcome, ItemBuilder, StreamEvent, UserTurn};
use crate::config::AiConfig;
use crate::logging::{write_log, LogStore};
use crate::mcp::McpManagerStore;
use crate::session::{self, ContextUsage, Session, DEFAULT_SESSION_TITLE};
use crate::shell::{ShellStore, TaskCompletion, TaskNotifier};
use crate::tools::ToolContext;

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

/// Everything a running turn has streamed so far, for a listener joining late.
/// Replaying `events` through the same reducer a live listener uses yields the
/// same view; live events with `seq` greater than this one continue it.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnSnapshot {
    pub turn_id: String,
    pub origin: TurnOrigin,
    pub seq: u64,
    pub events: Vec<StreamEvent>,
}

struct LiveTurn {
    turn_id: String,
    origin: TurnOrigin,
    cancel: CancellationToken,
    /// Last emitted `seq` and the events so far. Consecutive text deltas are
    /// merged (see `coalesce`), so the buffer stays small on long answers.
    buffer: Mutex<(u64, Vec<StreamEvent>)>,
}

/// Append `event` to the replay buffer, merging it into the previous event when
/// both are deltas of the same text stream. Replay stays event-for-event
/// equivalent because a reducer only ever appends delta text.
fn coalesce(buffer: &mut Vec<StreamEvent>, event: StreamEvent) {
    match (buffer.last_mut(), &event) {
        (Some(StreamEvent::Chunk { text: prev }), StreamEvent::Chunk { text }) => prev.push_str(text),
        (Some(StreamEvent::Reasoning { text: prev }), StreamEvent::Reasoning { text }) => {
            prev.push_str(text)
        }
        _ => buffer.push(event),
    }
}

pub struct TurnRunner {
    events: Arc<dyn TurnEvents>,
    log_store: LogStore,
    shell_store: ShellStore,
    mcp_store: McpManagerStore,
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
        log_store: LogStore,
        shell_store: ShellStore,
        mcp_store: McpManagerStore,
        rt: tokio::runtime::Handle,
    ) -> Arc<Self> {
        Arc::new_cyclic(|me| Self {
            events,
            log_store,
            shell_store,
            mcp_store,
            rt,
            live: Mutex::new(HashMap::new()),
            queued: Mutex::new(HashMap::new()),
            me: me.clone(),
        })
    }

    fn log(&self, msg: &str) {
        write_log(&self.log_store.0, msg);
    }

    /// Start a turn in `session_id` with the owner's input. The user item is
    /// persisted before this returns, so any interface reloading the session
    /// sees it immediately. Fails if the session already has a turn running.
    pub fn send(&self, session_id: &str, turn: UserTurn) -> Result<String, String> {
        // Load before locking: the read is the slow part, and a concurrent
        // `send` for the same session is caught by the busy check below.
        let mut sess = session::load_session(session_id.to_string())?;
        let mut live = self.live.lock().unwrap();
        if live.contains_key(session_id) {
            return Err("A turn is already running in this session".to_string());
        }
        sess.items.push(session::user_item(&turn.text, &turn.images));
        let origin = TurnOrigin::User { text: turn.text.clone() };
        self.start(&mut live, sess, turn, origin)
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
        sess.items = session::apply_completion_to_items(sess.items, &c.task_id, &c.result);
        sess.items.push(session::notification_item(&label, &c.result));
        let turn = UserTurn::text(format!("[后台任务完成] {}：\n{}", label, c.result));
        if let Err(e) = self.start(live, sess, turn, TurnOrigin::Completion { label }) {
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
    ) -> Result<String, String> {
        if sess.title == DEFAULT_SESSION_TITLE {
            if let Some(t) = session::derive_title(&sess.items) {
                sess.title = t;
            }
        }
        sess.updated_at = crate::common::iso_now();
        let session_id = sess.id.clone();
        let prior_messages = sess.messages.clone();
        session::save_session(sess)?;

        let turn_id = uuid::Uuid::new_v4().to_string();
        let lt = Arc::new(LiveTurn {
            turn_id: turn_id.clone(),
            origin: origin.clone(),
            cancel: CancellationToken::new(),
            buffer: Mutex::new((0, Vec::new())),
        });
        live.insert(session_id.clone(), lt.clone());
        self.events.notice(&TurnNotice::Started {
            session_id: session_id.clone(),
            turn_id: turn_id.clone(),
            origin,
        });

        let me = self.me.upgrade().ok_or_else(|| "turn runner is shutting down".to_string())?;
        self.rt.spawn(async move { me.run(session_id, lt, prior_messages, turn).await });
        Ok(turn_id)
    }

    async fn run(
        self: Arc<Self>,
        session_id: String,
        lt: Arc<LiveTurn>,
        prior_messages: Vec<serde_json::Value>,
        turn: UserTurn,
    ) {
        let sink = TurnSink {
            runner: self.clone(),
            session_id: session_id.clone(),
            live: lt.clone(),
            state: Mutex::new(SinkState::default()),
        };
        // Config errors (no agent, no key) surface in the transcript like any
        // other failure, instead of failing `send` where only one window sees it.
        let outcome = match AiConfig::from_settings() {
            Err(e) => {
                sink.send_error(&e);
                Err(e)
            }
            Ok(config) => {
                let notifier: Arc<dyn TaskNotifier> = self.clone();
                let mut ctx = ToolContext::new(
                    LogStore(self.log_store.0.clone()),
                    ShellStore(self.shell_store.0.clone()),
                    config.clone(),
                    self.mcp_store.clone(),
                    session_id.clone(),
                    Some(notifier),
                    None, // chat turns aren't heartbeats; no chat hook
                    false,
                );
                ctx.cancel = lt.cancel.clone();
                let result =
                    run_chat_pipeline(prior_messages, turn, &sink, &config, &self.mcp_store, &ctx)
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
                sess.items.extend(items);
                if let Ok(o) = &outcome {
                    sess.messages = o.messages.clone();
                }
                if let Some((used, total)) = usage {
                    sess.context_usage = Some(ContextUsage { used, total: total as u64 });
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
        self.events.notice(&TurnNotice::Finished {
            session_id: session_id.to_string(),
            turn_id: lt.turn_id.clone(),
        });

        let next = self.queued.lock().unwrap().get_mut(session_id).and_then(|q| q.pop_front());
        if let Some(c) = next {
            let mut live = self.live.lock().unwrap();
            self.start_completion(&mut live, session_id.to_string(), c);
        }
    }

    /// The running turn in `session_id`, replayed from its start. `None` = idle.
    pub fn attach(&self, session_id: &str) -> Option<TurnSnapshot> {
        let lt = self.live.lock().unwrap().get(session_id).cloned()?;
        let buffer = lt.buffer.lock().unwrap();
        Some(TurnSnapshot {
            turn_id: lt.turn_id.clone(),
            origin: lt.origin.clone(),
            seq: buffer.0,
            events: buffer.1.clone(),
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

// --- TurnSink ------------------------------------------------------------------
// Fans one turn's events out to the listeners, records them for late joiners,
// and folds them into the display items that get persisted at the end.

#[derive(Default)]
struct SinkState {
    items: ItemBuilder,
    usage: Option<(u64, u32)>,
    saw_error: bool,
}

struct TurnSink {
    runner: Arc<TurnRunner>,
    session_id: String,
    live: Arc<LiveTurn>,
    state: Mutex<SinkState>,
}

impl TurnSink {
    fn emit(&self, event: StreamEvent) {
        // Held across the emit so buffer order and `seq` order can never
        // disagree with the order listeners saw.
        let mut buffer = self.live.buffer.lock().unwrap();
        buffer.0 += 1;
        let seq = buffer.0;
        coalesce(&mut buffer.1, event.clone());
        self.runner.events.notice(&TurnNotice::Stream {
            session_id: self.session_id.clone(),
            turn_id: self.live.turn_id.clone(),
            seq,
            event,
        });
    }

    fn saw_error(&self) -> bool {
        self.state.lock().unwrap().saw_error
    }

    fn take(self) -> (Vec<serde_json::Value>, Option<(u64, u32)>) {
        let mut st = self.state.into_inner().unwrap();
        (st.items.take_items(), st.usage)
    }
}

impl ChatEventSink for TurnSink {
    fn send_chunk(&self, text: &str) {
        self.state.lock().unwrap().items.chunk(text);
        self.emit(StreamEvent::Chunk { text: text.to_string() });
    }
    fn send_reasoning(&self, text: &str) {
        self.state.lock().unwrap().items.reasoning(text);
        self.emit(StreamEvent::Reasoning { text: text.to_string() });
    }
    fn send_tool_start(&self, name: &str, arguments: &str) {
        self.state.lock().unwrap().items.tool_start(name, arguments);
        self.emit(StreamEvent::ToolStart { name: name.to_string(), arguments: arguments.to_string() });
    }
    fn send_tool_result(&self, name: &str, result: &str) {
        self.state.lock().unwrap().items.tool_result(name, result);
        self.emit(StreamEvent::ToolResult { name: name.to_string(), result: result.to_string() });
    }
    fn send_image(&self, data_url: &str) {
        self.state.lock().unwrap().items.image(data_url);
        self.emit(StreamEvent::Image { data_url: data_url.to_string() });
    }
    fn send_usage(&self, prompt_tokens: u64, total_tokens: u64, context_window: u32) {
        self.state.lock().unwrap().usage = Some((total_tokens, context_window));
        self.emit(StreamEvent::Usage { prompt_tokens, total_tokens, context_window });
    }
    fn send_done(&self) {
        self.state.lock().unwrap().items.done();
        self.emit(StreamEvent::Done {});
    }
    fn send_error(&self, message: &str) {
        {
            let mut st = self.state.lock().unwrap();
            st.items.error(message);
            st.saw_error = true;
        }
        self.emit(StreamEvent::Error { message: message.to_string() });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(t: &str) -> StreamEvent {
        StreamEvent::Chunk { text: t.into() }
    }

    #[test]
    fn replay_buffer_merges_text_deltas_but_keeps_event_order() {
        let mut buf = Vec::new();
        coalesce(&mut buf, chunk("he"));
        coalesce(&mut buf, chunk("llo"));
        coalesce(&mut buf, StreamEvent::Reasoning { text: "hm".into() });
        coalesce(&mut buf, StreamEvent::Reasoning { text: "m".into() });
        coalesce(&mut buf, StreamEvent::ToolStart { name: "bash".into(), arguments: "{}".into() });
        // Text after a tool call must start a NEW chunk: merging it into the
        // one before the call would replay the answer in the wrong order.
        coalesce(&mut buf, chunk("done"));

        assert_eq!(buf.len(), 4);
        assert!(matches!(&buf[0], StreamEvent::Chunk { text } if text == "hello"));
        assert!(matches!(&buf[1], StreamEvent::Reasoning { text } if text == "hmm"));
        assert!(matches!(&buf[2], StreamEvent::ToolStart { .. }));
        assert!(matches!(&buf[3], StreamEvent::Chunk { text } if text == "done"));
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
    }
}
