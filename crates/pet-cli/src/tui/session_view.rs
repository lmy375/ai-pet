//! Session, agent and transcript facts for the header and the replay
//! (the only part of the state that reads config/session files).

use pet_core::item::ChatItem;
use pet_core::session;

use crate::Mode;
use super::entries::{agent_color, Entry, ToolCall};

impl super::TuiApp {
    /// The active agent's display name. Reads the copy `refresh_header` cached
    /// instead of the disk, so a streaming event never parses settings.
    pub fn agent_name(&self) -> String {
        if self.agent_name_cached.is_empty() {
            "pet".to_string()
        } else {
            self.agent_name_cached.clone()
        }
    }

    /// Refresh the header + status-bar facts (agent, model, persisted context
    /// usage) from disk, and schedule a tool recount. Called at startup and
    /// after every turn / command / mode change.
    pub fn refresh_header(&mut self) {
        // The snapshot is shared by every reader and only re-parsed when
        // config.yaml changes, so this costs no disk read on the common path.
        let settings = pet_core::settings::snapshot()
            .map(|s| (*s).clone())
            .unwrap_or_default();
        let (name, model) = settings
            .active_agent_config()
            .map(|a| (a.name.clone(), a.model.clone()))
            .unwrap_or_default();
        self.agents = settings
            .agents
            .iter()
            .enumerate()
            .map(|(i, a)| (a.id.clone(), a.name.clone(), agent_color(i)))
            .collect();
        let index = session::list_sessions();
        self.session_id = index.active_id.clone();
        let title = index
            .sessions
            .iter()
            .find(|m| m.id == index.active_id)
            .map(|m| m.title.clone())
            .unwrap_or_else(|| "新会话".to_string());
        self.header = match self.mode {
            Mode::Chat => format!(" pet · 会话:{title}"),
            Mode::Group => " pet · 群聊".to_string(),
        };
        self.agent_name_cached = name;
        self.model = model;
        // Last persisted occupancy of the active session (live Usage events
        // overwrite this during a turn).
        if !index.active_id.is_empty() {
            if let Ok(sess) = session::load_session(index.active_id) {
                self.context_usage = sess.context_usage.map(|u| (u.used, u.total));
            }
        }
        self.tools_dirty = true;
    }

    /// Seed the transcript with the tail of the active session so reopening
    /// the CLI shows where the conversation left off.
    pub(super) fn replay_session(&mut self, limit: usize) {
        let index = session::list_sessions();
        if index.active_id.is_empty() {
            return;
        }
        let Ok(sess) = session::load_session(index.active_id) else { return };
        let start = sess.conv.items.len().saturating_sub(limit);
        for item in &sess.conv.items[start..] {
            self.push_item(item);
        }
    }

    /// Render one persisted display item as a transcript entry.
    pub(super) fn push_item(&mut self, item: &ChatItem) {
        match item {
            ChatItem::User { content, .. } => self.push(Entry::User { text: content.clone() }),
            ChatItem::Assistant { content, reasoning, .. } => {
                let reasoning = reasoning.clone().unwrap_or_default();
                // Image-only bubbles carry neither and have nothing to show
                // here; a reasoning-only item (thought, then a tool call with no
                // preamble) still renders its collapsed thinking.
                if content.is_empty() && reasoning.is_empty() {
                    return;
                }
                let name = self.agent_name();
                self.push(Entry::Assistant {
                    name,
                    text: content.clone(),
                    reasoning,
                    streaming: false,
                    reasoning_expanded: false,
                });
            }
            ChatItem::Tool { tool_calls, .. } => {
                let calls = tool_calls
                    .iter()
                    .map(|tc| ToolCall {
                        name: tc.name.clone(),
                        args: tc.arguments.clone(),
                        result: tc.result.clone(),
                    })
                    .collect();
                self.push(Entry::Tool { owner: None, calls, expanded: false, open: false });
            }
            ChatItem::Error { content, .. } => {
                self.push(Entry::Error { text: content.clone() })
            }
            ChatItem::Notification { content, .. } => {
                self.push(Entry::Notice { text: content.clone() })
            }
            ChatItem::Group { .. } => {}
        }
    }
}