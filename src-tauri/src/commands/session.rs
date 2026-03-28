use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;

fn sessions_dir() -> Result<PathBuf, String> {
    let dir = dirs::config_dir()
        .ok_or_else(|| "Cannot determine config directory".to_string())?
        .join("pet")
        .join("sessions");
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub messages: Vec<serde_json::Value>,
    pub items: Vec<serde_json::Value>,
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

fn now_iso() -> String {
    chrono::Local::now().format("%Y-%m-%dT%H:%M:%S%.3f").to_string()
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

    // Preserve created_at from existing file if not provided
    if session.created_at.is_empty() {
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(existing) = serde_json::from_str::<Session>(&content) {
                session.created_at = existing.created_at;
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

#[tauri::command]
pub fn create_session() -> Result<Session, String> {
    let id = Uuid::new_v4().to_string();
    let now = now_iso();

    // Load current SOUL.md as system message
    let soul = super::settings::get_soul().unwrap_or_default();
    let system_msg = serde_json::json!({ "role": "system", "content": soul });

    let session = Session {
        id: id.clone(),
        title: "新会话".to_string(),
        created_at: now.clone(),
        updated_at: now,
        messages: vec![system_msg],
        items: vec![],
    };

    // Save session file
    save_session(session.clone())?;

    Ok(session)
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
