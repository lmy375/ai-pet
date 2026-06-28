//! Long-term memory for the pet, stored per-agent under `<config>/memory/<id>/`.
//!
//! Three mandatory files form the always-injected "hot" layer:
//! - `SOUL.md`   — the pet's nature / persona (human-authored, read-only to the pet)
//! - `USER.md`   — facts and preferences about the owner (pet maintains)
//! - `MEMORY.md` — the pet's own long-term memory: valuable understanding,
//!   thoughts, judgments (pet maintains) — not a diary/daily log
//!
//! Any subfiles the pet creates under `memory/<id>/` are a "cold" layer: not
//! injected, reached on demand via `read_file` through `[[link]]` references in
//! the main files. Nothing is ever auto-expired — forgetting is the pet's own
//! deliberate act. Each agent has its own isolated memory directory.

use std::fs;
use std::path::PathBuf;

/// The base `memory/` directory (parent of all per-agent dirs).
fn memory_root() -> Result<PathBuf, String> {
    Ok(crate::common::config_dir()?.join("memory"))
}

/// This agent's memory directory: `<config>/memory/<agent_id>/`.
pub fn memory_dir(agent_id: &str) -> Result<PathBuf, String> {
    Ok(memory_root()?.join(agent_id))
}

fn soul_path(agent_id: &str) -> Result<PathBuf, String> {
    Ok(memory_dir(agent_id)?.join("SOUL.md"))
}

pub fn user_path(agent_id: &str) -> Result<PathBuf, String> {
    Ok(memory_dir(agent_id)?.join("USER.md"))
}

pub fn memory_path(agent_id: &str) -> Result<PathBuf, String> {
    Ok(memory_dir(agent_id)?.join("MEMORY.md"))
}

fn default_soul() -> String {
    "你是一个可爱的二次元少女 AI 宠物，性格活泼开朗。请用简短可爱的方式回复，偶尔使用颜文字。回复控制在50字以内。".to_string()
}

fn default_user() -> String {
    "# 关于主人\n\n（这里记录你逐渐了解到的、关于主人的事实与偏好：他是谁、做什么、喜欢什么、提过的要求。随聊天慢慢积累，就地整理，不要堆重复。）\n".to_string()
}

fn default_memory() -> String {
    "# 我的记忆\n\n（这里是你自己的长期记忆：值得长期记住的理解、想法、判断。只记真正有价值的，这不是日记，不要流水账记录每天/每次对话发生了什么。没有东西会自动消失。）\n".to_string()
}

fn read_file_or(path: Result<PathBuf, String>, default: fn() -> String) -> String {
    path.ok()
        .and_then(|p| fs::read_to_string(p).ok())
        .unwrap_or_else(default)
}

/// Create the agent's memory dir and the three mandatory files (with defaults)
/// if missing. Idempotent.
pub fn ensure_memory_files(agent_id: &str) -> Result<(), String> {
    let dir = memory_dir(agent_id)?;
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create memory dir: {e}"))?;

    let soul = soul_path(agent_id)?;
    if !soul.exists() {
        fs::write(&soul, default_soul()).map_err(|e| format!("Failed to write SOUL.md: {e}"))?;
    }

    let user = user_path(agent_id)?;
    if !user.exists() {
        fs::write(&user, default_user()).map_err(|e| format!("Failed to write USER.md: {e}"))?;
    }

    let mem = memory_path(agent_id)?;
    if !mem.exists() {
        fs::write(&mem, default_memory()).map_err(|e| format!("Failed to write MEMORY.md: {e}"))?;
    }

    Ok(())
}

/// The SOUL content (persona) for an agent, used to seed new sessions and the
/// system prompt. Returns the default if the file is missing.
pub fn read_soul(agent_id: &str) -> String {
    read_file_or(soul_path(agent_id), default_soul)
}

/// The USER.md content (facts/preferences about the owner) for an agent.
pub fn read_user(agent_id: &str) -> String {
    read_file_or(user_path(agent_id), default_user)
}

/// The MEMORY.md content (the agent's own long-term memory).
pub fn read_memory(agent_id: &str) -> String {
    read_file_or(memory_path(agent_id), default_memory)
}

#[tauri::command]
pub fn get_soul(agent_id: String) -> Result<String, String> {
    ensure_memory_files(&agent_id)?;
    fs::read_to_string(soul_path(&agent_id)?).map_err(|e| format!("Failed to read SOUL.md: {e}"))
}

#[tauri::command]
pub fn save_soul(agent_id: String, content: String) -> Result<(), String> {
    let path = soul_path(&agent_id)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create memory dir: {e}"))?;
    }
    fs::write(&path, content).map_err(|e| format!("Failed to write SOUL.md: {e}"))
}

#[tauri::command]
pub fn get_user(agent_id: String) -> Result<String, String> {
    ensure_memory_files(&agent_id)?;
    fs::read_to_string(user_path(&agent_id)?).map_err(|e| format!("Failed to read USER.md: {e}"))
}

#[tauri::command]
pub fn save_user(agent_id: String, content: String) -> Result<(), String> {
    let path = user_path(&agent_id)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create memory dir: {e}"))?;
    }
    fs::write(&path, content).map_err(|e| format!("Failed to write USER.md: {e}"))
}

#[tauri::command]
pub fn get_memory(agent_id: String) -> Result<String, String> {
    ensure_memory_files(&agent_id)?;
    fs::read_to_string(memory_path(&agent_id)?).map_err(|e| format!("Failed to read MEMORY.md: {e}"))
}

#[tauri::command]
pub fn save_memory(agent_id: String, content: String) -> Result<(), String> {
    let path = memory_path(&agent_id)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create memory dir: {e}"))?;
    }
    fs::write(&path, content).map_err(|e| format!("Failed to write MEMORY.md: {e}"))
}

#[tauri::command]
pub fn open_memory_dir(app: tauri::AppHandle, agent_id: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let dir = memory_dir(&agent_id)?;
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create memory dir: {e}"))?;
    app.opener()
        .open_path(dir.to_string_lossy().to_string(), None::<&str>)
        .map_err(|e| format!("Failed to open memory dir: {e}"))
}
