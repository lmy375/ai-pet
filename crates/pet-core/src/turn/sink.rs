use std::sync::{Arc, Mutex};

use tokio_util::sync::CancellationToken;

use super::{TurnEvents, TurnNotice, TurnOrigin, TurnRunner};
use crate::chat::{ChatEventSink, StreamEvent};
use crate::item::{ChatItem, ItemBuilder};
use crate::session::ContextUsage;

/// Everything the sink has recorded so far — read under one lock so `seq` and
/// the display transcript can never disagree.
#[derive(Default)]
pub(super) struct LiveState {
    pub(super) seq: u64,
    pub(super) items: ItemBuilder,
    pub(super) usage: Option<ContextUsage>,
    pub(super) spent: u64,
    saw_error: bool,
}

pub(super) struct LiveTurn {
    pub(super) turn_id: String,
    pub(super) origin: TurnOrigin,
    pub(super) started_at: i64,
    pub(super) cancel: CancellationToken,
    pub(super) state: Mutex<LiveState>,
    /// Extra per-turn listener (Telegram). Gets the same notices the global
    /// listener sees, for this turn only.
    observer: Option<Arc<dyn TurnEvents>>,
}

impl LiveTurn {
    pub(super) fn new(
        turn_id: String,
        origin: TurnOrigin,
        started_at: i64,
        observer: Option<Arc<dyn TurnEvents>>,
    ) -> Self {
        Self {
            turn_id,
            origin,
            started_at,
            cancel: CancellationToken::new(),
            state: Mutex::new(LiveState::default()),
            observer,
        }
    }

    pub(super) fn notify_observer(&self, notice: &TurnNotice) {
        if let Some(o) = &self.observer {
            o.notice(notice);
        }
    }
}

// --- TurnSink ------------------------------------------------------------------
// Fans one turn's events out to the listeners, records them for late joiners,
// and folds them into the display items that get persisted at the end.

pub(super) struct TurnSink {
    runner: Arc<TurnRunner>,
    session_id: String,
    live: Arc<LiveTurn>,
}

impl TurnSink {
    pub(super) fn new(runner: Arc<TurnRunner>, session_id: String, live: Arc<LiveTurn>) -> Self {
        Self { runner, session_id, live }
    }

    pub(super) fn saw_error(&self) -> bool {
        self.live.state.lock().unwrap().saw_error
    }

    pub(super) fn take(&self) -> (Vec<ChatItem>, Option<ContextUsage>) {
        let mut st = self.live.state.lock().unwrap();
        (std::mem::take(&mut st.items).take_items(), st.usage.clone())
    }
}

impl ChatEventSink for TurnSink {
    /// Fold the event and advance `seq` under one lock, so a concurrent
    /// `attach` can never see a display state that disagrees with its `seq`.
    fn emit(&self, event: StreamEvent) {
        let seq = {
            let mut st = self.live.state.lock().unwrap();
            match &event {
                StreamEvent::Chunk { text } => st.items.chunk(text),
                StreamEvent::Reasoning { text } => st.items.reasoning(text),
                StreamEvent::ToolStart { name, arguments } => {
                    st.items.tool_start(name, arguments);
                }
                StreamEvent::ToolResult { name, result } => st.items.tool_result(name, result),
                StreamEvent::Image { data_url } => st.items.image(data_url),
                StreamEvent::Usage { total_tokens, context_window } => {
                    st.usage = Some(ContextUsage {
                        used: *total_tokens,
                        total: *context_window as u64,
                    });
                    st.spent += total_tokens;
                }
                StreamEvent::Done {} => st.items.done(),
                StreamEvent::Error { message } => {
                    st.items.error(message);
                    st.saw_error = true;
                }
            }
            st.seq += 1;
            st.seq
        };
        let notice = TurnNotice::Stream {
            session_id: self.session_id.clone(),
            turn_id: self.live.turn_id.clone(),
            seq,
            event,
        };
        self.runner.events.notice(&notice);
        self.live.notify_observer(&notice);
    }
}

