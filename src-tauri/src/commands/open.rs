//! One "open in the OS file manager" command for every UI affordance. The
//! directory is selected by `kind`; `custom` opens the caller's own path (the
//! gallery / pet-image pickers, whose directories are arbitrary).

use pet_core::{memory, prompts, settings, skills};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpenTarget {
    Config,
    Memory,
    Skills,
    Prompts,
    Custom,
}

#[tauri::command]
pub fn open_dir(
    app: tauri::AppHandle,
    kind: OpenTarget,
    agent_id: Option<String>,
    path: Option<String>,
) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let dir = match kind {
        OpenTarget::Config => settings::ensure_config_dir()?,
        OpenTarget::Memory => match agent_id {
            Some(id) => memory::ensure_memory_dir(&id)?,
            None => return Err("缺少 agent_id".to_string()),
        },
        OpenTarget::Skills => skills::ensure_skills_dir()?,
        OpenTarget::Prompts => prompts::ensure_prompts_dir()?,
        OpenTarget::Custom => {
            let path = path.unwrap_or_default();
            if path.trim().is_empty() {
                return Err("路径为空".to_string());
            }
            std::path::PathBuf::from(path)
        }
    };
    app.opener()
        .open_path(dir.to_string_lossy().to_string(), None::<&str>)
        .map_err(|e| format!("Failed to open {}: {e}", dir.display()))
}