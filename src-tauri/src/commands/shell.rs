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
/// task finished and hands back one clean completion; deliver it to the turn
/// runner like any other completion, so the conversation reacts to the
/// cancellation with a follow-up turn.
#[tauri::command]
pub fn kill_task(
    task_id: String,
    store: State<'_, ShellStore>,
    turns: State<'_, crate::commands::chat::TurnStore>,
) -> Result<(), String> {
    if let Some(completion) = shell::kill_task(store.inner(), &task_id)? {
        turns.0.deliver(completion);
    }
    Ok(())
}
