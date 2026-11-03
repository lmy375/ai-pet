use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;

use crate::item::{ChatItem, MessageRange};

/// The file holding one session record (`index.json`, `<id>.json`).
fn file(name: &str) -> Result<PathBuf, String> {
    Ok(crate::store::data_dir("sessions")?.join(name))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionIndex {
    pub active_id: String,
    pub sessions: Vec<SessionMeta>,
}

/// Last reported context-window occupancy for a session, persisted so the chat
/// usage ring shows immediately on reload/switch instead of waiting for the next
/// turn. `#[serde(default)]` keeps older session files (without it) parseable.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextUsage {
    pub used: u64,
    pub total: u64,
}

/// The two parallel tracks an agent conversation keeps: the model-facing
/// `messages` and the display-facing `items`. Owners flatten it into their own
/// struct so both keys stay at the top level of the disk JSON.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Conversation {
    #[serde(default)]
    pub messages: Vec<serde_json::Value>,
    #[serde(default)]
    pub items: Vec<ChatItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    #[serde(flatten)]
    pub conv: Conversation,
    #[serde(default)]
    pub context_usage: Option<ContextUsage>,
}

fn read_index() -> Result<SessionIndex, String> {
    let path = file("index.json")?;
    Ok(crate::store::read_json(&path)?.unwrap_or_default())
}

fn write_index(index: &SessionIndex) -> Result<(), String> {
    crate::store::write_json(&file("index.json")?, index)
}

/// Sentinel title of a not-yet-named session, stored verbatim; interfaces
/// translate it at display time.
pub const DEFAULT_SESSION_TITLE: &str = "新会话";

/// True if `result` is the inline `{task_id, status: "running"}` JSON a tool
/// returns when its work went to the background as `task_id`.
fn result_is_for_task(result: &Option<String>, task_id: &str) -> bool {
    result
        .as_deref()
        .and_then(|r| serde_json::from_str::<serde_json::Value>(r).ok())
        .is_some_and(|v| v["task_id"] == task_id)
}

/// Replace the "running in background" placeholder result of the tool call that
/// launched `task_id` with the task's final result, so the rendered tool block
/// flips from running to finished when the completion turn is injected.
pub fn apply_completion_to_items(
    mut items: Vec<ChatItem>,
    task_id: &str,
    result: &str,
) -> Vec<ChatItem> {
    for item in items.iter_mut() {
        let ChatItem::Tool { tool_calls, .. } = item else {
            continue;
        };
        for tc in tool_calls.iter_mut() {
            if result_is_for_task(&tc.result, task_id) {
                tc.result = Some(result.to_string());
            }
        }
    }
    items
}

/// Derive a session title from its display items: the first non-empty `user`
/// item's text, truncated to 20 Unicode scalar values with an ellipsis. Returns
/// `None` when there's no usable user text, so callers pick their own fallback.
pub fn derive_title(items: &[ChatItem]) -> Option<String> {
    items
        .iter()
        .find_map(|i| match i {
            ChatItem::User { content, .. } if !content.is_empty() => Some(content),
            _ => None,
        })
        .map(|c| {
            let t: String = c.chars().take(20).collect();
            if c.chars().count() > 20 {
                format!("{}...", t)
            } else {
                t
            }
        })
}

/// Where a message appended now will sit once the conversation is stored.
/// Leading system seeds are rebuilt on every run and stripped before saving,
/// so item ranges count from after them.
pub(crate) fn stored_message_count(messages: &[serde_json::Value]) -> usize {
    messages
        .iter()
        .skip_while(|m| m["role"] == "System")
        .count()
}

/// Recompute each turn-opening item's span. An opener owns its whole block, so
/// its range runs from its own first message to the next opener's first message
/// (or the end of the conversation). Producers write every other item's range
/// when they create it; only this block span is not known until the turn ends.
fn refresh_opener_ranges(items: &mut [ChatItem], message_count: usize) {
    let starts: Vec<(usize, usize)> = items
        .iter()
        .enumerate()
        .filter(|(_, it)| it.opens_turn())
        .filter_map(|(i, it)| it.messages_range().map(|r| (i, r.start)))
        .collect();
    for (k, &(idx, start)) in starts.iter().enumerate() {
        let end = starts.get(k + 1).map(|&(_, s)| s).unwrap_or(message_count);
        items[idx].set_messages_range(Some(MessageRange { start, len: end.saturating_sub(start) }));
    }
}

fn item_blocks(items: &[ChatItem]) -> Vec<Vec<usize>> {
    let mut blocks: Vec<Vec<usize>> = Vec::new();
    for (i, item) in items.iter().enumerate() {
        if item.opens_turn() {
            blocks.push(vec![i]);
        } else if let Some(block) = blocks.last_mut() {
            block.push(i);
        }
    }
    blocks
}

/// Remove selected display items and the model messages they point at.
///
/// Each item carries an explicit [`MessageRange`] into `messages`, so pruning
/// is index arithmetic: the kept items are everything unselected, and the
/// messages to drop are the selected items' ranges. An item without a range
/// (an error line) disappears from the transcript alone.
///
/// A tool row's preamble bubbles go with it: they are the same round's display
/// fragments and would otherwise outlive the message they introduced.
///
/// If any range runs past the end of `messages` (a corrupt file), the messages
/// are returned untouched — a stale context beats one with holes.
pub fn prune_session(
    items: &[ChatItem],
    messages: &[serde_json::Value],
    selected: &HashSet<String>,
) -> (Vec<ChatItem>, Vec<serde_json::Value>) {
    let mut removed = selected.clone();
    for block in item_blocks(items) {
        // Removing the opener removes the whole turn from the transcript too.
        if selected.contains(items[block[0]].id()) {
            removed.extend(block.iter().map(|&i| items[i].id().to_string()));
            continue;
        }
        for (k, &idx) in block.iter().enumerate() {
            if !matches!(items[idx], ChatItem::Tool { .. }) || !selected.contains(items[idx].id()) {
                continue;
            }
            for &prev in block[..k].iter().rev() {
                if !matches!(items[prev], ChatItem::Assistant { .. }) {
                    break;
                }
                removed.insert(items[prev].id().to_string());
            }
        }
    }

    let kept: Vec<ChatItem> = items
        .iter()
        .filter(|it| !removed.contains(it.id()))
        .cloned()
        .collect();

    let mut drop: HashSet<usize> = HashSet::new();
    for item in items.iter().filter(|it| selected.contains(it.id())) {
        let Some(range) = item.messages_range() else {
            continue;
        };
        if range.start.saturating_add(range.len) > messages.len() {
            return (kept, messages.to_vec());
        }
        drop.extend(range.start..range.start + range.len);
    }

    // Shift the surviving ranges by the messages dropped before them. An
    // opener's length is recomputed on save; a leaf whose own span was cut
    // loses its link rather than pointing at the wrong messages.
    let kept_items = kept
        .into_iter()
        .map(|mut item| {
            if let Some(range) = item.messages_range() {
                let removed_before = (0..range.start).filter(|j| drop.contains(j)).count();
                let start = range.start - removed_before;
                let intact = (range.start..range.start + range.len).all(|j| !drop.contains(&j));
                let range = if item.opens_turn() {
                    Some(MessageRange { start, len: 0 })
                } else if intact {
                    Some(MessageRange { start, len: range.len })
                } else {
                    None
                };
                item.set_messages_range(range);
            }
            item
        })
        .collect();

    let kept_messages = messages
        .iter()
        .enumerate()
        .filter(|(j, _)| !drop.contains(j))
        .map(|(_, m)| m.clone())
        .collect();
    (kept_items, kept_messages)
}

/// Prune selected items from a session and persist the result, refreshing
/// `updated_at`. Returns the session as interfaces render it.
///
/// The caller must ensure no turn is running in the session: a finishing turn
/// appends to both arrays and would race this edit.
pub fn prune_and_save(session_id: &str, item_ids: &[String]) -> Result<SessionView, String> {
    let mut sess = load_session(session_id.to_string())?;
    let selected: HashSet<String> = item_ids.iter().cloned().collect();
    let (items, messages) = prune_session(&sess.conv.items, &sess.conv.messages, &selected);
    sess.conv.items = items;
    sess.conv.messages = messages;
    sess.updated_at = crate::common::iso_now();
    save_session(sess.clone())?;
    Ok(SessionView::from(sess))
}

/// A `{ "role": "system", "content": <SOUL.md> }` message — the first message
/// of every new session, seeded with the given agent's SOUL. The seed is only
/// the initial system message; every chat turn rebuilds it via
/// `SystemPrompt::Chat`, so it's effectively cosmetic.
pub fn soul_system_message(agent_id: &str) -> serde_json::Value {
    let soul = super::memory::read_soul(agent_id);
    crate::llm::store_message(&genai::chat::ChatMessage::system(soul))
}

/// Build (and persist) a fresh session with the given id and title, seeded with
/// the agent's SOUL system message. Used by `create_session` and the Telegram bot.
pub fn new_seeded_session(agent_id: &str, id: String, title: String) -> Result<Session, String> {
    let now = crate::common::iso_now();
    let session = Session {
        id,
        title,
        created_at: now.clone(),
        updated_at: now,
        conv: Conversation {
            messages: vec![soul_system_message(agent_id)],
            items: vec![],
        },
        context_usage: None,
    };
    save_session(session.clone())?;
    Ok(session)
}

pub fn list_sessions() -> SessionIndex {
    // Interfaces call this on every refresh and can't surface an error, so a
    // damaged index is logged and read as empty. It is never written from here,
    // leaving the file intact for recovery.
    match read_index() {
        Ok(index) => index,
        Err(e) => {
            eprintln!("{e}");
            SessionIndex::default()
        }
    }
}

/// Mark `id` as the active session in the shared index, without rewriting the
/// session file. `active_id` is the only cross-window pointer to "which session
/// is current" (disk is the sole shared state), and otherwise it's only updated
/// as a side effect of `save_session`. Switching sessions in one window must
/// persist the choice here so the other window's focus-reload converges on it
/// instead of reverting to whatever was last saved (the newest session).
pub fn set_active_session(id: String) -> Result<(), String> {
    let mut index = read_index()?;
    if !index.sessions.iter().any(|m| m.id == id) {
        return Err(format!("Unknown session {id}"));
    }
    index.active_id = id;
    write_index(&index)
}

pub fn load_session(id: String) -> Result<Session, String> {
    let path = file(&format!("{id}.json"))?;
    crate::store::read_json(&path)?.ok_or_else(|| format!("Unknown session {id}"))
}

/// A session as an interface renders it: the display transcript and its
/// metadata, WITHOUT the model-facing `messages`. Interfaces never build or
/// inspect those (the backend owns the LLM conversation), so they aren't sent
/// across — they can carry megabytes of inline images.
#[derive(Debug, Clone, Serialize)]
pub struct SessionView {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub items: Vec<ChatItem>,
    pub context_usage: Option<ContextUsage>,
}

impl From<Session> for SessionView {
    fn from(s: Session) -> Self {
        Self {
            id: s.id,
            title: s.title,
            created_at: s.created_at,
            updated_at: s.updated_at,
            items: s.conv.items,
            context_usage: s.context_usage,
        }
    }
}

pub fn load_session_view(id: String) -> Result<SessionView, String> {
    load_session(id).map(SessionView::from)
}

/// Write a session file and refresh its index row. Does NOT touch
/// `active_id`: turns run in the background for any session (a background-task
/// completion resumes the session that spawned it), and persisting one must
/// never yank the windows over to it. Only `set_active_session` /
/// `create_session` — user actions — move the pointer.
pub fn save_session(mut session: Session) -> Result<(), String> {
    refresh_opener_ranges(
        &mut session.conv.items,
        stored_message_count(&session.conv.messages),
    );

    let path = file(&format!("{}.json", session.id))?;

    // Preserve created_at (and last-known usage) from the existing file when the
    // caller didn't supply them — e.g. a turn whose provider omitted usage, or a
    // Telegram save, shouldn't blank out a previously-recorded occupancy. A
    // damaged old file is reported but never blocks the save: the in-memory
    // session is newer and is about to replace it.
    if session.created_at.is_empty() || session.context_usage.is_none() {
        match crate::store::read_json::<Session>(&path) {
            Ok(Some(existing)) => {
                if session.created_at.is_empty() {
                    session.created_at = existing.created_at;
                }
                if session.context_usage.is_none() {
                    session.context_usage = existing.context_usage;
                }
            }
            Ok(None) => {}
            Err(e) => eprintln!("{e}"),
        }
    }

    // Write session file
    crate::store::write_json(&path, &session)?;

    // Update index
    let mut index = read_index()?;
    if let Some(meta) = index.sessions.iter_mut().find(|m| m.id == session.id) {
        meta.title = session.title.clone();
        meta.updated_at = session.updated_at.clone();
    } else {
        index.sessions.push(SessionMeta {
            id: session.id.clone(),
            title: session.title.clone(),
            created_at: session.created_at.clone(),
            updated_at: session.updated_at.clone(),
        });
    }
    write_index(&index)
}

/// Rename a session: update both the session file's title and the index meta,
/// without touching its messages/items (so it can't race the in-memory chat).
pub fn rename_session(id: String, title: String) -> Result<(), String> {
    let title = title.trim().to_string();
    if title.is_empty() {
        return Err("Title cannot be empty".to_string());
    }

    let path = file(&format!("{id}.json"))?;
    if let Some(mut session) = crate::store::read_json::<Session>(&path)? {
        session.title = title.clone();
        crate::store::write_json(&path, &session)?;
    }

    let mut index = read_index()?;
    if let Some(meta) = index.sessions.iter_mut().find(|m| m.id == id) {
        meta.title = title;
        write_index(&index)?;
    }
    Ok(())
}

/// Create a fresh session for the active agent and make it the active one.
pub fn create_session() -> Result<Session, String> {
    let agent_id = crate::settings::active_agent_id();
    let session =
        new_seeded_session(&agent_id, Uuid::new_v4().to_string(), DEFAULT_SESSION_TITLE.to_string())?;
    set_active_session(session.id.clone())?;
    Ok(session)
}

/// Return the tail of `messages` covering the last `n` conversation turns,
/// where a turn starts at a `user` message and runs up to the next one. The
/// slice always begins at a `user` boundary, so it never starts with an orphan
/// `tool` message (which a chat-completions API rejects — a `tool` message must
/// follow the assistant message that requested it). Any leading system messages
/// are dropped as a side effect, which is fine: the heartbeat re-inserts its own
/// system messages via `SystemPrompt::Heartbeat`.
pub fn recent_turns(messages: &[serde_json::Value], n: usize) -> Vec<serde_json::Value> {
    if n == 0 {
        return vec![];
    }
    let user_indices: Vec<usize> = messages
        .iter()
        .enumerate()
        .filter(|(_, m)| crate::llm::is_user_message(m))
        .map(|(i, _)| i)
        .collect();
    let start = match user_indices.len() {
        0 => return vec![],
        len if len <= n => user_indices[0],
        len => user_indices[len - n],
    };
    messages[start..].to_vec()
}

/// Keep only the last `max_turns` conversation turns, starting at a `user`
/// message so a tool response is never orphaned. Every run that caps its
/// context uses this, so the rule can't drift between interfaces.
pub fn trim_for_context(messages: &mut Vec<genai::chat::ChatMessage>, max_turns: usize) {
    if max_turns == 0 {
        messages.clear();
        return;
    }
    let user_indices: Vec<usize> = messages
        .iter()
        .enumerate()
        .filter(|(_, m)| m.role == genai::chat::ChatRole::User)
        .map(|(i, _)| i)
        .collect();
    let start = match user_indices.len() {
        0 => return,
        len if len <= max_turns => user_indices[0],
        len => user_indices[len - max_turns],
    };
    messages.drain(..start);
}

pub fn delete_session(id: String) -> Result<(), String> {
    // Remove file
    let path = file(&format!("{id}.json"))?;
    match fs::remove_file(&path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("Failed to delete session file: {e}")),
    }

    // Update index
    let mut index = read_index()?;
    index.sessions.retain(|m| m.id != id);
    if index.active_id == id {
        index.active_id = index
            .sessions
            .last()
            .map(|m| m.id.clone())
            .unwrap_or_default();
    }
    write_index(&index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use genai::chat::ChatMessage;

    fn user(text: &str) -> serde_json::Value {
        crate::llm::store_message(&ChatMessage::user(text))
    }
    fn assistant(text: &str) -> serde_json::Value {
        crate::llm::store_message(&ChatMessage::assistant(text))
    }
    fn system(text: &str) -> serde_json::Value {
        crate::llm::store_message(&ChatMessage::system(text))
    }
    fn tool_result(call_id: &str, out: &str) -> serde_json::Value {
        crate::llm::store_message(&ChatMessage::tool(
            genai::chat::MessageContent::from_tool_responses(vec![genai::chat::ToolResponse::new(
                call_id, out,
            )]),
        ))
    }

    fn roles(msgs: &[serde_json::Value]) -> Vec<String> {
        msgs.iter()
            .map(|m| m["role"].as_str().unwrap_or("").to_string())
            .collect()
    }

    #[test]
    fn recent_turns_starts_at_user_boundary_never_orphan_tool() {
        // A system seed, one tool-using turn, then a plain turn.
        let msgs = vec![
            system("soul"),
            user("q1"),
            assistant(""),
            tool_result("c1", "result"),
            user("q2"),
            assistant("a2"),
        ];

        // One turn back starts at the LAST user message, not mid-tool-round.
        assert_eq!(roles(&recent_turns(&msgs, 1)), vec!["User", "Assistant"]);

        // Two turns back reaches the earlier user message and keeps its tool
        // round intact — a `Tool` message without its originating call is
        // rejected by every provider.
        assert_eq!(
            roles(&recent_turns(&msgs, 2)),
            vec!["User", "Assistant", "Tool", "User", "Assistant"]
        );

        // More turns than exist yields everything from the first user message,
        // dropping the system seed (which is rebuilt per turn anyway).
        assert_eq!(recent_turns(&msgs, 99).len(), 5);
    }

    #[test]
    fn disk_format_session_json_keeps_messages_and_items_at_top_level() {
        // A session file exactly as the previous struct wrote it: `messages`
        // and `items` sit next to the metadata, not under a `conv` object.
        let disk = r#"{
            "id": "s-1",
            "title": "hello",
            "created_at": "2026-10-19T14:31:56.452",
            "updated_at": "2026-10-26T17:36:48.761",
            "messages": [
                { "content": [{ "Text": "hello" }], "options": null, "role": "User" }
            ],
            "items": [
                { "id": "i1", "ts": 1, "type": "user", "content": "hello", "images": [],
                  "messagesRange": { "start": 0, "len": 1 } }
            ],
            "context_usage": { "used": 12, "total": 200000 }
        }"#;

        let s: Session = serde_json::from_str(disk).expect("old session files must parse");
        assert_eq!(s.conv.messages.len(), 1);
        assert_eq!(s.conv.items.len(), 1);
        assert_eq!(s.conv.items[0].id(), "i1");
        assert_eq!(s.conv.items[0].messages_range(), range(0, 1));
        assert_eq!(s.context_usage.as_ref().map(|u| u.used), Some(12));

        // Serializing back must not nest them under `conv`.
        let written = serde_json::to_value(&s).unwrap();
        assert_eq!(written["messages"][0]["role"], "User");
        assert_eq!(written["items"][0]["type"], "user");
        assert!(written.get("conv").is_none());
    }

    #[test]
    fn recent_turns_edge_cases() {
        assert!(recent_turns(&[], 3).is_empty());
        // Zero turns means no history, regardless of what's there.
        assert!(recent_turns(&[user("q")], 0).is_empty());
        // No user message → nothing to anchor a turn on.
        assert!(recent_turns(&[system("x")], 3).is_empty());
    }

    // --- message ranges ----------------------------------------------------

    fn user_item(id: &str, content: &str, range: Option<MessageRange>) -> ChatItem {
        ChatItem::User {
            id: id.into(),
            ts: 0,
            content: content.into(),
            images: vec![],
            messages_range: range.into(),
        }
    }

    fn assistant_item(id: &str, content: &str, range: Option<MessageRange>) -> ChatItem {
        ChatItem::Assistant {
            id: id.into(),
            ts: 0,
            content: content.into(),
            reasoning: None,
            images: vec![],
            messages_range: range.into(),
        }
    }

    fn tool_item(
        id: &str,
        tool_calls: Vec<crate::item::ToolCallRecord>,
        range: Option<MessageRange>,
    ) -> ChatItem {
        ChatItem::Tool {
            id: id.into(),
            ts: 0,
            content: String::new(),
            tool_calls,
            messages_range: range.into(),
        }
    }

    fn group_item(id: &str, content: &str, range: Option<MessageRange>) -> ChatItem {
        ChatItem::Group {
            id: id.into(),
            ts: 0,
            content: content.into(),
            sender_kind: "agent".into(),
            sender_id: Some("a1".into()),
            sender_name: "小宠".into(),
            messages_range: range.into(),
        }
    }

    fn msg_user(t: &str) -> serde_json::Value {
        crate::llm::store_message(&ChatMessage::user(t))
    }
    fn msg_assistant(t: &str) -> serde_json::Value {
        crate::llm::store_message(&ChatMessage::assistant(t))
    }
    fn msg_tool_result(id: &str) -> serde_json::Value {
        crate::llm::store_message(&ChatMessage::tool(
            genai::chat::MessageContent::from_tool_responses(vec![genai::chat::ToolResponse::new(
                id, "out",
            )]),
        ))
    }
    fn call(name: &str) -> crate::item::ToolCallRecord {
        crate::item::ToolCallRecord {
            name: name.into(),
            arguments: "{}".into(),
            result: Some("ok".into()),
        }
    }
    fn sel(ids: &[&str]) -> HashSet<String> {
        ids.iter().map(|s| s.to_string()).collect()
    }
    fn range(start: usize, len: usize) -> Option<MessageRange> {
        Some(MessageRange { start, len })
    }

    /// Deleting the opener drops every message the turn produced.
    #[test]
    fn pruning_an_opener_drops_its_whole_block() {
        let items = vec![
            user_item("u1", "q1", range(0, 4)),
            tool_item("t1", vec![call("bash")], range(1, 2)),
            assistant_item("a1", "a1", range(3, 1)),
            user_item("u2", "q2", range(4, 2)),
            assistant_item("a2", "a2", range(5, 1)),
        ];
        let messages = vec![
            msg_user("q1"),
            msg_assistant("calls"),
            msg_tool_result("c1"),
            msg_assistant("a1"),
            msg_user("q2"),
            msg_assistant("a2"),
        ];
        let (mut kept_items, kept) = prune_session(&items, &messages, &sel(&["u1"]));
        refresh_opener_ranges(&mut kept_items, kept.len());
        assert_eq!(roles(&kept), vec!["User", "Assistant"]);
        assert_eq!(kept_items.len(), 2);
        // The surviving opener and its leaf must point at the shifted messages.
        assert_eq!(kept_items[0].messages_range(), range(0, 2));
        assert_eq!(kept_items[1].messages_range(), range(1, 1));
    }

    /// Deleting a leaf drops only its own message.
    #[test]
    fn pruning_a_leaf_drops_only_its_messages() {
        let items = vec![
            user_item("u1", "q", range(0, 2)),
            assistant_item("a1", "a", range(1, 1)),
        ];
        let messages = vec![msg_user("q"), msg_assistant("a")];
        let (mut kept_items, kept) = prune_session(&items, &messages, &sel(&["a1"]));
        refresh_opener_ranges(&mut kept_items, kept.len());
        assert_eq!(roles(&kept), vec!["User"]);
        assert_eq!(kept_items[0].messages_range(), range(0, 1));
    }

    /// A tool row's display-only preamble goes with the round it introduced.
    #[test]
    fn pruning_a_tool_row_takes_its_preamble_and_round() {
        let items = vec![
            user_item("u1", "q", range(0, 3)),
            assistant_item("a1", "let me check", None),
            tool_item("t1", vec![call("bash"), call("read")], range(1, 3)),
            assistant_item("a2", "done", range(4, 1)),
            user_item("u2", "q2", range(5, 1)),
        ];
        let messages = vec![
            msg_user("q"),
            msg_assistant("calls"),
            msg_tool_result("c1"),
            msg_tool_result("c2"),
            msg_assistant("done"),
            msg_user("q2"),
        ];
        let (mut kept_items, kept) = prune_session(&items, &messages, &sel(&["t1"]));
        refresh_opener_ranges(&mut kept_items, kept.len());
        assert_eq!(roles(&kept), vec!["User", "Assistant", "User"]);
        assert_eq!(kept_items.len(), 3, "the preamble goes with its tool row");
        assert_eq!(kept_items[0].messages_range(), range(0, 2));
        assert_eq!(kept_items[1].messages_range(), range(1, 1));
        assert_eq!(kept_items[2].messages_range(), range(2, 1));
    }

    /// An error line owns no message: deleting it leaves the context alone.
    #[test]
    fn pruning_an_error_line_keeps_the_messages() {
        let items = vec![user_item("u1", "q", range(0, 1)), ChatItem::error("boom")];
        let messages = vec![msg_user("q")];
        let (kept_items, kept) = prune_session(&items, &messages, &sel(&["boom-id"]));
        assert_eq!(kept.len(), 1);
        assert_eq!(kept_items.len(), 2);
    }

    #[test]
    fn pruning_a_group_line_drops_the_injected_turn() {
        let items = vec![
            group_item("g1", "line", range(0, 2)),
            assistant_item("a1", "reply", range(1, 1)),
        ];
        let messages = vec![msg_user("line"), msg_assistant("reply")];
        let (kept_items, kept) = prune_session(&items, &messages, &sel(&["g1"]));
        assert!(kept.is_empty());
        assert!(kept_items.is_empty());
    }

    #[test]
    fn an_out_of_bounds_range_keeps_the_messages_untouched() {
        let items = vec![user_item("u1", "q1", range(5, 5))];
        let messages = vec![msg_user("q1"), msg_assistant("a1")];
        let (kept_items, kept) = prune_session(&items, &messages, &sel(&["u1"]));
        assert!(kept_items.is_empty());
        assert_eq!(kept, messages);
    }

    /// Only openers need a re-count on save; their span runs to the next
    /// opener's first message, or the end of the conversation.
    #[test]
    fn saving_recomputes_each_openers_span() {
        let mut items = vec![
            user_item("u1", "q1", range(1, 0)),
            assistant_item("a1", "a1", range(2, 1)),
            user_item("u2", "q2", range(4, 0)),
            assistant_item("a2", "a2", range(5, 1)),
        ];
        refresh_opener_ranges(&mut items, 6);
        assert_eq!(items[0].messages_range(), range(1, 3));
        assert_eq!(items[2].messages_range(), range(4, 2));
        assert_eq!(
            items[1].messages_range(),
            range(2, 1),
            "leaves keep the range their producer wrote"
        );
    }

    /// A session whose turn opened but produced no message yet still saves: the
    /// opener spans zero messages instead of inventing one.
    #[test]
    fn an_open_turn_with_no_messages_gets_an_empty_span() {
        let mut items = vec![user_item("u1", "q1", range(3, 0))];
        refresh_opener_ranges(&mut items, 3);
        assert_eq!(items[0].messages_range(), range(3, 0));
    }

    /// The field is part of the on-disk format: an old session without it is
    /// rejected instead of being given invented ranges.
    #[test]
    fn an_old_session_file_without_message_ranges_is_rejected() {
        let disk = r#"{
            "id": "s-1",
            "title": "hello",
            "created_at": "2026-10-19T14:31:56.452",
            "updated_at": "2026-10-26T17:36:48.761",
            "messages": [],
            "items": [
                { "id": "i1", "ts": 1, "type": "user", "content": "hello", "images": [] }
            ]
        }"#;
        assert!(serde_json::from_str::<Session>(disk).is_err());
    }

    #[test]
    fn completion_flips_only_the_tool_call_that_launched_the_task() {
        let running =
            |task: &str| serde_json::json!({ "task_id": task, "status": "running" }).to_string();
        let call = |result: &str| crate::item::ToolCallRecord {
            name: "bash".into(),
            arguments: "{}".into(),
            result: Some(result.to_string()),
        };
        let items = vec![
            tool_item("i1", vec![call(&running("t-1")), call(&running("t-2"))], None),
            // A plain (non-JSON) result must not be touched, nor parsed as one.
            tool_item("i2", vec![call("ok")], None),
        ];
        let out = apply_completion_to_items(items, "t-2", "exit 0");
        let ChatItem::Tool { tool_calls, .. } = &out[0] else {
            panic!()
        };
        assert_eq!(
            tool_calls[0].result.as_deref(),
            Some(running("t-1").as_str()),
            "other task keeps its placeholder"
        );
        assert_eq!(tool_calls[1].result.as_deref(), Some("exit 0"));
        let ChatItem::Tool { tool_calls, .. } = &out[1] else {
            panic!()
        };
        assert_eq!(tool_calls[0].result.as_deref(), Some("ok"));
    }
}
