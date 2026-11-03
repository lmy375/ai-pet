use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;

use crate::item::ChatItem;

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

/// Remove selected display items and the LLM messages behind them.
///
/// `items` and `messages` share no ids and aren't index-aligned, but they're
/// built in the same chronological order, so they're matched structurally:
///
/// - Messages split into *turn blocks*: a `User` message plus everything up to
///   the next one (its assistant replies, tool calls and tool results).
/// - The k-th block pairs with the k-th turn-opening item (`user` /
///   `notification`).
/// - Inside a block, assistant messages carrying text pair in order with that
///   block's non-empty `assistant` items.
///
/// Deleting a turn-opening item drops its whole block, so a tool result can
/// never be left without the call that produced it. Deleting an assistant item
/// drops only that message. Items with no message of their own — tool rows,
/// errors, image-only assistant bubbles — disappear from the transcript and
/// leave the context alone.
///
/// If the structures don't line up (a hand-edited session), items are removed
/// and messages left untouched: a stale context beats a corrupted one.
pub fn prune_session(
    items: &[ChatItem],
    messages: &[serde_json::Value],
    selected: &HashSet<String>,
) -> (Vec<ChatItem>, Vec<serde_json::Value>) {
    let is_selected = |it: &ChatItem| selected.contains(it.id());
    let bears_assistant_message = |it: &ChatItem| it.assistant_text().is_some();

    let kept_items: Vec<ChatItem> = items.iter().filter(|it| !is_selected(it)).cloned().collect();

    // Message turn blocks: [start, end) index ranges, each opened by a `User`.
    let mut blocks: Vec<(usize, usize)> = Vec::new();
    for (i, m) in messages.iter().enumerate() {
        if crate::llm::is_user_message(m) {
            if let Some(last) = blocks.last_mut() {
                last.1 = i;
            }
            blocks.push((i, messages.len()));
        }
    }

    // Item turn blocks, in the same order.
    let mut item_blocks: Vec<Vec<usize>> = Vec::new();
    for (i, it) in items.iter().enumerate() {
        if it.opens_turn() {
            item_blocks.push(vec![i]);
        } else if let Some(block) = item_blocks.last_mut() {
            block.push(i);
        }
    }

    // Anything before the first user message (a system seed) is never paired.
    let leading = blocks.first().map(|(s, _)| *s).unwrap_or(messages.len());
    if item_blocks.len() != blocks.len() {
        return (kept_items, messages.to_vec());
    }

    let mut drop: HashSet<usize> = HashSet::new();
    for (block_idx, item_idxs) in item_blocks.iter().enumerate() {
        let (start, end) = blocks[block_idx];
        if is_selected(&items[item_idxs[0]]) {
            drop.extend(start..end);
            continue;
        }
        // Assistant messages with text, in order, within this block.
        let text_msgs: Vec<usize> = (start..end)
            .filter(|&j| {
                crate::llm::load_messages(&messages[j..=j])
                    .first()
                    .is_some_and(|m| {
                        m.role == genai::chat::ChatRole::Assistant
                            && m.content.texts().iter().any(|t| !t.trim().is_empty())
                    })
            })
            .collect();
        let text_items: Vec<usize> = item_idxs
            .iter()
            .copied()
            .filter(|&i| bears_assistant_message(&items[i]))
            .collect();
        if text_items.len() != text_msgs.len() {
            return (kept_items, messages.to_vec());
        }
        for (k, &item_idx) in text_items.iter().enumerate() {
            if is_selected(&items[item_idx]) {
                drop.insert(text_msgs[k]);
            }
        }
    }

    let kept_messages = messages
        .iter()
        .enumerate()
        .filter(|(j, _)| *j < leading || !drop.contains(j))
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
            genai::chat::MessageContent::from_tool_responses(vec![
                genai::chat::ToolResponse::new(call_id, out),
            ]),
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
                { "id": "i1", "ts": 1, "type": "user", "content": "hello", "images": [] }
            ],
            "context_usage": { "used": 12, "total": 200000 }
        }"#;

        let s: Session = serde_json::from_str(disk).expect("old session files must parse");
        assert_eq!(s.conv.messages.len(), 1);
        assert_eq!(s.conv.items.len(), 1);
        assert_eq!(s.conv.items[0].id(), "i1");
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

    // --- prune_session -----------------------------------------------------

    fn user_item(id: &str, content: &str) -> ChatItem {
        ChatItem::User {
            id: id.into(),
            ts: 0,
            content: content.into(),
            images: vec![],
        }
    }

    fn assistant_item(id: &str, content: &str) -> ChatItem {
        ChatItem::Assistant {
            id: id.into(),
            ts: 0,
            content: content.into(),
            reasoning: None,
            images: vec![],
        }
    }

    fn tool_item(id: &str, tool_calls: Vec<crate::item::ToolCallRecord>) -> ChatItem {
        ChatItem::Tool {
            id: id.into(),
            ts: 0,
            content: String::new(),
            tool_calls,
        }
    }

    fn msg_user(t: &str) -> serde_json::Value {
        crate::llm::store_message(&ChatMessage::user(t))
    }
    fn msg_assistant(t: &str) -> serde_json::Value {
        crate::llm::store_message(&ChatMessage::assistant(t))
    }
    fn msg_tool_call(id: &str) -> serde_json::Value {
        crate::llm::store_message(&ChatMessage::from(vec![genai::chat::ToolCall {
            call_id: id.to_string(),
            fn_name: "bash".to_string(),
            fn_arguments: serde_json::json!({}),
            thought_signatures: None,
        }]))
    }
    fn msg_tool_result(id: &str) -> serde_json::Value {
        crate::llm::store_message(&ChatMessage::tool(
            genai::chat::MessageContent::from_tool_responses(vec![genai::chat::ToolResponse::new(
                id, "out",
            )]),
        ))
    }
    fn sel(ids: &[&str]) -> HashSet<String> {
        ids.iter().map(|s| s.to_string()).collect()
    }

    /// A tool-using turn followed by a plain one — the shape the old
    /// frontend-side pairing could not represent at all, since it assumed
    /// messages held nothing but user/assistant text.
    fn fixture() -> (Vec<ChatItem>, Vec<serde_json::Value>) {
        let items = vec![
            user_item("i1", "q1"),
            tool_item("i2", vec![]),
            assistant_item("i3", "a1"),
            user_item("i4", "q2"),
            assistant_item("i5", "a2"),
        ];
        let messages = vec![
            msg_user("q1"),
            msg_tool_call("c1"),
            msg_tool_result("c1"),
            msg_assistant("a1"),
            msg_user("q2"),
            msg_assistant("a2"),
        ];
        (items, messages)
    }

    #[test]
    fn deleting_a_user_item_drops_its_whole_turn_including_the_tool_round() {
        let (items, messages) = fixture();
        let (new_items, new_msgs) = prune_session(&items, &messages, &sel(&["i1"]));
        assert_eq!(new_items.len(), 4);
        // The tool call and its result go with the turn — leaving a Tool message
        // whose originating call was deleted is rejected by every provider.
        assert_eq!(roles(&new_msgs), vec!["User", "Assistant"]);
    }

    #[test]
    fn deleting_an_assistant_item_drops_only_that_message() {
        let (items, messages) = fixture();
        let (_, new_msgs) = prune_session(&items, &messages, &sel(&["i3"]));
        // The tool round it followed is still a valid, self-contained exchange.
        assert_eq!(roles(&new_msgs), vec!["User", "Assistant", "Tool", "User", "Assistant"]);
    }

    #[test]
    fn deleting_a_tool_item_touches_no_message() {
        let (items, messages) = fixture();
        let (new_items, new_msgs) = prune_session(&items, &messages, &sel(&["i2"]));
        assert_eq!(new_items.len(), 4);
        assert_eq!(new_msgs.len(), messages.len());
    }

    #[test]
    fn a_system_seed_is_never_pruned() {
        let items = vec![user_item("i1", "q1"), assistant_item("i2", "a1")];
        let messages = vec![
            crate::llm::store_message(&ChatMessage::system("soul")),
            msg_user("q1"),
            msg_assistant("a1"),
        ];
        let (_, new_msgs) = prune_session(&items, &messages, &sel(&["i1"]));
        assert_eq!(roles(&new_msgs), vec!["System"]);
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
            tool_item("i1", vec![call(&running("t-1")), call(&running("t-2"))]),
            // A plain (non-JSON) result must not be touched, nor parsed as one.
            tool_item("i2", vec![call("ok")]),
        ];
        let out = apply_completion_to_items(items, "t-2", "exit 0");
        let ChatItem::Tool { tool_calls, .. } = &out[0] else { panic!() };
        assert_eq!(
            tool_calls[0].result.as_deref(),
            Some(running("t-1").as_str()),
            "other task keeps its placeholder"
        );
        assert_eq!(tool_calls[1].result.as_deref(), Some("exit 0"));
        let ChatItem::Tool { tool_calls, .. } = &out[1] else { panic!() };
        assert_eq!(tool_calls[0].result.as_deref(), Some("ok"));
    }

    #[test]
    fn structural_mismatch_leaves_the_context_untouched() {
        // Items claim two turns, messages hold one — a hand-edited session.
        let items = vec![user_item("i1", "q1"), user_item("i2", "q2")];
        let messages = vec![msg_user("q1"), msg_assistant("a1")];
        let (new_items, new_msgs) = prune_session(&items, &messages, &sel(&["i1"]));
        assert_eq!(new_items.len(), 1);
        assert_eq!(new_msgs, messages, "messages must survive an unreliable mapping");
    }
}
