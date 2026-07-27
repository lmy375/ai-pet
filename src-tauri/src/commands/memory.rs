//! Thin Tauri wrappers over `pet_core::memory`.

use pet_core::memory;

#[tauri::command]
pub fn get_soul(agent_id: String) -> Result<String, String> {
    memory::get_soul(agent_id)
}

#[tauri::command]
pub fn save_soul(agent_id: String, content: String) -> Result<(), String> {
    memory::save_soul(agent_id, content)
}

#[tauri::command]
pub fn get_user(agent_id: String) -> Result<String, String> {
    memory::get_user(agent_id)
}

#[tauri::command]
pub fn save_user(agent_id: String, content: String) -> Result<(), String> {
    memory::save_user(agent_id, content)
}

#[tauri::command]
pub fn get_memory(agent_id: String) -> Result<String, String> {
    memory::get_memory(agent_id)
}

#[tauri::command]
pub fn save_memory(agent_id: String, content: String) -> Result<(), String> {
    memory::save_memory(agent_id, content)
}

#[tauri::command]
pub fn open_memory_dir(app: tauri::AppHandle, agent_id: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let dir = memory::ensure_memory_dir(&agent_id)?;
    app.opener()
        .open_path(dir.to_string_lossy().to_string(), None::<&str>)
        .map_err(|e| format!("Failed to open memory dir: {e}"))
}
