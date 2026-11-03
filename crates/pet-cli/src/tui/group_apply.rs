//! Group-room stream events → transcript entries and the cached roster lookup.

use pet_core::chat::StreamEvent;
use ratatui::style::Color;

use crate::Mode;
use super::entries::{Entry, ToolCall};

impl super::TuiApp {
    /// The cached roster from `refresh_header`; no disk read per group event.
    pub(super) fn group_roster(&self, agent_id: &str) -> (String, Color) {
        self.agents
            .iter()
            .find(|(id, ..)| id == agent_id)
            .map(|(_, name, color)| (name.clone(), *color))
            .unwrap_or_else(|| (agent_id.to_string(), Color::Reset))
    }

    pub(super) fn apply_group_stream(&mut self, agent_id: String, ev: StreamEvent) {
        if self.mode != Mode::Group {
            return; // muted outside the room (workers keep running)
        }
        match ev {
            StreamEvent::ToolStart { name, arguments } => {
                let call = ToolCall { name, args: arguments, result: None };
                let existing = self
                    .group_tools
                    .get(&agent_id)
                    .copied()
                    .filter(|&idx| matches!(self.entries.get(idx), Some(Entry::Tool { .. })));
                match existing {
                    Some(idx) => {
                        if let Some(Entry::Tool { calls, .. }) = self.entries.get_mut(idx) {
                            calls.push(call);
                        }
                        self.touch(idx);
                    }
                    None => {
                        let owner = self.group_roster(&agent_id);
                        self.push(Entry::Tool {
                            owner: Some(owner),
                            calls: vec![call],
                            expanded: false,
                            open: true,
                        });
                        self.group_tools.insert(agent_id, self.entries.len() - 1);
                    }
                }
            }
            StreamEvent::ToolResult { name, result } => {
                if let Some(&idx) = self.group_tools.get(&agent_id) {
                    if let Some(Entry::Tool { calls, .. }) = self.entries.get_mut(idx) {
                        if let Some(c) =
                            calls.iter_mut().find(|c| c.name == name && c.result.is_none())
                        {
                            c.result = Some(result);
                        }
                    }
                    self.touch(idx);
                }
            }
            StreamEvent::Error { message } => {
                let (name, _) = self.group_roster(&agent_id);
                self.push(Entry::Error { text: format!("{name}: {message}") });
            }
            _ => {}
        }
    }
}