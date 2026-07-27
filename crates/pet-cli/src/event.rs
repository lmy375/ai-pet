//! The single event stream the TUI loop consumes: terminal input, chat-turn
//! stream events, group activity and background-task completions all funnel
//! into one channel so the UI has exactly one place where state changes.

use pet_core::chat::StreamEvent;
use pet_core::group::{GroupEvents, GroupMessage};
use pet_core::shell::{TaskCompletion, TaskNotifier};
use tokio::sync::mpsc::UnboundedSender;

use crate::Mode;

pub enum AppEvent {
    /// A crossterm terminal event (keys, resize).
    Term(ratatui::crossterm::event::Event),
    /// Stream event from the current single-agent chat turn.
    Stream(StreamEvent),
    /// The chat turn finished (session already saved). Err = transport failure
    /// that never reached the sink as an Error event.
    TurnDone(Result<(), String>),
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
    /// A background task finished.
    TaskDone(TaskCompletion),
    /// Show a modal list overlay (built by a command handler).
    OpenPicker(crate::tui::picker::Picker),
    /// Recount of the active agent's available tools (built-in + MCP).
    ToolsCount(usize),
    Quit,
}

/// Background-task completions → the UI loop.
pub struct CliNotifier(pub UnboundedSender<AppEvent>);

impl TaskNotifier for CliNotifier {
    fn notify(&self, completion: &TaskCompletion) {
        let _ = self.0.send(AppEvent::TaskDone(completion.clone()));
    }
}

/// Group orchestrator events → the UI loop. Chunks/reasoning are dropped here:
/// concurrent agents' token streams would interleave unreadably, so the room
/// shows finished messages plus a light tool/error trace.
pub struct TuiGroupEvents(pub UnboundedSender<AppEvent>);

impl GroupEvents for TuiGroupEvents {
    fn message(&self, msg: &GroupMessage) {
        let _ = self.0.send(AppEvent::GroupMsg(msg.clone()));
    }

    fn stream(&self, agent_id: &str, event: &StreamEvent) {
        match event {
            StreamEvent::ToolStart { .. }
            | StreamEvent::ToolResult { .. }
            | StreamEvent::Error { .. } => {
                let _ = self.0.send(AppEvent::GroupStream {
                    agent_id: agent_id.to_string(),
                    event: event.clone(),
                });
            }
            _ => {}
        }
    }

    fn injected(&self, _agent_id: &str, _items: &[serde_json::Value]) {}

    fn agent_done(&self, agent_id: &str) {
        let _ = self.0.send(AppEvent::GroupAgentDone(agent_id.to_string()));
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
