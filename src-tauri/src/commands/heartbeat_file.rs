//! Thin Tauri wrappers over `pet_core::heartbeat_file`.

use pet_core::heartbeat_file;

#[tauri::command]
pub fn get_heartbeat(agent_id: String) -> Result<String, String> {
    heartbeat_file::get_heartbeat(agent_id)
}

#[tauri::command]
pub fn save_heartbeat(agent_id: String, content: String) -> Result<(), String> {
    heartbeat_file::save_heartbeat(agent_id, content)
}
