use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;

fn sessions_dir() -> Result<PathBuf, String> {
    let dir = crate::common::config_dir()?.join("sessions");
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create sessions dir: {e}"))?;
    Ok(dir)
}

fn index_path() -> Result<PathBuf, String> {
    Ok(sessions_dir()?.join("index.json"))
}

fn session_path(id: &str) -> Result<PathBuf, String> {
    Ok(sessions_dir()?.join(format!("{id}.json")))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub messages: Vec<serde_json::Value>,
    pub items: Vec<serde_json::Value>,
    #[serde(default)]
    pub context_usage: Option<ContextUsage>,
}

fn read_index() -> SessionIndex {
    index_path()
        .ok()
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|c| serde_json::from_str(&c).ok())
        .unwrap_or(SessionIndex {
            active_id: String::new(),
            sessions: vec![],
        })
}

/// Serialize `value` as pretty JSON and write it to `path`. `what` names the
/// thing in error messages (e.g. "index", "session").
fn write_json_pretty<T: Serialize>(path: &PathBuf, value: &T, what: &str) -> Result<(), String> {
    let json = serde_json::to_string_pretty(value)
        .map_err(|e| format!("Failed to serialize {what}: {e}"))?;
    fs::write(path, json).map_err(|e| format!("Failed to write {what}: {e}"))
}

fn write_index(index: &SessionIndex) -> Result<(), String> {
    write_json_pretty(&index_path()?, index, "index")
}

// --- Display-transcript ("ChatItem") constructors ---
//
// The `items` array is shaped by the frontend (see `ChatItem` in useChat.ts) and
// round-tripped here as opaque JSON. These build the two shapes the backend
// itself authors (the Telegram path and the `chat` tool), so the field names
// ("type"/"content"/"images") live in one place instead of being hand-written at
// each call site and silently drifting from the TS union.

/// A stable per-item id. Every producer stamps one at creation: display items
/// are addressed by id (React keys, multi-select, `prune_session`), so an item
/// that reaches disk without one can't be referred to at all.
pub fn item_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Epoch milliseconds, the display timestamp carried by every item.
pub fn item_ts() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// A `user` display item. `images` are data URLs shown alongside the text.
pub fn user_item(content: &str, images: &[String]) -> serde_json::Value {
    serde_json::json!({
        "id": item_id(),
        "ts": item_ts(),
        "type": "user",
        "content": content,
        "images": images,
    })
}

/// An `assistant` display item carrying `images` (e.g. a screenshot the pet
/// produced). Pass `&[]` for a plain text bubble.
pub fn assistant_item(content: &str, images: &[String]) -> serde_json::Value {
    let mut item = serde_json::json!({
        "id": item_id(),
        "ts": item_ts(),
        "type": "assistant",
        "content": content,
    });
    if !images.is_empty() {
        item["images"] = serde_json::json!(images);
    }
    item
}

/// Derive a session title from its display items: the first non-empty `user`
/// item's text, truncated to 20 Unicode scalar values with an ellipsis. Returns
/// `None` when there's no usable user text, so callers pick their own fallback.
pub fn derive_title(items: &[serde_json::Value]) -> Option<String> {
    items
        .iter()
        .find(|i| i["type"] == "user")
        .and_then(|i| i["content"].as_str())
        .filter(|c| !c.is_empty())
        .map(|c| {
            let t: String = c.chars().take(20).collect();
            if c.chars().count() > 20 {
                format!("{}...", t)
            } else {
                t
            }
        })
}

/// A `{ "role": "system", "content": <SOUL.md> }` message — the first message
/// of every new session, seeded with the given agent's SOUL. The seed is only
/// the initial system message; every chat turn rebuilds it via
/// `prepend_system_messages`, so it's effectively cosmetic.
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
        messages: vec![soul_system_message(agent_id)],
        items: vec![],
        context_usage: None,
    };
    save_session(session.clone())?;
    Ok(session)
}

pub fn list_sessions() -> SessionIndex {
    read_index()
}

/// Mark `id` as the active session in the shared index, without rewriting the
/// session file. `active_id` is the only cross-window pointer to "which session
/// is current" (disk is the sole shared state), and otherwise it's only updated
/// as a side effect of `save_session`. Switching sessions in one window must
/// persist the choice here so the other window's focus-reload converges on it
/// instead of reverting to whatever was last saved (the newest session).
pub fn set_active_session(id: String) -> Result<(), String> {
    let mut index = read_index();
    if !index.sessions.iter().any(|m| m.id == id) {
        return Err(format!("Unknown session {id}"));
    }
    index.active_id = id;
    write_index(&index)
}

pub fn load_session(id: String) -> Result<Session, String> {
    let path = session_path(&id)?;
    let content =
        fs::read_to_string(&path).map_err(|e| format!("Failed to read session {id}: {e}"))?;
    serde_json::from_str(&content).map_err(|e| format!("Failed to parse session {id}: {e}"))
}

pub fn save_session(mut session: Session) -> Result<(), String> {
    let path = session_path(&session.id)?;

    // Preserve created_at (and last-known usage) from the existing file when the
    // caller didn't supply them — e.g. a turn whose provider omitted usage, or a
    // Telegram save, shouldn't blank out a previously-recorded occupancy.
    if session.created_at.is_empty() || session.context_usage.is_none() {
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(existing) = serde_json::from_str::<Session>(&content) {
                if session.created_at.is_empty() {
                    session.created_at = existing.created_at;
                }
                if session.context_usage.is_none() {
                    session.context_usage = existing.context_usage;
                }
            }
        }
    }

    // Write session file
    write_json_pretty(&path, &session, "session")?;

    // Update index
    let mut index = read_index();
    index.active_id = session.id.clone();
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

    let path = session_path(&id)?;
    if let Ok(content) = fs::read_to_string(&path) {
        if let Ok(mut session) = serde_json::from_str::<Session>(&content) {
            session.title = title.clone();
            write_json_pretty(&path, &session, "session")?;
        }
    }

    let mut index = read_index();
    if let Some(meta) = index.sessions.iter_mut().find(|m| m.id == id) {
        meta.title = title;
        write_index(&index)?;
    }
    Ok(())
}

pub fn create_session() -> Result<Session, String> {
    let agent_id = crate::settings::active_agent_id();
    new_seeded_session(&agent_id, Uuid::new_v4().to_string(), "新会话".to_string())
}

/// Return the tail of `messages` covering the last `n` conversation turns,
/// where a turn starts at a `user` message and runs up to the next one. The
/// slice always begins at a `user` boundary, so it never starts with an orphan
/// `tool` message (which a chat-completions API rejects — a `tool` message must
/// follow the assistant message that requested it). Any leading system messages
/// are dropped as a side effect, which is fine: the heartbeat re-inserts its own
/// system messages via `prepend_heartbeat_system_messages`.
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

pub fn delete_session(id: String) -> Result<(), String> {
    // Remove file
    let path = session_path(&id)?;
    if path.exists() {
        fs::remove_file(&path).map_err(|e| format!("Failed to delete session file: {e}"))?;
    }

    // Update index
    let mut index = read_index();
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
    fn recent_turns_edge_cases() {
        assert!(recent_turns(&[], 3).is_empty());
        // Zero turns means no history, regardless of what's there.
        assert!(recent_turns(&[user("q")], 0).is_empty());
        // No user message → nothing to anchor a turn on.
        assert!(recent_turns(&[system("x")], 3).is_empty());
    }
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
    items: &[serde_json::Value],
    messages: &[serde_json::Value],
    selected: &std::collections::HashSet<String>,
) -> (Vec<serde_json::Value>, Vec<serde_json::Value>) {
    let item_type = |it: &serde_json::Value| it["type"].as_str().unwrap_or("").to_string();
    let opens_turn = |it: &serde_json::Value| matches!(item_type(it).as_str(), "user" | "notification");
    let bears_assistant_message = |it: &serde_json::Value| {
        item_type(it) == "assistant" && !it["content"].as_str().unwrap_or("").trim().is_empty()
    };
    let is_selected =
        |it: &serde_json::Value| it["id"].as_str().is_some_and(|id| selected.contains(id));

    let kept_items: Vec<serde_json::Value> =
        items.iter().filter(|it| !is_selected(it)).cloned().collect();

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
        if opens_turn(it) {
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

    let mut drop: std::collections::HashSet<usize> = std::collections::HashSet::new();
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

#[cfg(test)]
mod prune_tests {
    use super::*;
    use genai::chat::{ChatMessage, MessageContent, ToolCall, ToolResponse};
    use std::collections::HashSet;

    fn msg_user(t: &str) -> serde_json::Value {
        crate::llm::store_message(&ChatMessage::user(t))
    }
    fn msg_assistant(t: &str) -> serde_json::Value {
        crate::llm::store_message(&ChatMessage::assistant(t))
    }
    fn msg_tool_call(id: &str) -> serde_json::Value {
        crate::llm::store_message(&ChatMessage::from(vec![ToolCall {
            call_id: id.to_string(),
            fn_name: "bash".to_string(),
            fn_arguments: serde_json::json!({}),
            thought_signatures: None,
        }]))
    }
    fn msg_tool_result(id: &str) -> serde_json::Value {
        crate::llm::store_message(&ChatMessage::tool(MessageContent::from_tool_responses(
            vec![ToolResponse::new(id, "out")],
        )))
    }
    fn item(id: &str, ty: &str, content: &str) -> serde_json::Value {
        serde_json::json!({ "id": id, "type": ty, "content": content })
    }
    fn sel(ids: &[&str]) -> HashSet<String> {
        ids.iter().map(|s| s.to_string()).collect()
    }
    fn roles(msgs: &[serde_json::Value]) -> Vec<String> {
        msgs.iter().map(|m| m["role"].as_str().unwrap_or("").to_string()).collect()
    }

    /// A tool-using turn followed by a plain one — the shape the old
    /// frontend-side pairing could not represent at all, since it assumed
    /// messages held nothing but user/assistant text.
    fn fixture() -> (Vec<serde_json::Value>, Vec<serde_json::Value>) {
        let items = vec![
            item("i1", "user", "q1"),
            item("i2", "tool", ""),
            item("i3", "assistant", "a1"),
            item("i4", "user", "q2"),
            item("i5", "assistant", "a2"),
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
        let items = vec![item("i1", "user", "q1"), item("i2", "assistant", "a1")];
        let messages = vec![
            crate::llm::store_message(&ChatMessage::system("soul")),
            msg_user("q1"),
            msg_assistant("a1"),
        ];
        let (_, new_msgs) = prune_session(&items, &messages, &sel(&["i1"]));
        assert_eq!(roles(&new_msgs), vec!["System"]);
    }

    #[test]
    fn structural_mismatch_leaves_the_context_untouched() {
        // Items claim two turns, messages hold one — a hand-edited session.
        let items = vec![item("i1", "user", "q1"), item("i2", "user", "q2")];
        let messages = vec![msg_user("q1"), msg_assistant("a1")];
        let (new_items, new_msgs) = prune_session(&items, &messages, &sel(&["i1"]));
        assert_eq!(new_items.len(), 1);
        assert_eq!(new_msgs, messages, "messages must survive an unreliable mapping");
    }
}
