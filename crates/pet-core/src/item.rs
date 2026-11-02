//! The display-transcript protocol.
//!
//! `ChatItem` is the single type for what interfaces render and sessions store.
//! It is serialized with `type` as the tag and camelCase fields, which is the
//! shape the frontend (`ChatItem` in `useChat.ts`) and the CLI read. Every
//! producer — the chat pipeline's [`ItemBuilder`], the session constructors,
//! the group orchestrator — builds this type, so a renamed field is a compile
//! error instead of a silently dropped bubble.

use serde::{Deserialize, Serialize};

/// One tool call inside a `tool` item. `isRunning` is deliberately absent: a
/// persisted call has always finished (the live "running" flag is a frontend
/// display concern), and storing a constant `false` only invited readers to
/// treat it as meaningful.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallRecord {
    pub name: String,
    pub arguments: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ChatItem {
    User {
        id: String,
        ts: i64,
        content: String,
        #[serde(default)]
        images: Vec<String>,
    },
    Assistant {
        id: String,
        ts: i64,
        content: String,
        /// Chain-of-thought from a reasoning model. Display-only — never sent
        /// back to the model.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reasoning: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        images: Vec<String>,
    },
    Tool {
        id: String,
        ts: i64,
        /// Always empty; kept so every bubble has a `content` field to render.
        content: String,
        tool_calls: Vec<ToolCallRecord>,
    },
    Error {
        id: String,
        ts: i64,
        content: String,
    },
    /// The line that opens a background-task resumption turn.
    Notification {
        id: String,
        ts: i64,
        content: String,
        label: String,
        detail: String,
    },
    /// A group line injected into an agent's private context.
    Group {
        id: String,
        ts: i64,
        content: String,
        #[serde(default)]
        sender_kind: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sender_id: Option<String>,
        #[serde(default)]
        sender_name: String,
    },
}

/// A stable per-item id. Every producer stamps one at creation: display items
/// are addressed by id (React keys, multi-select, `prune_session`), so an item
/// that reaches disk without one can't be referred to at all.
pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

impl ChatItem {
    pub fn user(content: impl Into<String>, images: &[String]) -> Self {
        Self::User {
            id: new_id(),
            ts: crate::common::now_ms(),
            content: content.into(),
            images: images.to_vec(),
        }
    }

    pub fn assistant(
        content: impl Into<String>,
        reasoning: Option<String>,
        images: &[String],
    ) -> Self {
        Self::Assistant {
            id: new_id(),
            ts: crate::common::now_ms(),
            content: content.into(),
            reasoning: reasoning.filter(|r| !r.is_empty()),
            images: images.to_vec(),
        }
    }

    pub fn tool(tool_calls: Vec<ToolCallRecord>) -> Self {
        Self::Tool {
            id: new_id(),
            ts: crate::common::now_ms(),
            content: String::new(),
            tool_calls,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self::Error {
            id: new_id(),
            ts: crate::common::now_ms(),
            content: message.into(),
        }
    }

    pub fn notification(label: &str, detail: &str) -> Self {
        Self::Notification {
            id: new_id(),
            ts: crate::common::now_ms(),
            content: format!("后台任务完成：{label}"),
            label: label.to_string(),
            detail: detail.to_string(),
        }
    }

    pub fn group(
        content: impl Into<String>,
        sender_kind: impl Into<String>,
        sender_id: Option<String>,
        sender_name: impl Into<String>,
        ts: i64,
    ) -> Self {
        Self::Group {
            id: new_id(),
            ts,
            content: content.into(),
            sender_kind: sender_kind.into(),
            sender_id,
            sender_name: sender_name.into(),
        }
    }

    pub fn id(&self) -> &str {
        match self {
            Self::User { id, .. }
            | Self::Assistant { id, .. }
            | Self::Tool { id, .. }
            | Self::Error { id, .. }
            | Self::Notification { id, .. }
            | Self::Group { id, .. } => id,
        }
    }

    /// True for items that open a conversation turn (`user` / `notification`).
    pub fn opens_turn(&self) -> bool {
        matches!(self, Self::User { .. } | Self::Notification { .. })
    }

    /// The assistant's text, when this item carries a model-authored message.
    pub fn assistant_text(&self) -> Option<&str> {
        match self {
            Self::Assistant { content, .. } if !content.trim().is_empty() => Some(content),
            _ => None,
        }
    }
}

/// What a running agent has produced so far: committed items plus the text,
/// thinking and tool calls still streaming. Built by [`ItemBuilder::snapshot`],
/// so a late reader never has to re-fold the event stream.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemTail {
    pub items: Vec<ChatItem>,
    pub response: String,
    pub reasoning: String,
    pub tool_calls: Vec<ToolCallRecord>,
}

/// Folds an agent run's stream events into display items. This is the single
/// authority for that fold: interfaces render events live, but what gets
/// persisted (and what a late joiner receives through a snapshot) comes from
/// here.
#[derive(Default)]
pub struct ItemBuilder {
    /// Accumulated assistant text not yet committed.
    accumulated: String,
    /// Accumulated chain-of-thought for the current assistant item. Display-only.
    reasoning: String,
    /// Tool calls in the current (not-yet-flushed) group.
    tool_calls: Vec<ToolCallRecord>,
    /// Committed display items.
    items: Vec<ChatItem>,
}

impl ItemBuilder {
    fn flush_tool_calls(&mut self) {
        if self.tool_calls.is_empty() {
            return;
        }
        let calls = std::mem::take(&mut self.tool_calls);
        self.items.push(ChatItem::tool(calls));
    }

    fn commit_text(&mut self) {
        let text = std::mem::take(&mut self.accumulated);
        let reasoning = std::mem::take(&mut self.reasoning);
        // A round that produced only thinking still becomes an item: the model
        // thought and then called a tool with no preamble. Both fields were
        // already taken above, so bailing out on empty text alone would drop
        // the thought for good.
        if text.trim().is_empty() && reasoning.trim().is_empty() {
            return;
        }
        self.items
            .push(ChatItem::assistant(text, Some(reasoning).filter(|r| !r.is_empty()), &[]));
    }

    pub fn chunk(&mut self, text: &str) {
        self.flush_tool_calls();
        self.accumulated.push_str(text);
    }

    pub fn reasoning(&mut self, text: &str) {
        // Thinking for a new round starts only after the previous round's tool
        // calls returned, so flush their group here. Otherwise a reasoning-only
        // run would collect every round's calls and commit them as one item
        // after all the thinking, at `done`.
        self.flush_tool_calls();
        self.reasoning.push_str(text);
    }

    pub fn tool_start(&mut self, name: &str, arguments: &str) {
        // Commit any assistant text streamed before this tool call.
        self.commit_text();
        self.tool_calls.push(ToolCallRecord {
            name: name.to_string(),
            arguments: arguments.to_string(),
            result: None,
        });
    }

    pub fn tool_result(&mut self, name: &str, result: &str) {
        // Attach to the first tool call of that name still missing a result.
        if let Some(tc) = self
            .tool_calls
            .iter_mut()
            .find(|tc| tc.name == name && tc.result.is_none())
        {
            tc.result = Some(result.to_string());
        }
    }

    pub fn image(&mut self, data_url: &str) {
        self.flush_tool_calls();
        self.items.push(ChatItem::assistant("", None, &[data_url.to_string()]));
    }

    pub fn done(&mut self) {
        self.flush_tool_calls();
        self.commit_text();
    }

    pub fn error(&mut self, message: &str) {
        self.flush_tool_calls();
        self.items.push(ChatItem::error(message));
    }

    /// What the run has produced so far, without consuming it.
    pub fn snapshot(&self) -> ItemTail {
        ItemTail {
            items: self.items.clone(),
            response: self.accumulated.clone(),
            reasoning: self.reasoning.clone(),
            tool_calls: self.tool_calls.clone(),
        }
    }

    /// Take the accumulated display items (after the run finishes).
    pub fn take_items(&mut self) -> Vec<ChatItem> {
        std::mem::take(&mut self.items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value(item: ChatItem) -> serde_json::Value {
        serde_json::to_value(item).unwrap()
    }

    #[test]
    fn wire_shape_matches_the_frontend_chat_item() {
        let user = ChatItem::User {
            id: "i1".into(),
            ts: 1,
            content: "hi".into(),
            images: vec!["data:x".into()],
        };
        assert_eq!(
            value(user),
            serde_json::json!({
                "id": "i1", "ts": 1, "type": "user", "content": "hi", "images": ["data:x"]
            })
        );

        let assistant = ChatItem::Assistant {
            id: "i2".into(),
            ts: 2,
            content: "hello".into(),
            reasoning: Some("thinking".into()),
            images: vec![],
        };
        assert_eq!(
            value(assistant),
            serde_json::json!({
                "id": "i2", "ts": 2, "type": "assistant", "content": "hello",
                "reasoning": "thinking"
            })
        );

        let tool = ChatItem::Tool {
            id: "i3".into(),
            ts: 3,
            content: String::new(),
            tool_calls: vec![ToolCallRecord {
                name: "bash".into(),
                arguments: "{}".into(),
                result: Some("ok".into()),
            }],
        };
        assert_eq!(
            value(tool),
            serde_json::json!({
                "id": "i3", "ts": 3, "type": "tool", "content": "",
                "toolCalls": [{ "name": "bash", "arguments": "{}", "result": "ok" }]
            })
        );

        let group = ChatItem::Group {
            id: "i4".into(),
            ts: 4,
            content: "line".into(),
            sender_kind: "agent".into(),
            sender_id: Some("a1".into()),
            sender_name: "小宠".into(),
        };
        assert_eq!(
            value(group),
            serde_json::json!({
                "id": "i4", "ts": 4, "type": "group", "content": "line",
                "senderKind": "agent", "senderId": "a1", "senderName": "小宠"
            })
        );
    }

    /// Old files carry the constant `isRunning: false`; it is ignored on read
    /// and never written again.
    #[test]
    fn stored_tool_calls_deserialize_without_the_retired_running_flag() {
        let v = serde_json::json!({
            "id": "i1", "ts": 1, "type": "tool", "content": "",
            "toolCalls": [{ "name": "bash", "arguments": "{}", "isRunning": false, "result": "ok" }]
        });
        let item: ChatItem = serde_json::from_value(v).unwrap();
        let ChatItem::Tool { tool_calls, .. } = &item else { panic!() };
        assert_eq!(tool_calls[0].result.as_deref(), Some("ok"));
        assert!(serde_json::to_value(&item).unwrap()["toolCalls"][0]
            .get("isRunning")
            .is_none());
    }

    /// The model thinks, then calls a tool without writing a preamble. That
    /// thought has to reach the session file instead of being dropped by the
    /// empty-text early return.
    #[test]
    fn thinking_before_a_tool_call_survives_into_the_transcript() {
        let mut b = ItemBuilder::default();
        b.reasoning("weighing the options");
        b.tool_start("bash", "{}");
        b.tool_result("bash", "ok");
        b.chunk("done");
        b.done();

        let items = b.take_items();
        assert_eq!(items.len(), 3);
        assert!(matches!(
            &items[0],
            ChatItem::Assistant { content, reasoning: Some(r), .. }
                if content.is_empty() && r == "weighing the options"
        ));
        assert!(matches!(&items[1], ChatItem::Tool { tool_calls, .. } if tool_calls[0].result.as_deref() == Some("ok")));
        assert!(matches!(&items[2], ChatItem::Assistant { content, reasoning: None, .. } if content == "done"));
    }

    /// Each round's thinking must stay next to the tool calls it produced.
    #[test]
    fn thinking_stays_adjacent_to_its_own_tool_round() {
        let mut b = ItemBuilder::default();
        b.reasoning("round one");
        b.tool_start("bash", "{}");
        b.tool_result("bash", "ok");
        b.reasoning("round two");
        b.tool_start("bash", "{}");
        b.tool_result("bash", "ok");
        b.chunk("answer");
        b.done();

        let items = b.take_items();
        let kinds: Vec<&str> = items
            .iter()
            .map(|i| match i {
                ChatItem::Assistant { .. } => "assistant",
                ChatItem::Tool { .. } => "tool",
                _ => "other",
            })
            .collect();
        assert_eq!(kinds, ["assistant", "tool", "assistant", "tool", "assistant"]);
        assert!(matches!(&items[0], ChatItem::Assistant { reasoning: Some(r), .. } if r == "round one"));
        assert!(matches!(&items[2], ChatItem::Assistant { reasoning: Some(r), .. } if r == "round two"));
        assert!(matches!(&items[4], ChatItem::Assistant { content, .. } if content == "answer"));
    }

    #[test]
    fn an_empty_round_commits_no_item() {
        let mut b = ItemBuilder::default();
        b.done();
        assert!(b.take_items().is_empty());
    }

    #[test]
    fn snapshot_holds_committed_items_and_the_uncommitted_tail() {
        let mut b = ItemBuilder::default();
        b.tool_start("bash", "{}");
        b.tool_result("bash", "ok");
        b.chunk("partial");
        b.reasoning("hm");

        let tail = b.snapshot();
        // The tool group was committed when text started streaming; the text
        // and thinking that followed are still uncommitted.
        assert_eq!(tail.items.len(), 1);
        assert!(matches!(&tail.items[0], ChatItem::Tool { tool_calls, .. } if tool_calls[0].result.as_deref() == Some("ok")));
        assert_eq!(tail.response, "partial");
        assert_eq!(tail.reasoning, "hm");
        assert!(tail.tool_calls.is_empty());
    }
}