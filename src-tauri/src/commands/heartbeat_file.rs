//! HEARTBEAT.md — the pet's scheduled-task list, kept at `<config>/HEARTBEAT.md`.
//!
//! On each scheduled heartbeat the pet wakes up in the background, reads this
//! file as part of its system prompt, and decides whether any timed task is due.
//! The pet maintains the file itself (via edit_file/write_file) when the owner
//! asks for anything recurring or time-based — that's how "set a timer" works.
//! It lives alongside `config.yaml`, separate from the memory journal.

use std::fs;
use std::path::PathBuf;

pub fn heartbeat_path() -> Result<PathBuf, String> {
    Ok(crate::common::config_dir()?.join("HEARTBEAT.md"))
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

/// Current HEARTBEAT.md content, or the default if the file is missing.
pub fn read_heartbeat() -> String {
    heartbeat_path()
        .ok()
        .and_then(|p| fs::read_to_string(p).ok())
        .unwrap_or_else(default_heartbeat)
}

/// Create HEARTBEAT.md with default content if it doesn't exist yet. Idempotent.
pub fn ensure_heartbeat_file() -> Result<(), String> {
    let path = heartbeat_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create config dir: {e}"))?;
    }
    if !path.exists() {
        fs::write(&path, default_heartbeat())
            .map_err(|e| format!("Failed to write HEARTBEAT.md: {e}"))?;
    }
    Ok(())
}

#[tauri::command]
pub fn get_heartbeat() -> Result<String, String> {
    ensure_heartbeat_file()?;
    fs::read_to_string(heartbeat_path()?).map_err(|e| format!("Failed to read HEARTBEAT.md: {e}"))
}

#[tauri::command]
pub fn save_heartbeat(content: String) -> Result<(), String> {
    let path = heartbeat_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create config dir: {e}"))?;
    }
    fs::write(&path, content).map_err(|e| format!("Failed to write HEARTBEAT.md: {e}"))
}
