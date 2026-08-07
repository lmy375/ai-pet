//! Thin Tauri wrappers over `pet_core::skills`. Read-only as far as skills go —
//! they're authored on disk, never through the UI; only the directory is a
//! setting, and it's written through the existing `save_settings` (which is what
//! emits `settings-changed`).

use pet_core::skills::{self, Skill};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct SkillsInfo {
    /// The directory actually scanned (settings value with `~` expanded).
    pub dir: String,
    /// Quick-set candidates for the settings field, in the form stored in
    /// config. Built here rather than in the frontend because only the Rust
    /// side can resolve the app's own config dir.
    pub presets: Vec<String>,
    pub skills: Vec<Skill>,
}

#[tauri::command]
pub fn list_skills() -> SkillsInfo {
    let dir = skills::skills_dir();
    let config_skills = pet_core::common::config_dir()
        .map(|d| d.join("skills").to_string_lossy().to_string())
        .unwrap_or_default();
    SkillsInfo {
        skills: skills::list_skills_in(&dir),
        dir: dir.to_string_lossy().to_string(),
        presets: vec![
            skills::DEFAULT_SKILLS_DIR.to_string(),
            "~/.claude/skills".to_string(),
            config_skills,
        ],
    }
}

/// Reveal the skills directory, creating it first so the button also works
/// before the owner has added any skill. Mirrors `open_config_dir`.
#[tauri::command]
pub fn open_skills_dir(app: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let dir = skills::ensure_skills_dir()?;
    app.opener()
        .open_path(dir.to_string_lossy().to_string(), None::<&str>)
        .map_err(|e| format!("Failed to open skills dir: {}", e))
}

/// Expand a `/skill:<slug> [task]` line into the message actually sent to the
/// model. `None` when the line isn't a skill command or no skill matches, in
/// which case the caller sends the text as typed. Keeping this in the engine is
/// what stops the expansion wording from being duplicated in TypeScript.
#[tauri::command]
pub fn expand_skill_command(line: String) -> Option<String> {
    skills::expand_command(&line, &skills::list_skills())
}
