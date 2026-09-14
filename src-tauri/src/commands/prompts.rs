//! Thin Tauri wrappers over `pet_core::prompts` — the owner-editable system
//! prompts. `content` is always the text in force (the override if there is one,
//! otherwise the built-in default), so the editor shows what the model reads.

use pet_core::prompts::{self, PromptKey};
use serde::Serialize;

#[derive(Serialize)]
pub struct PromptInfo {
    /// Identifier the UI passes back to save/reset (`persona`, `tool_usage`, …).
    pub key: PromptKey,
    /// Where an override lives, shown so it can be edited outside the app too.
    pub path: String,
    /// True when an override file exists — i.e. this one no longer follows the
    /// built-in text.
    pub customized: bool,
    /// The text currently in force.
    pub content: String,
    /// Placeholders this prompt may use.
    pub vars: Vec<String>,
    /// Placeholders it must keep (`save` refuses an edit that drops one).
    pub required_vars: Vec<String>,
}

fn info(key: PromptKey) -> PromptInfo {
    PromptInfo {
        key,
        path: key.path().map(|p| p.to_string_lossy().to_string()).unwrap_or_default(),
        customized: key.is_customized(),
        content: prompts::text(key),
        vars: key.vars().iter().map(|v| v.to_string()).collect(),
        required_vars: key.required_vars().iter().map(|v| v.to_string()).collect(),
    }
}

/// Every system prompt with its current text. One call — the whole set is a few
/// KB, and the settings page shows them together.
#[tauri::command]
pub fn list_prompts() -> Vec<PromptInfo> {
    PromptKey::ALL.into_iter().map(info).collect()
}

#[tauri::command]
pub fn save_prompt(key: PromptKey, content: String) -> Result<PromptInfo, String> {
    prompts::save(key, &content)?;
    Ok(info(key))
}

/// Drop the override and go back to the built-in text.
#[tauri::command]
pub fn reset_prompt(key: PromptKey) -> Result<PromptInfo, String> {
    prompts::reset(key)?;
    Ok(info(key))
}

/// Reveal `<config>/prompts/`, creating it first so the button works before
/// anything has been overridden. Mirrors `open_skills_dir`.
#[tauri::command]
pub fn open_prompts_dir(app: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let dir = prompts::ensure_prompts_dir()?;
    app.opener()
        .open_path(dir.to_string_lossy().to_string(), None::<&str>)
        .map_err(|e| format!("Failed to open prompts dir: {e}"))
}
