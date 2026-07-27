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
