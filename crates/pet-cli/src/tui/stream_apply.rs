//! Chat stream events → transcript entries (the CLI analogue of `reduceTurn`).

use pet_core::chat::StreamEvent;

use super::entries::{Entry, ToolCall};

impl super::TuiApp {
    pub(super) fn apply_stream(&mut self, ev: StreamEvent) {
        match ev {
            StreamEvent::Chunk { text } => {
                self.close_open_tool();
                let appended = matches!(
                    self.entries.last(),
                    Some(Entry::Assistant { streaming: true, .. })
                );
                if appended {
                    if let Some(Entry::Assistant { text: t, .. }) = self.entries.last_mut() {
                        t.push_str(&text);
                    }
                } else {
                    let name = self.agent_name();
                    self.push(Entry::Assistant {
                        name,
                        text,
                        reasoning: String::new(),
                        streaming: true,
                        reasoning_expanded: false,
                    });
                }
                self.touch_last();
            }
            StreamEvent::Reasoning { text } => {
                let appended = matches!(
                    self.entries.last(),
                    Some(Entry::Assistant { streaming: true, .. })
                );
                if appended {
                    if let Some(Entry::Assistant { reasoning, .. }) = self.entries.last_mut() {
                        reasoning.push_str(&text);
                    }
                } else {
                    let name = self.agent_name();
                    self.push(Entry::Assistant {
                        name,
                        text: String::new(),
                        reasoning: text,
                        streaming: true,
                        reasoning_expanded: false,
                    });
                }
                self.touch_last();
            }
            StreamEvent::ToolStart { name, arguments } => {
                self.finish_streaming_assistant();
                let call = ToolCall { name, args: arguments, result: None };
                let appendable =
                    matches!(self.entries.last(), Some(Entry::Tool { open: true, .. }));
                if appendable {
                    if let Some(Entry::Tool { calls, .. }) = self.entries.last_mut() {
                        calls.push(call);
                    }
                } else {
                    self.push(Entry::Tool {
                        owner: None,
                        calls: vec![call],
                        expanded: false,
                        open: true,
                    });
                }
                self.touch_last();
            }
            StreamEvent::ToolResult { name, result } => {
                if let Some(Entry::Tool { calls, .. }) =
                    self.entries.iter_mut().rev().find(|e| matches!(e, Entry::Tool { owner: None, .. }))
                {
                    if let Some(c) =
                        calls.iter_mut().find(|c| c.name == name && c.result.is_none())
                    {
                        c.result = Some(result);
                    }
                }
                // The open tool entry is near the end; refreshing the last few
                // is cheap and correct even if text followed.
                let n = self.entries.len();
                for i in n.saturating_sub(3)..n {
                    self.touch(i);
                }
            }
            StreamEvent::Image { data_url } => {
                self.close_open_tool();
                self.push(Entry::Notice { text: format!("[图片 · {} bytes]", data_url.len()) });
            }
            StreamEvent::Usage { total_tokens, context_window, .. } => {
                self.context_usage = Some((total_tokens, context_window as u64));
            }
            StreamEvent::Done {} => {
                self.close_open_tool();
                self.finish_streaming_assistant();
                self.prune_empty_assistant();
            }
            StreamEvent::Error { message } => {
                self.close_open_tool();
                self.finish_streaming_assistant();
                self.prune_empty_assistant();
                self.push(Entry::Error { text: message });
            }
        }
    }

    pub(super) fn close_open_tool(&mut self) {
        if let Some(Entry::Tool { open, .. }) = self.entries.last_mut() {
            *open = false;
        }
    }

    /// Commit the trailing streaming assistant entry (its text is final).
    pub(super) fn finish_streaming_assistant(&mut self) {
        let committed = match self.entries.last_mut() {
            Some(Entry::Assistant { streaming, .. }) if *streaming => {
                *streaming = false;
                true
            }
            _ => false,
        };
        if committed {
            self.touch_last();
        }
    }

    /// Drop a trailing assistant entry that ended with nothing to show (e.g.
    /// the local placeholder when the turn failed before streaming).
    pub(super) fn prune_empty_assistant(&mut self) {
        if matches!(
            self.entries.last(),
            Some(Entry::Assistant { text, reasoning, streaming: false, .. })
                if text.trim().is_empty() && reasoning.is_empty()
        ) {
            self.entries.pop();
            self.line_cache.pop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::entries::Entry;
    use super::super::TuiApp;
    use crate::event::AppEvent;
    use pet_core::chat::StreamEvent;
    use pet_core::turn::TurnNotice;
    use ratatui::crossterm::event::{Event as TermEvent, KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn app() -> TuiApp {
        let mut a = TuiApp::new();
        a.entries.clear();
        a.line_cache.clear();
        a
    }

    /// A stream event of the viewed session's turn, as the runner would send it.
    fn stream(a: &mut TuiApp, event: StreamEvent) {
        let session_id = a.session_id.clone();
        a.apply(AppEvent::Turn(TurnNotice::Stream {
            session_id,
            turn_id: "t".into(),
            seq: 0,
            event,
        }));
    }

    #[test]
    fn stream_builds_assistant_then_tool_then_selection_toggles_it() {
        let mut a = app();
        stream(&mut a, StreamEvent::Chunk { text: "让我看看".into() });
        stream(&mut a, StreamEvent::ToolStart {
            name: "bash".into(),
            arguments: "{}".into(),
        });
        stream(&mut a, StreamEvent::ToolResult {
            name: "bash".into(),
            result: "ok".into(),
        });
        stream(&mut a, StreamEvent::Done {});
        assert_eq!(a.entries.len(), 2);
        assert!(matches!(a.entries[1], Entry::Tool { expanded: false, .. }));

        // Empty input: ↑ selects the tool entry, Enter expands it.
        a.apply(AppEvent::Term(TermEvent::Key(key(KeyCode::Up))));
        assert_eq!(a.sel_entry, Some(1));
        a.apply(AppEvent::Term(TermEvent::Key(key(KeyCode::Enter))));
        assert!(matches!(a.entries[1], Entry::Tool { expanded: true, .. }));
    }
}