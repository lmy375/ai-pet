//! Thin Tauri wrappers over the agent's always-present markdown files. Each is
//! the same read/write with a different path, so one command pair takes the
//! `kind` instead of four pairs.

use pet_core::{heartbeat_file, memory};
use serde::Deserialize;

/// The files under `memory/<agent>/`: SOUL, USER, MEMORY and HEARTBEAT.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryFile {
    Soul,
    User,
    Memory,
    Heartbeat,
}

#[tauri::command]
pub fn get_memory_file(agent_id: String, kind: MemoryFile) -> Result<String, String> {
    match kind {
        MemoryFile::Soul => memory::get_soul(agent_id),
        MemoryFile::User => memory::get_user(agent_id),
        MemoryFile::Memory => memory::get_memory(agent_id),
        MemoryFile::Heartbeat => heartbeat_file::get_heartbeat(agent_id),
    }
}

#[tauri::command]
pub fn save_memory_file(
    agent_id: String,
    kind: MemoryFile,
    content: String,
) -> Result<(), String> {
    match kind {
        MemoryFile::Soul => memory::save_soul(agent_id, content),
        MemoryFile::User => memory::save_user(agent_id, content),
        MemoryFile::Memory => memory::save_memory(agent_id, content),
        MemoryFile::Heartbeat => heartbeat_file::save_heartbeat(agent_id, content),
    }
}