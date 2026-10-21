//! The single event stream the TUI loop consumes: terminal input, chat-turn
//! activity and group activity all funnel into one channel so the UI has
//! exactly one place where state changes.

use std::sync::{Arc, Mutex};

use pet_core::chat::StreamEvent;
use pet_core::group::{GroupEvents, GroupMessage};
use pet_core::turn::{TurnEvents, TurnNotice, TurnSnapshot};
use tokio::sync::mpsc::UnboundedSender;

use crate::Mode;

pub enum AppEvent {
    /// A crossterm terminal event (keys, resize).
    Term(ratatui::crossterm::event::Event),
    /// Chat-turn activity from the runner — every session's; the TUI keeps
    /// what concerns the session it shows.
    Turn(TurnNotice),
    /// Replay of a turn already running in the session being viewed (sent by
    /// the run loop after it attaches, at startup or on a session switch).
    TurnSnapshot(TurnSnapshot),
    /// A command handler finished (mode changes arrive separately).
    CommandDone,
    /// Informational line for the transcript.
    Notice(String),
    /// Error line for the transcript.
    ErrorNotice(String),
    /// Switch between chat and group mode.
    SetMode(Mode),
    /// A line landed in the shared group transcript (incl. the owner's own —
    /// the TUI renders the room only from these events, never locally).
    GroupMsg(GroupMessage),
    /// Tool/error activity from one group agent's private run.
    GroupStream { agent_id: String, event: StreamEvent },
    /// A group agent's worker finished one run.
    GroupAgentDone(String),
    /// Show a modal list overlay (built by a command handler).
    OpenPicker(crate::tui::picker::Picker),
    /// Recount of the active agent's available tools (built-in + MCP).
    ToolsCount(usize),
    Quit,
}

/// Turn-runner notices → the UI loop.
pub struct CliTurnEvents(pub UnboundedSender<AppEvent>);

impl TurnEvents for CliTurnEvents {
    fn notice(&self, notice: &TurnNotice) {
        let _ = self.0.send(AppEvent::Turn(notice.clone()));
    }
}

/// Group orchestrator events → the UI loop. Chunks/reasoning are dropped here:
/// concurrent agents' token streams would interleave unreadably, so the room
/// shows finished messages plus a light tool/error trace. Events from other
/// rooms (the GUI can run several at once) are ignored.
pub struct TuiGroupEvents {
    pub tx: UnboundedSender<AppEvent>,
    /// The room the TUI is currently showing (empty = group mode not entered).
    pub group_id: Arc<Mutex<String>>,
}

impl TuiGroupEvents {
    fn active(&self, group_id: &str) -> bool {
        self.group_id
            .lock()
            .map(|id| id.as_str() == group_id)
            .unwrap_or(false)
    }
}

impl GroupEvents for TuiGroupEvents {
    fn message(&self, group_id: &str, msg: &GroupMessage) {
        if self.active(group_id) {
            let _ = self.tx.send(AppEvent::GroupMsg(msg.clone()));
        }
    }

    fn stream(&self, group_id: &str, agent_id: &str, event: &StreamEvent) {
        if !self.active(group_id) {
            return;
        }
        match event {
            StreamEvent::ToolStart { .. }
            | StreamEvent::ToolResult { .. }
            | StreamEvent::Error { .. } => {
                let _ = self.tx.send(AppEvent::GroupStream {
                    agent_id: agent_id.to_string(),
                    event: event.clone(),
                });
            }
            _ => {}
        }
    }

    fn injected(&self, _group_id: &str, _agent_id: &str, _items: &[serde_json::Value]) {}

    fn agent_done(&self, group_id: &str, agent_id: &str) {
        if self.active(group_id) {
            let _ = self.tx.send(AppEvent::GroupAgentDone(agent_id.to_string()));
        }
    }
}

/// Spawn the blocking crossterm reader thread. It parks on `event::read()` and
/// forwards everything; it dies with the process (the channel closing just
/// stops the sends).
pub fn spawn_term_reader(tx: UnboundedSender<AppEvent>) {
    std::thread::spawn(move || loop {
        match ratatui::crossterm::event::read() {
            Ok(ev) => {
                if tx.send(AppEvent::Term(ev)).is_err() {
                    return;
                }
            }
            Err(_) => return,
        }
    });
}
