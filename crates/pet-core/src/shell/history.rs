use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

use crate::logging::log_dir;
use crate::shell::output::build_shell_result;
use crate::shell::task::{ShellTask, TaskKind, TaskStatus};
#[cfg(test)]
use crate::shell::task::BashProcess;

/// How many finished background tasks are kept on disk (and in the panel).
const TASK_HISTORY_LIMIT: usize = 200;

/// The on-disk shape of a task row (`logs/tasks.json`).
#[derive(Clone, Serialize, Deserialize)]
struct PersistedTask {
    task_id: String,
    kind: String,
    label: String,
    input: String,
    session_id: String,
    status: String,
    return_code: Option<i32>,
    started_at: String,
    finished_at: Option<String>,
    pid: u32,
    stdout_path: String,
    stderr_path: String,
    stdout: String,
    stderr: String,
    result: Option<String>,
    truncated: bool,
}


fn parse_local_time(s: &str) -> Option<DateTime<Local>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.with_timezone(&Local))
}

fn task_sort_key(task: &ShellTask) -> DateTime<Local> {
    task.finished_at.unwrap_or(task.started_at)
}

fn snapshot_task(task_id: &str, task: &ShellTask) -> PersistedTask {
    // One projection (ShellResult) feeds the disk format; `finished_at` and
    // `result` are the only fields it does not carry.
    let r = build_shell_result(task_id, task);
    PersistedTask {
        task_id: r.task_id,
        kind: r.kind.as_str().to_string(),
        label: r.label,
        input: r.input,
        session_id: r.session_id,
        status: r.status,
        return_code: r.return_code,
        started_at: r.started_at.to_rfc3339(),
        finished_at: task.finished_at.map(|t| t.to_rfc3339()),
        pid: r.pid,
        stdout_path: r.stdout_path,
        stderr_path: r.stderr_path,
        stdout: r.stdout,
        stderr: r.stderr,
        result: task.result.clone(),
        truncated: r.truncated,
    }
}

/// Clone the background tasks that belong in history (newest first, capped) so
/// the expensive snapshot+write can run WITHOUT holding the store lock. Call this
/// inside the lock; pass the result to `write_task_history` after releasing it.
pub(crate) fn collect_history_tasks(map: &HashMap<String, ShellTask>) -> Vec<(String, ShellTask)> {
    // Only background tasks belong in the panel/history; foreground tasks that
    // finished inline (e.g. a bash a sub-agent ran synchronously) are excluded.
    let mut tasks: Vec<(&String, &ShellTask)> =
        map.iter().filter(|(_, task)| task.backgrounded).collect();
    tasks.sort_by_key(|(_, task)| std::cmp::Reverse(task_sort_key(task)));
    tasks
        .into_iter()
        .take(TASK_HISTORY_LIMIT)
        .map(|(id, task)| (id.clone(), task.clone()))
        .collect()
}

/// Serialize and write the task history. MUST be called OUTSIDE the store lock —
/// for bash tasks `snapshot_task` reads each task's stdout/stderr files, so doing
/// this under the lock would block every other task-store access on disk I/O.
pub(crate) fn write_task_history(tasks: &[(String, ShellTask)]) {
    let rows: Vec<PersistedTask> = tasks
        .iter()
        .map(|(id, task)| snapshot_task(id, task))
        .collect();

    // A failed history write loses the panel's list after a restart but never
    // blocks the task itself, so it is reported instead of propagated.
    if let Err(e) = crate::store::write_json(&log_dir().join("tasks.json"), &rows) {
        eprintln!("{e}");
    }
}

fn shell_task_from_persisted(row: PersistedTask) -> Option<(String, ShellTask)> {
    let kind = TaskKind::from_str(&row.kind)?;
    let started_at = parse_local_time(&row.started_at).unwrap_or_else(Local::now);
    let status = TaskStatus::Finished;
    let return_code = row.return_code;
    let mut finished_at = row.finished_at.as_deref().and_then(parse_local_time);
    let mut result = row.result.clone();

    // A task restored after app restart cannot be controlled or observed live.
    // Keep its captured content, but do not show an unkillable "running" row.
    if row.status == "running" {
        finished_at = Some(Local::now());
        if matches!(kind, TaskKind::Subagent | TaskKind::Heartbeat) && result.is_none() {
            result = Some("（应用已重启，无法继续跟踪该任务）".to_string());
        }
    }

    let stdout =
        if row.stdout.is_empty() && matches!(kind, TaskKind::Subagent | TaskKind::Heartbeat) {
            result.clone()
        } else {
            Some(row.stdout)
        };

    let task = ShellTask {
        kind,
        label: row.label,
        input: row.input,
        session_id: row.session_id,
        status,
        return_code,
        // Only backgrounded tasks are ever persisted, so any restored row was one.
        backgrounded: true,
        started_at,
        finished_at,
        pid: 0,
        stdout_path: PathBuf::from(row.stdout_path),
        stderr_path: PathBuf::from(row.stderr_path),
        persisted_stdout: stdout,
        persisted_stderr: Some(row.stderr),
        persisted_truncated: row.truncated,
        result,
        abort: None,
        stdin_sender: None,
        // Restored rows are always Finished (see above) — nothing left to notify.
        notifies: false,
    };
    Some((row.task_id, task))
}

pub(super) fn load_persisted_tasks() -> HashMap<String, ShellTask> {
    let path = log_dir().join("tasks.json");
    let rows = match crate::store::read_json::<Vec<PersistedTask>>(&path) {
        Ok(Some(rows)) => rows,
        // First run: nothing persisted yet.
        Ok(None) => return HashMap::new(),
        // Damaged history is reported and left on disk for inspection; the
        // panel starts empty rather than refusing to start.
        Err(e) => {
            eprintln!("{e}");
            return HashMap::new();
        }
    };
    let mut map: HashMap<String, ShellTask> = rows
        .into_iter()
        .filter_map(shell_task_from_persisted)
        .collect();
    cleanup_old_tasks(&mut map);
    write_task_history(&collect_history_tasks(&map));
    map
}

pub(super) fn cleanup_old_tasks(map: &mut HashMap<String, ShellTask>) {
    // Foreground tasks that finished inline aren't shown or persisted — they only
    // lingered to keep their output files readable for the rest of the turn. Drop
    // them eagerly so they neither leak files nor crowd the background history.
    let foreground: Vec<String> = map
        .iter()
        .filter(|(_, t)| t.status == TaskStatus::Finished && !t.backgrounded)
        .map(|(id, _)| id.clone())
        .collect();

    // Among the background tasks, keep only the newest `TASK_HISTORY_LIMIT`.
    let mut finished: Vec<(String, DateTime<Local>)> = map
        .iter()
        .filter(|(_, t)| t.status == TaskStatus::Finished && t.backgrounded)
        .map(|(id, t)| (id.clone(), task_sort_key(t)))
        .collect();
    finished.sort_by_key(|(_, t)| std::cmp::Reverse(*t));

    let to_remove = foreground.into_iter().chain(
        finished
            .into_iter()
            .skip(TASK_HISTORY_LIMIT)
            .map(|(id, _)| id),
    );
    for id in to_remove {
        if let Some(task) = map.remove(&id) {
            let _ = std::fs::remove_file(&task.stdout_path);
            let _ = std::fs::remove_file(&task.stderr_path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleanup_drops_foreground_finished_keeps_backgrounded() {
        let mut map = HashMap::new();
        // A bash a sub-agent ran synchronously: finished inline, never backgrounded.
        let mut fg = ShellTask::new_bash(
            BashProcess {
                pid: 0,
                stdout_path: PathBuf::new(),
                stderr_path: PathBuf::new(),
                stdin_sender: None,
            },
            Local::now(),
            "date".to_string(),
            "sess".to_string(),
            "date".to_string(),
            false,
            true,
        );
        fg.mark_finished(Some(0));
        map.insert("fg".to_string(), fg);

        // A real background task that finished.
        let mut bg = ShellTask::new_background(
            TaskKind::Subagent,
            "work".to_string(),
            "sess".to_string(),
            Local::now(),
            "do work".to_string(),
            true,
            true,
        );
        bg.mark_finished(Some(0));
        map.insert("bg".to_string(), bg);

        cleanup_old_tasks(&mut map);

        // The foreground task is dropped; the background one survives.
        assert!(!map.contains_key("fg"));
        assert!(map.contains_key("bg"));
    }
}
