use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Local};
use serde::Serialize;
use tauri::State;

use crate::tools::ToolContext;

pub const MAX_OUTPUT: usize = 32768;
pub const DEFAULT_TIMEOUT_MS: u64 = 120_000;
pub const MAX_TIMEOUT_MS: u64 = 600_000;
pub const SHELL_DIR: &str = "/tmp/pet/shell";

// --- Types ---

#[derive(Clone, PartialEq)]
enum TaskStatus {
    Running,
    Finished,
}

/// What kind of work a background task represents. Both bash commands and
/// spawned sub-agents are tracked in the same store so they share status
/// queries, cleanup and completion notifications.
#[derive(Clone, Copy, PartialEq)]
pub enum TaskKind {
    Bash,
    Subagent,
    /// A scheduled heartbeat run (see `lib.rs` scheduler). Like a sub-agent it
    /// produces a single result string and is tracked so the panel can show it.
    Heartbeat,
}

impl TaskKind {
    fn as_str(&self) -> &'static str {
        match self {
            TaskKind::Bash => "bash",
            TaskKind::Subagent => "subagent",
            TaskKind::Heartbeat => "heartbeat",
        }
    }
}

#[derive(Clone)]
pub(crate) struct ShellTask {
    kind: TaskKind,
    label: String,
    // Full input: the bash command, or the sub-agent prompt. Shown in the task
    // detail view (the `label` is only the short description/first line).
    input: String,
    session_id: String,
    status: TaskStatus,
    return_code: Option<i32>,
    started_at: DateTime<Local>,
    finished_at: Option<DateTime<Local>>,
    // Bash-only: live output is read from these files (dummy for sub-agents).
    pid: u32,
    stdout_path: PathBuf,
    stderr_path: PathBuf,
    // Sub-agent final text (None for bash; bash output lives in its files).
    result: Option<String>,
    // Cancels a running sub-agent (its work future). Bash is killed by pid, so
    // this stays None for bash tasks. See `kill_task`.
    abort: Option<tokio::task::AbortHandle>,
}

impl ShellTask {
    pub fn new_bash(
        pid: u32,
        stdout_path: PathBuf,
        stderr_path: PathBuf,
        started_at: DateTime<Local>,
        label: String,
        session_id: String,
        input: String,
    ) -> Self {
        Self {
            kind: TaskKind::Bash,
            label,
            input,
            session_id,
            status: TaskStatus::Running,
            return_code: None,
            started_at,
            finished_at: None,
            pid,
            stdout_path,
            stderr_path,
            result: None,
            abort: None,
        }
    }

    /// A file-less task whose output is a single result string (sub-agents and
    /// other string-result work). Bash uses `new_bash` instead — it needs a pid
    /// and live output files.
    pub fn new_background(
        kind: TaskKind,
        label: String,
        session_id: String,
        started_at: DateTime<Local>,
        input: String,
    ) -> Self {
        Self {
            kind,
            label,
            input,
            session_id,
            status: TaskStatus::Running,
            return_code: None,
            started_at,
            finished_at: None,
            pid: 0,
            stdout_path: PathBuf::new(),
            stderr_path: PathBuf::new(),
            result: None,
            abort: None,
        }
    }

    /// Record completion without firing a notification — used by the foreground
    /// path, where the caller already gets the result inline in the same turn.
    pub fn mark_finished(&mut self, code: Option<i32>) {
        self.status = TaskStatus::Finished;
        self.return_code = code;
        self.finished_at = Some(Local::now());
    }

    pub fn status_info(&self) -> (&str, Option<i32>, u64) {
        let status = match self.status {
            TaskStatus::Running => "running",
            TaskStatus::Finished => "finished",
        };
        let elapsed = self
            .finished_at
            .unwrap_or_else(Local::now)
            .signed_duration_since(self.started_at)
            .num_milliseconds()
            .max(0) as u64;
        (status, self.return_code, elapsed)
    }
}

#[derive(Clone)]
pub struct ShellStore(pub Arc<Mutex<HashMap<String, ShellTask>>>);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellResult {
    task_id: String,
    pid: u32,
    status: String,
    return_code: Option<i32>,
    execution_time_ms: u64,
    // The bash command or sub-agent prompt that produced this task.
    input: String,
    stdout: String,
    stderr: String,
    stdout_path: String,
    stderr_path: String,
    truncated: bool,
}

// --- Background-task completion notifications ---

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

// --- Shared helper ---

/// Find the nearest valid UTF-8 char boundary at or after `pos`.
pub fn ceil_char_boundary(s: &str, pos: usize) -> usize {
    let mut i = pos;
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

pub fn read_with_truncation(path: &PathBuf) -> (String, bool) {
    let content = std::fs::read_to_string(path).unwrap_or_default();
    if content.len() <= MAX_OUTPUT {
        (content, false)
    } else {
        let start = ceil_char_boundary(&content, content.len() - MAX_OUTPUT);
        let tail = &content[start..];
        let truncated = format!(
            "--- truncated ({} bytes total), full output: {} ---\n{}",
            content.len(),
            path.display(),
            tail
        );
        (truncated, true)
    }
}

pub(crate) fn build_shell_result(task_id: &str, task: &ShellTask) -> ShellResult {
    let (status, return_code, execution_time_ms) = task.status_info();

    // Sub-agents have no output files — their final text lives in `result`.
    let (stdout, stderr, truncated) = match task.kind {
        TaskKind::Bash => {
            let (out, out_trunc) = read_with_truncation(&task.stdout_path);
            let (err, err_trunc) = read_with_truncation(&task.stderr_path);
            (out, err, out_trunc || err_trunc)
        }
        TaskKind::Subagent | TaskKind::Heartbeat => {
            (task.result.clone().unwrap_or_default(), String::new(), false)
        }
    };

    ShellResult {
        task_id: task_id.to_string(),
        pid: task.pid,
        status: status.to_string(),
        return_code,
        execution_time_ms,
        input: task.input.clone(),
        stdout,
        stderr,
        stdout_path: task.stdout_path.to_string_lossy().to_string(),
        stderr_path: task.stderr_path.to_string_lossy().to_string(),
        truncated,
    }
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

/// Record completion and fire the notification. Used by background waiters (NOT
/// the foreground path, which returns its result inline without notifying).
pub fn mark_finished_and_notify(
    store: &Arc<Mutex<HashMap<String, ShellTask>>>,
    notifier: &Option<Arc<dyn TaskNotifier>>,
    task_id: &str,
    return_code: Option<i32>,
    result: Option<String>,
) {
    // Update under the lock, then snapshot the task so the notification string
    // (which for bash reads stdout/stderr files) is built AFTER the lock is
    // released — never hold the store mutex across file I/O.
    let snapshot = {
        let mut map = store.lock().unwrap();
        let task = match map.get_mut(task_id) {
            Some(t) => t,
            None => return,
        };
        // Already finished — e.g. killed by the user via `kill_task`, which marks
        // the task and fires its own notification before the process/future
        // actually unwinds into this waiter. Skip to avoid a duplicate/garbled one.
        if task.status == TaskStatus::Finished {
            return;
        }
        task.status = TaskStatus::Finished;
        task.return_code = return_code;
        task.finished_at = Some(Local::now());
        if result.is_some() {
            task.result = result;
        }
        task.clone()
    };
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

pub fn cleanup_old_tasks(map: &mut HashMap<String, ShellTask>) {
    let cutoff = Local::now() - chrono::Duration::hours(1);
    let to_remove: Vec<String> = map
        .iter()
        .filter(|(_, t)| {
            t.status == TaskStatus::Finished && t.finished_at.map_or(false, |f| f < cutoff)
        })
        .map(|(id, _)| id.clone())
        .collect();

    for id in to_remove {
        if let Some(task) = map.remove(&id) {
            let _ = std::fs::remove_file(&task.stdout_path);
            let _ = std::fs::remove_file(&task.stderr_path);
        }
    }
}

/// Run `work` (a future producing the final result string) with the same
/// explicit-background + timeout-auto-background semantics as bash. Registers a
/// task in the shared store; on background completion it stores the result and
/// fires the notification. Returns either the result inline (finished in time)
/// or a `{task_id, status:"running"}` JSON (backgrounded).
///
/// Bash keeps its own process-based logic (it needs pid + live output files);
/// this is the reusable skeleton for sub-agents and other string-result tasks.
pub async fn run_or_background<F>(
    ctx: &ToolContext,
    kind: TaskKind,
    label: String,
    input: String,
    timeout_ms: u64,
    run_in_background: bool,
    work: F,
) -> String
where
    F: Future<Output = (Option<i32>, String)> + Send + 'static,
{
    let task_id = uuid::Uuid::new_v4().to_string();
    let started_at = Local::now();
    {
        let mut map = ctx.shell_store.0.lock().unwrap();
        cleanup_old_tasks(&mut map);
        map.insert(
            task_id.clone(),
            ShellTask::new_background(kind, label, ctx.session_id.clone(), started_at, input),
        );
    }

    let mut handle = tokio::spawn(work);
    // Record a cancel handle so `kill_task` can abort this work future (the only
    // way to stop a sub-agent, which has no OS process to signal).
    {
        let mut map = ctx.shell_store.0.lock().unwrap();
        if let Some(t) = map.get_mut(&task_id) {
            t.abort = Some(handle.abort_handle());
        }
    }
    let store = ctx.shell_store.0.clone();
    let notifier = ctx.notifier.clone();

    // A join error means the work panicked: report it as a failed task.
    let join_err =
        |e: tokio::task::JoinError| (Some(-1), format!(r#"{{"error": "task join error: {}"}}"#, e));

    if run_in_background {
        let tid = task_id.clone();
        tokio::spawn(async move {
            let (code, result) = handle.await.unwrap_or_else(join_err);
            mark_finished_and_notify(&store, &notifier, &tid, code, Some(result));
        });
        return serde_json::json!({
            "task_id": task_id,
            "status": "running",
            "message": format!("{} started in background. You will be notified when it finishes — do not poll.", kind.as_str()),
        })
        .to_string();
    }

    let dur = std::time::Duration::from_millis(timeout_ms);
    match tokio::time::timeout(dur, &mut handle).await {
        Ok(joined) => {
            let (code, result) = joined.unwrap_or_else(join_err);
            // Foreground completion: return inline, no notification.
            {
                let mut map = store.lock().unwrap();
                if let Some(t) = map.get_mut(&task_id) {
                    t.mark_finished(code);
                    t.result = Some(result.clone());
                }
            }
            result
        }
        Err(_) => {
            let tid = task_id.clone();
            tokio::spawn(async move {
                let (code, result) = handle.await.unwrap_or_else(join_err);
                mark_finished_and_notify(&store, &notifier, &tid, code, Some(result));
            });
            serde_json::json!({
                "task_id": task_id,
                "status": "running",
                "message": format!("{} still running after {}ms. You will be notified when it finishes — do not poll.", kind.as_str(), timeout_ms),
            })
            .to_string()
        }
    }
}

// --- Commands ---

#[tauri::command]
pub fn check_task_status(
    task_id: String,
    store: State<'_, ShellStore>,
) -> Result<ShellResult, String> {
    let map = store.0.lock().unwrap();
    match map.get(&task_id) {
        Some(task) => Ok(build_shell_result(&task_id, task)),
        None => Err(format!("Task not found: {}", task_id)),
    }
}

/// A single row for the Panel "任务" tab. Built from a `ShellTask` (whose fields
/// are private to this module).
#[derive(Serialize)]
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

/// List all tracked tasks (running + recently finished; `cleanup_old_tasks`
/// drops finished tasks older than an hour). The UI groups and sorts them.
#[tauri::command]
pub fn list_tasks(store: State<'_, ShellStore>) -> Vec<TaskListItem> {
    let map = store.0.lock().unwrap();
    map.iter()
        .map(|(id, t)| {
            let (status, return_code, elapsed_ms) = t.status_info();
            TaskListItem {
                task_id: id.clone(),
                kind: t.kind.as_str().to_string(),
                label: t.label.clone(),
                status: status.to_string(),
                return_code,
                elapsed_ms,
                started_at: t.started_at.to_rfc3339(),
                session_id: t.session_id.clone(),
            }
        })
        .collect()
}

/// Kill a running task and tell the pet it was cancelled.
///
/// Marks the task finished BEFORE the actual kill so the background waiter, when
/// the process dies / future aborts, sees `Finished` in `mark_finished_and_notify`
/// and skips its own (now-garbled) notification — we fire one clean notification
/// here instead.
#[tauri::command]
pub fn kill_task(
    task_id: String,
    app: tauri::AppHandle,
    store: State<'_, ShellStore>,
) -> Result<(), String> {
    let (kind, pid, abort, completion) = {
        let mut map = store.0.lock().unwrap();
        let task = map
            .get_mut(&task_id)
            .ok_or_else(|| format!("Task not found: {}", task_id))?;
        if task.status == TaskStatus::Finished {
            return Ok(()); // already done — nothing to kill
        }
        let result = "（该后台任务已被用户手动终止）".to_string();
        task.status = TaskStatus::Finished;
        task.finished_at = Some(Local::now());
        task.return_code = Some(-1);
        task.result = Some(result.clone());
        let completion = TaskCompletion {
            session_id: task.session_id.clone(),
            task_id: task_id.clone(),
            kind: task.kind.as_str().to_string(),
            label: task.label.clone(),
            result,
        };
        (task.kind, task.pid, task.abort.take(), completion)
    };

    // Stop the actual work: bash by process group (set via process_group(0) at
    // spawn), sub-agent by aborting its work future.
    match kind {
        TaskKind::Bash => {
            if pid != 0 {
                let _ = std::process::Command::new("kill")
                    .arg("-9")
                    .arg(format!("-{}", pid))
                    .spawn();
            }
        }
        TaskKind::Subagent | TaskKind::Heartbeat => {
            if let Some(a) = abort {
                a.abort();
            }
        }
    }

    // Notify the active window, reusing the same event the frontend already
    // handles (so the conversation reacts to the cancellation).
    use tauri::Emitter;
    let label = crate::commands::window::active_window_label(&app);
    if let Err(e) = app.emit_to(&label, "background-finished", completion) {
        eprintln!("failed to emit background-finished for killed task {}: {}", task_id, e);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_result_renders_subagent_from_stored_text() {
        let mut task = ShellTask::new_background(
            TaskKind::Subagent,
            "count files".to_string(),
            "sess-1".to_string(),
            Local::now(),
            "count the files in /tmp".to_string(),
        );
        task.mark_finished(Some(0));
        task.result = Some("found 42 files".to_string());

        let r = build_shell_result("t1", &task);
        assert_eq!(r.status, "finished");
        assert_eq!(r.stdout, "found 42 files");
        assert_eq!(r.stderr, "");
        assert_eq!(r.input, "count the files in /tmp");
    }
}
