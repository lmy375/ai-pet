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
    let path = match index_path() {
        Ok(p) => p,
        Err(_) => {
            return SessionIndex {
                active_id: String::new(),
                sessions: vec![],
            }
        }
    };
    match fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or(SessionIndex {
            active_id: String::new(),
            sessions: vec![],
        }),
        Err(_) => SessionIndex {
            active_id: String::new(),
            sessions: vec![],
        },
    }
}

fn write_index(index: &SessionIndex) -> Result<(), String> {
    let path = index_path()?;
    let json = serde_json::to_string_pretty(index)
        .map_err(|e| format!("Failed to serialize index: {e}"))?;
    fs::write(&path, json).map_err(|e| format!("Failed to write index: {e}"))
}

/// A `{ "role": "system", "content": <SOUL.md> }` message — the first message
/// of every new session. Shared so sessions seeded by the panel and Telegram
/// are identical.
pub fn soul_system_message() -> serde_json::Value {
    let soul = super::memory::read_soul();
    serde_json::json!({ "role": "system", "content": soul })
}

/// Build (and persist) a fresh session with the given id and title, seeded with
/// the SOUL system message. Used by `create_session` and the Telegram bot.
pub fn new_seeded_session(id: String, title: String) -> Result<Session, String> {
    let now = crate::common::iso_now();
    let session = Session {
        id,
        title,
        created_at: now.clone(),
        updated_at: now,
        messages: vec![soul_system_message()],
        items: vec![],
        context_usage: None,
    };
    save_session(session.clone())?;
    Ok(session)
}

#[tauri::command]
pub fn list_sessions() -> SessionIndex {
    read_index()
}

#[tauri::command]
pub fn load_session(id: String) -> Result<Session, String> {
    let path = session_path(&id)?;
    let content =
        fs::read_to_string(&path).map_err(|e| format!("Failed to read session {id}: {e}"))?;
    serde_json::from_str(&content).map_err(|e| format!("Failed to parse session {id}: {e}"))
}

#[tauri::command]
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
    let json = serde_json::to_string_pretty(&session)
        .map_err(|e| format!("Failed to serialize session: {e}"))?;
    fs::write(&path, json).map_err(|e| format!("Failed to write session: {e}"))?;

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
#[tauri::command]
pub fn rename_session(id: String, title: String) -> Result<(), String> {
    let title = title.trim().to_string();
    if title.is_empty() {
        return Err("Title cannot be empty".to_string());
    }

    let path = session_path(&id)?;
    if let Ok(content) = fs::read_to_string(&path) {
        if let Ok(mut session) = serde_json::from_str::<Session>(&content) {
            session.title = title.clone();
            let json = serde_json::to_string_pretty(&session)
                .map_err(|e| format!("Failed to serialize session: {e}"))?;
            fs::write(&path, json).map_err(|e| format!("Failed to write session: {e}"))?;
        }
    }

    let mut index = read_index();
    if let Some(meta) = index.sessions.iter_mut().find(|m| m.id == id) {
        meta.title = title;
        write_index(&index)?;
    }
    Ok(())
}

#[tauri::command]
pub fn create_session() -> Result<Session, String> {
    new_seeded_session(Uuid::new_v4().to_string(), "新会话".to_string())
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
        .filter(|(_, m)| m.get("role").and_then(|r| r.as_str()) == Some("user"))
        .map(|(i, _)| i)
        .collect();
    let start = match user_indices.len() {
        0 => return vec![],
        len if len <= n => user_indices[0],
        len => user_indices[len - n],
    };
    messages[start..].to_vec()
}

#[tauri::command]
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
    use serde_json::json;

    fn roles(msgs: &[serde_json::Value]) -> Vec<String> {
        msgs.iter()
            .map(|m| m["role"].as_str().unwrap_or("").to_string())
            .collect()
    }

    #[test]
    fn recent_turns_starts_at_user_boundary_never_orphan_tool() {
        // A system seed, one tool-using turn, then a plain turn.
        let msgs = vec![
            json!({ "role": "system", "content": "soul" }),
            json!({ "role": "user", "content": "q1" }),
            json!({ "role": "assistant", "content": "", "tool_calls": [{ "id": "c1" }] }),
            json!({ "role": "tool", "tool_call_id": "c1", "content": "result" }),
            json!({ "role": "user", "content": "q2" }),
            json!({ "role": "assistant", "content": "a2" }),
        ];

        // Last 1 turn = from the final user message onward; starts with `user`,
        // never the orphan `tool` from the previous turn.
        let one = recent_turns(&msgs, 1);
        assert_eq!(roles(&one), vec!["user", "assistant"]);
        assert_eq!(one[0]["content"], "q2");

        // Both turns: starts at the first user message, dropping the system seed.
        let two = recent_turns(&msgs, 2);
        assert_eq!(roles(&two), vec!["user", "assistant", "tool", "user", "assistant"]);

        // More turns than exist: same as taking all turns.
        assert_eq!(recent_turns(&msgs, 9), two);

        // n == 0 and no-user inputs both yield empty.
        assert!(recent_turns(&msgs, 0).is_empty());
        assert!(recent_turns(&[json!({ "role": "system", "content": "x" })], 3).is_empty());
    }
}
