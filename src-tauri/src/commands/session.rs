//! Thin Tauri wrappers over `pet_core::session`.

use pet_core::session::{self, Session, SessionIndex};

#[tauri::command]
pub fn list_sessions() -> SessionIndex {
    session::list_sessions()
}

#[tauri::command]
pub fn set_active_session(id: String) -> Result<(), String> {
    session::set_active_session(id)
}

#[tauri::command]
pub fn load_session(id: String) -> Result<Session, String> {
    session::load_session(id)
}

#[tauri::command]
pub fn save_session(session: Session) -> Result<(), String> {
    session::save_session(session)
}

#[tauri::command]
pub fn create_session() -> Result<Session, String> {
    session::create_session()
}

#[tauri::command]
pub fn rename_session(id: String, title: String) -> Result<(), String> {
    session::rename_session(id, title)
}

#[tauri::command]
pub fn delete_session(id: String) -> Result<(), String> {
    session::delete_session(id)
}

/// Delete the selected display items and the LLM messages behind them, then
/// persist. Lives in the backend because deciding which messages an item owns
/// requires understanding the stored message format — which is genai's, and
/// deliberately opaque to the frontend.
#[tauri::command]
pub fn prune_session_items(id: String, item_ids: Vec<String>) -> Result<Session, String> {
    let mut sess = session::load_session(id)?;
    let selected: std::collections::HashSet<String> = item_ids.into_iter().collect();
    let (items, messages) = session::prune_session(&sess.items, &sess.messages, &selected);
    sess.items = items;
    sess.messages = messages;
    sess.updated_at = pet_core::common::iso_now();
    sess.created_at = String::new(); // preserved by save_session
    session::save_session(sess.clone())?;
    Ok(sess)
}
