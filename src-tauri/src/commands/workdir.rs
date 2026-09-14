//! Thin Tauri wrappers over `pet_core::workdir` — the directory the agent works
//! in, shown and switched in the panel's session rail. It lives in memory for
//! the life of the process (the GUI always starts at `$HOME`), so there is
//! nothing to persist and no `settings-changed` to emit.

#[tauri::command]
pub fn get_workdir() -> String {
    pet_core::workdir::get_string()
}

/// Switch the working directory, returning the accepted (canonical) path so the
/// UI shows exactly what the model will be told.
#[tauri::command]
pub fn set_workdir(path: String) -> Result<String, String> {
    pet_core::workdir::set(&path).map(|p| p.to_string_lossy().to_string())
}
