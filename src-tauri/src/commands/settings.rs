//! Thin Tauri wrappers over `pet_core::settings`. Settings-mutating commands
//! emit `settings-changed` after the write — each window holds its own
//! in-memory settings copy and reloads on that event (without it the pet window
//! wouldn't react to a panel-side change, e.g. enabling gallery mode, until
//! refocused). Keep the emit on any new settings-writing command.

use pet_core::settings::{self, AppSettings};
use tauri::Emitter;

fn emit_settings_changed(app: &tauri::AppHandle) {
    let _ = app.emit("settings-changed", ());
}

#[tauri::command]
pub fn get_settings() -> Result<AppSettings, String> {
    settings::get_settings()
}

#[tauri::command]
pub fn save_settings(app: tauri::AppHandle, settings: AppSettings) -> Result<(), String> {
    settings::save_settings(&settings)?;
    emit_settings_changed(&app);
    Ok(())
}

/// Switch the active agent (the one answering the desktop chat window).
#[tauri::command]
pub fn set_active_agent(app: tauri::AppHandle, id: String) -> Result<(), String> {
    settings::set_active_agent(&id)?;
    emit_settings_changed(&app);
    Ok(())
}

/// Change one agent's `model` — used by the in-chat model switcher.
#[tauri::command]
pub fn set_agent_model(app: tauri::AppHandle, id: String, model: String) -> Result<(), String> {
    settings::set_agent_model(&id, &model)?;
    emit_settings_changed(&app);
    Ok(())
}

#[tauri::command]
pub fn get_config_raw() -> Result<String, String> {
    settings::get_config_raw()
}

#[tauri::command]
pub fn save_config_raw(app: tauri::AppHandle, content: String) -> Result<(), String> {
    settings::save_config_raw(&content)?;
    emit_settings_changed(&app);
    Ok(())
}

#[tauri::command]
pub async fn list_models(api_base: String, api_key: String) -> Result<Vec<String>, String> {
    settings::list_models(api_base, api_key).await
}

#[tauri::command]
pub async fn test_model(api_base: String, api_key: String, model: String) -> Result<(), String> {
    settings::test_model(api_base, api_key, model).await
}

#[tauri::command]
pub fn open_config_dir(app: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let dir = settings::ensure_config_dir()?;
    app.opener()
        .open_path(dir.to_string_lossy().to_string(), None::<&str>)
        .map_err(|e| format!("Failed to open config dir: {}", e))
}

/// Open an arbitrary directory/file in the OS file manager (e.g. the gallery
/// folder in Finder). Used by the settings "open" buttons.
#[tauri::command]
pub fn open_path(app: tauri::AppHandle, path: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    if path.trim().is_empty() {
        return Err("路径为空".to_string());
    }
    app.opener()
        .open_path(path, None::<&str>)
        .map_err(|e| format!("Failed to open path: {}", e))
}
