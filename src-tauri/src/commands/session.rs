//! Thin Tauri wrappers over `pet_core::session`. Windows read sessions; they
//! never write one — every write happens in the backend (the turn runner,
//! pruning, rename), so a window can't clobber a turn it isn't watching.

use pet_core::session::{self, SessionIndex, SessionView};
use tauri::State;

use crate::commands::chat::TurnStore;

#[tauri::command]
pub fn list_sessions() -> SessionIndex {
    session::list_sessions()
}

#[tauri::command]
pub fn set_active_session(id: String) -> Result<(), String> {
    session::set_active_session(id)
}

#[tauri::command]
pub fn load_session(id: String) -> Result<SessionView, String> {
    session::load_session_view(id)
}

#[tauri::command]
pub fn create_session() -> Result<SessionView, String> {
    session::create_session().map(SessionView::from)
}

#[tauri::command]
pub fn rename_session(id: String, title: String) -> Result<(), String> {
    session::rename_session(id, title)
}

/// Delete a session, stopping its turn first if one is running (the turn's
/// result is dropped when it finds no file to save into).
#[tauri::command]
pub fn delete_session(id: String, turns: State<'_, TurnStore>) -> Result<(), String> {
    turns.0.cancel(&id);
    session::delete_session(id)
}

/// Delete the selected display items and the LLM messages behind them, then
/// persist. Lives in the backend because deciding which messages an item owns
/// requires understanding the stored message format — which is genai's, and
/// deliberately opaque to the frontend. Refused while a turn is running: the
/// turn will append to both arrays when it finishes and would race the edit.
#[tauri::command]
pub fn prune_session_items(
    id: String,
    item_ids: Vec<String>,
    turns: State<'_, TurnStore>,
) -> Result<SessionView, String> {
    if turns.0.is_running(&id) {
        return Err("Wait for the running reply to finish before deleting messages".to_string());
    }
    let mut sess = session::load_session(id)?;
    let selected: std::collections::HashSet<String> = item_ids.into_iter().collect();
    let (items, messages) = session::prune_session(&sess.items, &sess.messages, &selected);
    sess.items = items;
    sess.messages = messages;
    sess.updated_at = pet_core::common::iso_now();
    session::save_session(sess.clone())?;
    Ok(SessionView::from(sess))
}
