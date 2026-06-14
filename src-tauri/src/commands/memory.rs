//! Long-term memory for the pet, stored under `<config>/memory/`.
//!
//! Three mandatory files form the always-injected "hot" layer:
//! - `SOUL.md`   — the pet's nature / persona (human-authored, read-only to the pet)
//! - `USER.md`   — facts and preferences about the owner (pet maintains)
//! - `MEMORY.md` — the pet's own journal: understanding, thoughts (pet maintains)
//!
//! Any subfiles the pet creates under `memory/` are a "cold" layer: not injected,
//! reached on demand via `read_file` through `[[link]]` references in the main
//! files. Nothing is ever auto-expired — forgetting is the pet's own deliberate act.

use std::fs;
use std::path::PathBuf;

pub fn memory_dir() -> Result<PathBuf, String> {
    Ok(crate::common::config_dir()?.join("memory"))
}

fn soul_path() -> Result<PathBuf, String> {
    Ok(memory_dir()?.join("SOUL.md"))
}

pub fn user_path() -> Result<PathBuf, String> {
    Ok(memory_dir()?.join("USER.md"))
}

pub fn memory_path() -> Result<PathBuf, String> {
    Ok(memory_dir()?.join("MEMORY.md"))
}

fn default_soul() -> String {
    "你是一个可爱的二次元少女 AI 宠物，性格活泼开朗。请用简短可爱的方式回复，偶尔使用颜文字。回复控制在50字以内。".to_string()
}

fn default_user() -> String {
    "# 关于主人\n\n（这里记录你逐渐了解到的、关于主人的事实与偏好：他是谁、做什么、喜欢什么、提过的要求。随聊天慢慢积累，就地整理，不要堆重复。）\n".to_string()
}

fn default_memory() -> String {
    "# 我的记忆\n\n（这里是你自己的日记：你的理解、想法、想记住的事。像写日记，不要记流水账。没有东西会自动消失。）\n".to_string()
}

fn read_file_or(path: Result<PathBuf, String>, default: fn() -> String) -> String {
    path.ok()
        .and_then(|p| fs::read_to_string(p).ok())
        .unwrap_or_else(default)
}

/// Create the memory dir and the three mandatory files (with defaults) if
/// missing. Idempotent.
pub fn ensure_memory_files() -> Result<(), String> {
    let dir = memory_dir()?;
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create memory dir: {e}"))?;

    let soul = soul_path()?;
    if !soul.exists() {
        fs::write(&soul, default_soul()).map_err(|e| format!("Failed to write SOUL.md: {e}"))?;
    }

    let user = user_path()?;
    if !user.exists() {
        fs::write(&user, default_user()).map_err(|e| format!("Failed to write USER.md: {e}"))?;
    }

    let mem = memory_path()?;
    if !mem.exists() {
        fs::write(&mem, default_memory()).map_err(|e| format!("Failed to write MEMORY.md: {e}"))?;
    }

    Ok(())
}

/// The SOUL content (persona), used to seed new sessions and the system prompt.
/// Returns the default if the file is missing — call `ensure_memory_files` first
/// when you need the file to exist on disk.
pub fn read_soul() -> String {
    read_file_or(soul_path(), default_soul)
}

/// The USER.md content (facts/preferences about the owner).
pub fn read_user() -> String {
    read_file_or(user_path(), default_user)
}

/// The MEMORY.md content (the pet's own journal).
pub fn read_memory() -> String {
    read_file_or(memory_path(), default_memory)
}

#[tauri::command]
pub fn get_soul() -> Result<String, String> {
    ensure_memory_files()?;
    fs::read_to_string(soul_path()?).map_err(|e| format!("Failed to read SOUL.md: {e}"))
}

#[tauri::command]
pub fn save_soul(content: String) -> Result<(), String> {
    let path = soul_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create memory dir: {e}"))?;
    }
    fs::write(&path, content).map_err(|e| format!("Failed to write SOUL.md: {e}"))
}

#[tauri::command]
pub fn get_user() -> Result<String, String> {
    ensure_memory_files()?;
    fs::read_to_string(user_path()?).map_err(|e| format!("Failed to read USER.md: {e}"))
}

#[tauri::command]
pub fn save_user(content: String) -> Result<(), String> {
    let path = user_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create memory dir: {e}"))?;
    }
    fs::write(&path, content).map_err(|e| format!("Failed to write USER.md: {e}"))
}

#[tauri::command]
pub fn get_memory() -> Result<String, String> {
    ensure_memory_files()?;
    fs::read_to_string(memory_path()?).map_err(|e| format!("Failed to read MEMORY.md: {e}"))
}

#[tauri::command]
pub fn save_memory(content: String) -> Result<(), String> {
    let path = memory_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create memory dir: {e}"))?;
    }
    fs::write(&path, content).map_err(|e| format!("Failed to write MEMORY.md: {e}"))
}

#[tauri::command]
pub fn open_memory_dir(app: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let dir = memory_dir()?;
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create memory dir: {e}"))?;
    app.opener()
        .open_path(dir.to_string_lossy().to_string(), None::<&str>)
        .map_err(|e| format!("Failed to open memory dir: {e}"))
}
