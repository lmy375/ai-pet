//! HEARTBEAT.md — an agent's scheduled-task list, kept in its memory dir at
//! `<config>/memory/<id>/HEARTBEAT.md`, alongside SOUL.md / USER.md / MEMORY.md.
//!
//! On each scheduled heartbeat the agent wakes up in the background, reads this
//! file as part of its system prompt, and decides whether any timed task is due.
//! The agent maintains the file itself (via edit_file/write_file) when the owner
//! asks for anything recurring or time-based — that's how "set a timer" works.

use std::fs;
use std::path::PathBuf;

pub fn heartbeat_path(agent_id: &str) -> Result<PathBuf, String> {
    Ok(crate::commands::memory::memory_dir(agent_id)?.join("HEARTBEAT.md"))
}

fn default_heartbeat() -> String {
    "# 定时任务\n\n\
（这里是你的定时任务清单。每次心跳醒来时你都会读到它，据此判断现在是否到了\
该执行某条任务的时间或条件。\n\n\
- 主人让你做类似「每天/每隔一段时间/到某个点做某事」的事时，把它写进这里，\
写清楚：做什么、什么时候或多久一次、上次执行时间。\n\
- 一次性的提醒做完后就把它删掉；周期性的更新它的「上次执行时间」。\n\
- 没有任何任务时，心跳什么都不用做，安静结束即可。）\n"
        .to_string()
}

/// Current HEARTBEAT.md content for an agent, or the default if missing.
pub fn read_heartbeat(agent_id: &str) -> String {
    heartbeat_path(agent_id)
        .ok()
        .and_then(|p| fs::read_to_string(p).ok())
        .unwrap_or_else(default_heartbeat)
}

/// Create an agent's HEARTBEAT.md with default content if it doesn't exist yet.
/// Idempotent.
pub fn ensure_heartbeat_file(agent_id: &str) -> Result<(), String> {
    let path = heartbeat_path(agent_id)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create memory dir: {e}"))?;
    }
    if !path.exists() {
        fs::write(&path, default_heartbeat())
            .map_err(|e| format!("Failed to write HEARTBEAT.md: {e}"))?;
    }
    Ok(())
}

#[tauri::command]
pub fn get_heartbeat(agent_id: String) -> Result<String, String> {
    ensure_heartbeat_file(&agent_id)?;
    fs::read_to_string(heartbeat_path(&agent_id)?)
        .map_err(|e| format!("Failed to read HEARTBEAT.md: {e}"))
}

#[tauri::command]
pub fn save_heartbeat(agent_id: String, content: String) -> Result<(), String> {
    let path = heartbeat_path(&agent_id)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create config dir: {e}"))?;
    }
    fs::write(&path, content).map_err(|e| format!("Failed to write HEARTBEAT.md: {e}"))
}
