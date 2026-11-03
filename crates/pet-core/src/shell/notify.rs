use std::sync::Arc;

use chrono::Local;
use serde::Serialize;

use crate::shell::history::{collect_history_tasks, write_task_history};
use crate::shell::output::build_shell_result;
use crate::shell::task::{ShellStore, ShellTask, TaskKind, TaskStatus};

/// Payload delivered when a background task finishes. Serialized to the frontend
/// (camelCase) so the main window can resume the conversation with the result.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskCompletion {
    pub session_id: String,
    pub task_id: String,
    pub kind: String,
    pub label: String,
    pub result: String,
}

/// Sink for completion notifications. The Tauri implementation emits an event to
/// the main window; non-UI callers (e.g. Telegram) pass `None`.
pub trait TaskNotifier: Send + Sync {
    fn notify(&self, completion: &TaskCompletion);
}

/// The result string carried in a completion notification: the sub-agent's final
/// text directly, or the full bash result JSON (stdout/stderr/return_code).
fn notify_result_string(task_id: &str, task: &ShellTask) -> String {
    match task.kind {
        TaskKind::Subagent | TaskKind::Heartbeat => task.result.clone().unwrap_or_default(),
        TaskKind::Bash => serde_json::to_string(&build_shell_result(task_id, task))
            .unwrap_or_else(|_| "{}".to_string()),
    }
}

impl ShellStore {
    /// Mark a task finished and fire its completion notification. The payload is
    /// built (and history written) after the lock is released, since for bash it
    /// reads the output files.
    pub fn finish_and_notify(
        &self,
        notifier: &Option<Arc<dyn TaskNotifier>>,
        task_id: &str,
        return_code: Option<i32>,
        result: Option<String>,
    ) {
        let (snapshot, history) = {
            let mut map = self.0.lock().unwrap();
            let task = match map.get_mut(task_id) {
                Some(t) => t,
                None => return,
            };
            // Already finished — e.g. killed by the user via `kill_task`, which
            // marks the task and fires its own notification before the process or
            // future actually unwinds into this waiter. Skip to avoid a duplicate.
            if task.status == TaskStatus::Finished {
                return;
            }
            task.status = TaskStatus::Finished;
            task.return_code = return_code;
            task.finished_at = Some(Local::now());
            if result.is_some() {
                task.result = result;
            }
            (task.clone(), collect_history_tasks(&map))
        };
        write_task_history(&history);
        if let Some(n) = notifier {
            let completion = TaskCompletion {
                session_id: snapshot.session_id.clone(),
                task_id: task_id.to_string(),
                kind: snapshot.kind.as_str().to_string(),
                label: snapshot.label.clone(),
                result: notify_result_string(task_id, &snapshot),
            };
            n.notify(&completion);
        }
    }
}
