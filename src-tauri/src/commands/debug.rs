//! Thin Tauri wrappers over `pet_core::logging`.

use pet_core::logging::{self, LogStore};
use tauri::State;

#[tauri::command]
pub fn get_llm_logs(limit: Option<usize>) -> Vec<String> {
    logging::get_llm_logs(limit)
}

#[tauri::command]
pub fn get_logs(store: State<'_, LogStore>) -> Vec<String> {
    logging::get_logs(store.inner())
}

#[tauri::command]
pub fn clear_logs(store: State<'_, LogStore>) {
    logging::clear_logs(store.inner())
}
