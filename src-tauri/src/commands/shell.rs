//! Thin Tauri wrappers over `pet_core::shell`'s task queries.

use pet_core::shell::{self, ShellResult, ShellStore, TaskListItem};
use tauri::State;

#[tauri::command]
pub fn check_task_status(
    task_id: String,
    store: State<'_, ShellStore>,
) -> Result<ShellResult, String> {
    shell::check_task_status(store.inner(), &task_id)
}

/// List all tracked tasks (running + up to 200 recently finished tasks). The UI
/// groups and sorts them.
#[tauri::command]
pub fn list_tasks(store: State<'_, ShellStore>) -> Vec<TaskListItem> {
    shell::list_tasks(store.inner())
}

/// Kill a running task and tell the pet it was cancelled. The core marks the
/// task finished and hands back one clean completion; deliver it via the same
/// `background-finished` event the frontend already handles, so the
/// conversation reacts to the cancellation.
#[tauri::command]
pub fn kill_task(
    task_id: String,
    app: tauri::AppHandle,
    store: State<'_, ShellStore>,
) -> Result<(), String> {
    if let Some(completion) = shell::kill_task(store.inner(), &task_id)? {
        crate::commands::chat::emit_background_finished(&app, completion);
    }
    Ok(())
}
