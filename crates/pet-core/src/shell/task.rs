use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Local};

use crate::shell::history::{
    cleanup_old_tasks, collect_history_tasks, load_persisted_tasks, write_task_history,
};
use crate::shell::notify::TaskCompletion;
use crate::shell::output::ShellResult;
use crate::shell::TaskListItem;

// --- Types ---

#[derive(Clone, PartialEq)]
pub(super) enum TaskStatus {
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
    /// A scheduled heartbeat run (see `heartbeat.rs` scheduler). Like a
    /// sub-agent it produces a single result string and is tracked so the panel
    /// can show it.
    Heartbeat,
}

impl TaskKind {
    pub(super) fn as_str(&self) -> &'static str {
        match self {
            TaskKind::Bash => "bash",
            TaskKind::Subagent => "subagent",
            TaskKind::Heartbeat => "heartbeat",
        }
    }

    pub(super) fn from_str(s: &str) -> Option<Self> {
        match s {
            "bash" => Some(TaskKind::Bash),
            "subagent" => Some(TaskKind::Subagent),
            "heartbeat" => Some(TaskKind::Heartbeat),
            _ => None,
        }
    }
}

#[derive(Clone)]
pub struct ShellTask {
    pub(super) kind: TaskKind,
    pub(super) label: String,
    // Full input: the bash command, or the sub-agent prompt. Shown in the task
    // detail view (the `label` is only the short description/first line).
    pub(super) input: String,
    pub(super) session_id: String,
    pub(super) status: TaskStatus,
    pub(super) return_code: Option<i32>,
    // True only once the task actually ran in the background — either started
    // there (explicit `run_in_background`) or auto-converted on a foreground
    // timeout. Tasks that finish inline within the foreground timeout stay false
    // and are hidden from the "background tasks" panel (and not persisted): a
    // bash a sub-agent/heartbeat ran synchronously isn't a background task. See
    // `ShellStore::list` / `ShellStore::register` / `cleanup_old_tasks`.
    pub(super) backgrounded: bool,
    pub(super) started_at: DateTime<Local>,
    pub(super) finished_at: Option<DateTime<Local>>,
    // Bash-only: live output is read from these files (dummy for sub-agents).
    pub(super) pid: u32,
    pub(super) stdout_path: PathBuf,
    pub(super) stderr_path: PathBuf,
    pub(super) persisted_stdout: Option<String>,
    pub(super) persisted_stderr: Option<String>,
    pub(super) persisted_truncated: bool,
    // Sub-agent final text (None for bash; bash output lives in its files).
    pub(super) result: Option<String>,
    // Cancels a running sub-agent (its work future). Bash is killed by pid, so
    // this stays None for bash tasks. See `kill_task`.
    pub(super) abort: Option<tokio::task::AbortHandle>,
    // REPL bash only: channel to forward lines into the process's stdin.
    pub(crate) stdin_sender: Option<tokio::sync::mpsc::Sender<String>>,
    // Whether a TaskNotifier was present at registration, i.e. finishing this
    // task will fire a completion notification. Sub-agents' nested tasks have
    // none (`ToolContext::child` drops the notifier), and neither do heartbeat
    // runs. `pending_notify_count` uses this to know what is worth waiting for.
    pub(super) notifies: bool,
}

/// OS process behind a bash task: its pid, the files its output goes to, and
/// (REPL only) the stdin channel.
pub(crate) struct BashProcess {
    pub pid: u32,
    pub stdout_path: PathBuf,
    pub stderr_path: PathBuf,
    pub stdin_sender: Option<tokio::sync::mpsc::Sender<String>>,
}

impl ShellTask {
    pub(crate) fn new_bash(
        process: BashProcess,
        started_at: DateTime<Local>,
        label: String,
        session_id: String,
        input: String,
        backgrounded: bool,
        notifies: bool,
    ) -> Self {
        let BashProcess {
            pid,
            stdout_path,
            stderr_path,
            stdin_sender,
        } = process;
        Self {
            kind: TaskKind::Bash,
            label,
            input,
            session_id,
            status: TaskStatus::Running,
            return_code: None,
            backgrounded,
            started_at,
            finished_at: None,
            pid,
            stdout_path,
            stderr_path,
            persisted_stdout: None,
            persisted_stderr: None,
            persisted_truncated: false,
            result: None,
            abort: None,
            stdin_sender,
            notifies,
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
        backgrounded: bool,
        notifies: bool,
    ) -> Self {
        Self {
            kind,
            label,
            input,
            session_id,
            status: TaskStatus::Running,
            return_code: None,
            backgrounded,
            started_at,
            finished_at: None,
            pid: 0,
            stdout_path: PathBuf::new(),
            stderr_path: PathBuf::new(),
            persisted_stdout: None,
            persisted_stderr: None,
            persisted_truncated: false,
            result: None,
            abort: None,
            stdin_sender: None,
            notifies,
        }
    }

    /// Record completion without firing a notification — used by the foreground
    /// path, where the caller already gets the result inline in the same turn.
    pub fn mark_finished(&mut self, code: Option<i32>) {
        self.status = TaskStatus::Finished;
        self.return_code = code;
        self.finished_at = Some(Local::now());
    }

    /// Promote a foreground task to a background one — called when it exceeds the
    /// foreground timeout and converts to a notified background task, so it now
    /// belongs in the background-task panel.
    pub fn mark_backgrounded(&mut self) {
        self.backgrounded = true;
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

/// What a killed task needs for the actual kill (process group / future abort).
pub(crate) struct KilledTask {
    pub kind: TaskKind,
    pub pid: u32,
    pub abort: Option<tokio::task::AbortHandle>,
    pub completion: TaskCompletion,
}

/// The shared background-task table. Fields are private so the lock discipline
/// ("never write history under the lock") is enforced here instead of at every
/// call site: each method below snapshots under the lock and persists outside it.
#[derive(Clone)]
pub struct ShellStore(pub(super) Arc<Mutex<HashMap<String, ShellTask>>>);

impl Default for ShellStore {
    fn default() -> Self {
        Self::new()
    }
}

impl ShellStore {
    pub fn new() -> Self {
        Self(Arc::new(Mutex::new(HashMap::new())))
    }

    /// Restore the tasks persisted by the previous run.
    pub fn load_persisted() -> Self {
        Self(Arc::new(Mutex::new(load_persisted_tasks())))
    }

    /// Register a task, pruning old finished ones first. History is written after
    /// the lock is released.
    pub fn register(&self, task_id: String, task: ShellTask) {
        let history = {
            let mut map = self.0.lock().unwrap();
            cleanup_old_tasks(&mut map);
            map.insert(task_id, task);
            collect_history_tasks(&map)
        };
        write_task_history(&history);
    }

    /// Mutate one task under the lock, then persist history outside it. Returns
    /// false when the task no longer exists.
    pub fn update(&self, task_id: &str, f: impl FnOnce(&mut ShellTask)) -> bool {
        let history = {
            let mut map = self.0.lock().unwrap();
            match map.get_mut(task_id) {
                Some(task) => f(task),
                None => return false,
            }
            collect_history_tasks(&map)
        };
        write_task_history(&history);
        true
    }

    pub fn snapshot(&self, task_id: &str) -> Option<ShellTask> {
        self.0.lock().unwrap().get(task_id).cloned()
    }

    /// How many tasks are still due to fire a completion notification. See
    /// `TaskStatus` / `ShellStore::pending_notify_count`.
    pub fn pending_notify_count(&self) -> usize {
        let map = self.0.lock().unwrap();
        map.values()
            .filter(|t| {
                t.status == TaskStatus::Running
                    && t.backgrounded
                    && t.notifies
                    && t.stdin_sender.is_none()
            })
            .count()
    }

    /// All tracked tasks shown in the Panel "任务" tab.
    pub fn list(&self) -> Vec<TaskListItem> {
        let (items, history) = {
            let mut map = self.0.lock().unwrap();
            let before = map.len();
            cleanup_old_tasks(&mut map);
            let history = (map.len() != before).then(|| collect_history_tasks(&map));
            let items = map
                .iter()
                // Only actually-backgrounded tasks belong in the panel; a
                // foreground bash a sub-agent/heartbeat ran inline is not one.
                .filter(|(_, t)| t.backgrounded)
                .map(|(id, t)| TaskListItem::from(&ShellResult::metadata(id, t)))
                .collect();
            (items, history)
        };
        if let Some(history) = history {
            write_task_history(&history);
        }
        items
    }

    /// Mark a task cancelled and snapshot what the caller must kill. History is
    /// written after the lock is released. Returns `Ok(None)` when it has
    /// already finished.
    pub(crate) fn kill_snapshot(&self, task_id: &str) -> Result<Option<KilledTask>, String> {
        let (kind, pid, abort, completion, history) = {
            let mut map = self.0.lock().unwrap();
            let task = map
                .get_mut(task_id)
                .ok_or_else(|| format!("Task not found: {}", task_id))?;
            if task.status == TaskStatus::Finished {
                return Ok(None); // already done — nothing to kill
            }
            let result = "（该后台任务已被用户手动终止）".to_string();
            task.status = TaskStatus::Finished;
            task.finished_at = Some(Local::now());
            task.return_code = Some(-1);
            task.result = Some(result.clone());
            let completion = TaskCompletion {
                session_id: task.session_id.clone(),
                task_id: task_id.to_string(),
                kind: task.kind.as_str().to_string(),
                label: task.label.clone(),
                result,
            };
            let ret = (task.kind, task.pid, task.abort.take(), completion);
            // `ret` ended the `task` mutable borrow above, so the map is free to read.
            let history = collect_history_tasks(&map);
            (ret.0, ret.1, ret.2, ret.3, history)
        };
        write_task_history(&history);
        Ok(Some(KilledTask {
            kind,
            pid,
            abort,
            completion,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// one-shot 退出条件：只有「会通知、能自己结束」的任务算未完成——没 notifier
    /// 的（子代理嵌套任务）、REPL（永不退出）、已完成的都不算，否则 -p 要么早退
    /// 丢工作、要么永远等不到静默。
    #[test]
    fn pending_notify_count_only_counts_awaitable_notifying_tasks() {
        let store = ShellStore::new();
        let mk = |backgrounded, notifies| {
            ShellTask::new_background(
                TaskKind::Subagent,
                "work".to_string(),
                "sess".to_string(),
                Local::now(),
                "do work".to_string(),
                backgrounded,
                notifies,
            )
        };
        {
            let mut map = store.0.lock().unwrap();
            // 计入：真后台 + 有 notifier + 在跑
            map.insert("counted".to_string(), mk(true, true));
            // 不计：子代理嵌套任务（ctx.child 丢掉了 notifier）
            map.insert("no-notifier".to_string(), mk(true, false));
            // 不计：已完成
            let mut done = mk(true, true);
            done.mark_finished(Some(0));
            map.insert("done".to_string(), done);
            // 不计：REPL（stdin 通道在手，进程不会自己退出）
            let mut repl = ShellTask::new_bash(
                BashProcess {
                    pid: 1,
                    stdout_path: PathBuf::new(),
                    stderr_path: PathBuf::new(),
                    stdin_sender: Some(tokio::sync::mpsc::channel(1).0),
                },
                Local::now(),
                "python3".to_string(),
                "sess".to_string(),
                "python3".to_string(),
                true,
                true,
            );
            repl.status = TaskStatus::Running;
            map.insert("repl".to_string(), repl);
        }
        assert_eq!(store.pending_notify_count(), 1);

        // 唯一计入的那个完成后归零——one-shot 可以退出了。
        store
            .0
            .lock()
            .unwrap()
            .get_mut("counted")
            .unwrap()
            .mark_finished(Some(0));
        assert_eq!(store.pending_notify_count(), 0);
    }
}
