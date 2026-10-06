//! Thin Tauri wrappers over `pet_core::logging`.

use pet_core::logging::{self, LogStore};
use tauri::State;

/// The LLM-log list: one small metadata record per conversation, newest first.
/// The messages stay on disk until a row is opened (`get_llm_entry`).
#[tauri::command]
pub fn get_llm_index() -> Vec<logging::LlmMeta> {
    logging::read_llm_index()
}

/// The full body of one conversation. `None` once compaction has dropped it.
#[tauri::command]
pub fn get_llm_entry(id: String) -> Option<serde_json::Value> {
    logging::read_llm_entry(&id)
}

#[tauri::command]
pub fn get_logs(store: State<'_, LogStore>) -> Vec<String> {
    logging::get_logs(store.inner())
}

#[tauri::command]
pub fn clear_logs(store: State<'_, LogStore>) {
    logging::clear_logs(store.inner())
}
