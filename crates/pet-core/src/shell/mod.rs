//! Background-task bookkeeping: spawn a bash command or a future, track it in
//! a shared store, auto-convert to the background on timeout and notify on
//! completion.
//!
//! This module is infrastructure: it must NOT depend on `crate::tools` (its
//! caller layer). Callers pass the store, session id and notifier explicitly.
//! Sub-modules are split by responsibility; this file only exposes the public
//! task API and the task queries used by the interfaces.

mod history;
mod notify;
mod output;
mod run;
mod task;

pub use notify::{TaskCompletion, TaskNotifier};
pub(crate) use output::build_shell_result;
pub use output::ShellResult;
pub use run::{run_or_background, BackgroundRequest};
pub(crate) use run::{spawn_bash, BashRequest, BashStart, BashWait};
pub use task::{ShellStore, ShellTask, TaskKind};

/// Timeout for a foreground command before it converts to a background task.
pub const DEFAULT_TIMEOUT_MS: u64 = 120_000;
pub const MAX_TIMEOUT_MS: u64 = 600_000;
pub const SHELL_DIR: &str = "/tmp/pet/shell";

// --- Queries / commands ---

/// How many tasks are still due to fire a completion notification: running,
/// backgrounded, registered with a notifier, and not an interactive REPL (a
/// REPL process never exits on its own, so waiting on it would never end).
///
/// Long-lived hosts (GUI/TUI) don't need this — their event loop just receives
/// whatever arrives. The one-shot CLI uses it to know when it may exit without
/// dropping background work.
pub fn pending_notify_count(store: &ShellStore) -> usize {
    store.pending_notify_count()
}

pub fn check_task_status(store: &ShellStore, task_id: &str) -> Result<ShellResult, String> {
    store
        .snapshot(task_id)
        .map(|task| build_shell_result(task_id, &task))
        .ok_or_else(|| format!("Task not found: {}", task_id))
}

/// A single row for the Panel "任务" tab. Built from a `ShellTask` (whose fields
/// are private to the `task` module).
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskListItem {
    pub task_id: String,
    pub kind: String,
    pub label: String,
    pub status: String,
    pub return_code: Option<i32>,
    pub elapsed_ms: u64,
    pub started_at: String,
    pub session_id: String,
}

impl From<&ShellResult> for TaskListItem {
    fn from(r: &ShellResult) -> Self {
        Self {
            task_id: r.task_id.clone(),
            kind: r.kind.as_str().to_string(),
            label: r.label.clone(),
            status: r.status.clone(),
            return_code: r.return_code,
            elapsed_ms: r.execution_time_ms,
            started_at: r.started_at.to_rfc3339(),
            session_id: r.session_id.clone(),
        }
    }
}

/// List all tracked tasks (running + up to 200 recently finished tasks). The UI
/// groups and sorts them.
pub fn list_tasks(store: &ShellStore) -> Vec<TaskListItem> {
    store.list()
}

/// Kill a running task and tell the pet it was cancelled.
///
/// Marks the task finished BEFORE the actual kill so the background waiter, when
/// the process dies / future aborts, sees `Finished` in `ShellStore::finish_and_notify`
/// and skips its own (now-garbled) notification — the caller delivers the one
/// clean `TaskCompletion` returned here instead (the GUI routes it to the active
/// window as `background-finished`). `Ok(None)` means the task was already done.
pub fn kill_task(store: &ShellStore, task_id: &str) -> Result<Option<TaskCompletion>, String> {
    let Some(killed) = store.kill_snapshot(task_id)? else {
        return Ok(None);
    };

    // Stop the actual work: bash by process group (set via process_group(0) at
    // spawn), sub-agent by aborting its work future.
    match killed.kind {
        TaskKind::Bash => {
            if killed.pid != 0 {
                let _ = std::process::Command::new("kill")
                    .arg("-9")
                    .arg(format!("-{}", killed.pid))
                    .spawn();
            }
        }
        TaskKind::Subagent | TaskKind::Heartbeat => {
            if let Some(a) = killed.abort {
                a.abort();
            }
        }
    }

    Ok(Some(killed.completion))
}
