//! TUI state machine: one struct owns the transcript, input line, palette,
//! selection and scroll state; terminal + engine events mutate it and the draw
//! module renders it. Effects (submits, turns, quit) are returned to the run
//! loop in `main.rs`, which owns the async plumbing.

pub mod draw;
pub mod entries;
pub mod markdown;
pub mod palette;
pub mod picker;
pub mod wrap;

mod action;
mod group_apply;
mod input;
mod keys;
mod picker_glue;
mod session_view;
mod state;
mod stream_apply;
mod viewport;

pub use action::Action;
pub use input::InputState;
pub use state::TuiApp;

use pet_core::chat::StreamEvent;
use pet_core::turn::{TurnNotice, TurnOrigin, TurnSnapshot};
use ratatui::crossterm::event::Event as TermEvent;
use ratatui::style::Color;

use crate::event::AppEvent;
use crate::Mode;
use entries::Entry;

impl TuiApp {
    // --- event application (returns follow-up actions) ---

    pub fn apply(&mut self, ev: AppEvent) -> Vec<Action> {
        match ev {
            AppEvent::Term(TermEvent::Key(key)) => return self.on_key(key),
            AppEvent::Term(TermEvent::Resize(..)) => {}
            AppEvent::Term(_) => {}
            AppEvent::Turn(notice) => self.apply_turn(notice),
            AppEvent::TurnSnapshot(snap) => self.apply_snapshot(snap),
            AppEvent::CommandDone => {
                self.busy = false;
                // A chat line whose turn failed to start leaves its local
                // placeholder behind; a command never has one.
                self.finish_streaming_assistant();
                self.prune_empty_assistant();
                self.refresh_header();
            }
            AppEvent::Notice(text) => self.push(Entry::Notice { text }),
            AppEvent::ErrorNotice(text) => self.push(Entry::Error { text }),
            AppEvent::SetMode(mode) => {
                self.mode = mode;
                self.sel_entry = None;
                self.refresh_palette();
                self.refresh_header();
            }
            AppEvent::GroupMsg(msg) => {
                if self.mode == Mode::Group {
                    let entry = match msg.agent_id.as_deref() {
                        Some(id) => {
                            let (name, color) = self.group_roster(id);
                            Entry::GroupMsg { name, color, text: msg.content }
                        }
                        None => Entry::GroupMsg {
                            name: msg.name,
                            color: Color::Green,
                            text: msg.content,
                        },
                    };
                    self.push(entry);
                }
            }
            AppEvent::GroupStream { agent_id, event } => self.apply_group_stream(agent_id, event),
            AppEvent::GroupAgentDone(agent_id) => {
                self.group_tools.remove(&agent_id);
            }
            AppEvent::OpenPicker(p) => self.picker = Some(p),
            AppEvent::ToolsCount(n) => self.tools_count = Some(n),
            AppEvent::Quit => return vec![Action::Quit],
        }
        vec![]
    }

    // --- backend-owned turn notices ---

    /// Turn activity for every session. Only the viewed session's turn is
    /// rendered; a completion that resumes another session gets one notice
    /// line (it runs on in the background, visible when that session is opened).
    fn apply_turn(&mut self, notice: TurnNotice) {
        match notice {
            TurnNotice::Started { session_id, origin, .. } => {
                if session_id == self.session_id {
                    self.open_turn(origin);
                } else if let TurnOrigin::Completion { label } = origin {
                    self.push(Entry::Notice {
                        text: format!("后台任务完成（其他会话）：{label}，已在后台续聊"),
                    });
                }
            }
            TurnNotice::Stream { session_id, event, .. } => {
                if session_id == self.session_id {
                    self.apply_stream(event);
                }
            }
            TurnNotice::Finished { session_id, .. } => {
                if session_id == self.session_id {
                    self.busy = false;
                    self.close_open_tool();
                    self.finish_streaming_assistant();
                    self.prune_empty_assistant();
                    self.refresh_header();
                }
            }
        }
    }

    /// Catch up with a turn already running in the viewed session (startup or
    /// session switch): echo its opening line, render what has been committed,
    /// then show the still-streaming tail.
    fn apply_snapshot(&mut self, snap: TurnSnapshot) {
        let TurnSnapshot { origin, tail, usage, .. } = snap;
        self.open_turn(origin);
        for item in &tail.items {
            self.push_item(item);
        }
        if !tail.reasoning.is_empty() {
            self.apply_stream(StreamEvent::Reasoning { text: tail.reasoning.clone() });
        }
        for call in &tail.tool_calls {
            self.apply_stream(StreamEvent::ToolStart {
                name: call.name.clone(),
                arguments: call.arguments.clone(),
            });
            if let Some(result) = &call.result {
                self.apply_stream(StreamEvent::ToolResult {
                    name: call.name.clone(),
                    result: result.clone(),
                });
            }
        }
        if !tail.response.is_empty() {
            self.apply_stream(StreamEvent::Chunk { text: tail.response });
        }
        if let Some(u) = usage {
            self.context_usage = Some((u.used, u.total));
        }
    }

    /// Mark the viewed session busy and make sure the turn's opening line and
    /// the streaming placeholder are on screen. A turn this view submitted
    /// itself already echoed both (see `submit`); one started elsewhere — a GUI
    /// window, or a background-task resumption — hasn't.
    fn open_turn(&mut self, origin: TurnOrigin) {
        let echoed = self.busy
            && matches!(self.entries.last(), Some(Entry::Assistant { streaming: true, .. }));
        self.busy = true;
        if echoed {
            return;
        }
        match origin {
            TurnOrigin::User { text } => self.push(Entry::User { text }),
            TurnOrigin::Completion { label } => self.push(Entry::Notice {
                text: format!("后台任务完成：{label} — 自动继续对话"),
            }),
        }
        let name = self.agent_name();
        self.push(Entry::Assistant {
            name,
            text: String::new(),
            reasoning: String::new(),
            streaming: true,
            reasoning_expanded: false,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn app() -> TuiApp {
        let mut a = TuiApp::new();
        a.entries.clear();
        a.line_cache.clear();
        a
    }

    fn started(a: &mut TuiApp, session_id: &str, origin: TurnOrigin) -> Vec<Action> {
        a.apply(AppEvent::Turn(TurnNotice::Started {
            session_id: session_id.into(),
            turn_id: "t".into(),
            origin,
            started_at: 0,
        }))
    }

    #[test]
    fn completion_turn_in_another_session_only_notices() {
        let mut a = app();
        let actions = started(
            &mut a,
            "some-other-session",
            TurnOrigin::Completion { label: "sleep".into() },
        );
        assert!(actions.is_empty());
        assert!(!a.busy, "the other session's turn must not lock this view");
        assert!(matches!(a.entries.last(), Some(Entry::Notice { .. })));
    }

    #[test]
    fn turn_started_elsewhere_is_echoed_and_own_turn_is_not_echoed_twice() {
        let mut a = app();
        let sid = a.session_id.clone();

        // Started by a GUI window: the view echoes the opening line itself.
        started(&mut a, &sid, TurnOrigin::User { text: "hi".into() });
        assert!(a.busy);
        assert!(matches!(
            &a.entries[..],
            [Entry::User { text }, Entry::Assistant { streaming: true, .. }] if text == "hi"
        ));
        a.apply(AppEvent::Turn(TurnNotice::Finished { session_id: sid.clone(), turn_id: "t".into() }));
        assert!(!a.busy);
        assert_eq!(a.entries.len(), 1, "an empty placeholder is pruned on finish");

        // Submitted here: the bubble and placeholder are already up, so the
        // runner's Started notice must not add a second pair.
        for c in "yo".chars() {
            a.apply(AppEvent::Term(TermEvent::Key(key(KeyCode::Char(c)))));
        }
        a.apply(AppEvent::Term(TermEvent::Key(key(KeyCode::Enter))));
        let n = a.entries.len();
        started(&mut a, &sid, TurnOrigin::User { text: "yo".into() });
        assert_eq!(a.entries.len(), n);
        assert!(a.busy);
    }
}