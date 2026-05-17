use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Local};
use serde::Serialize;
use tauri::State;

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

pub(crate) struct ShellTask {
    pid: u32,
    status: TaskStatus,
    return_code: Option<i32>,
    stdout_path: PathBuf,
    stderr_path: PathBuf,
    started_at: DateTime<Local>,
    finished_at: Option<DateTime<Local>>,
}

impl ShellTask {
    pub fn new(
        pid: u32,
        stdout_path: PathBuf,
        stderr_path: PathBuf,
        started_at: DateTime<Local>,
    ) -> Self {
        Self {
            pid,
            status: TaskStatus::Running,
            return_code: None,
            stdout_path,
            stderr_path,
            started_at,
            finished_at: None,
        }
    }

    pub fn mark_finished(&mut self, code: Option<i32>) {
        self.status = TaskStatus::Finished;
        self.return_code = code;
        self.finished_at = Some(Local::now());
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }

    pub fn stdout_path(&self) -> &PathBuf {
        &self.stdout_path
    }

    pub fn stderr_path(&self) -> &PathBuf {
        &self.stderr_path
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
    stdout: String,
    stderr: String,
    stdout_path: String,
    stderr_path: String,
    truncated: bool,
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

fn build_shell_result(task_id: &str, task: &ShellTask) -> ShellResult {
    let (stdout, stdout_trunc) = read_with_truncation(&task.stdout_path);
    let (stderr, stderr_trunc) = read_with_truncation(&task.stderr_path);

    let elapsed = task
        .finished_at
        .unwrap_or_else(Local::now)
        .signed_duration_since(task.started_at);
    let execution_time_ms = elapsed.num_milliseconds().max(0) as u64;

    ShellResult {
        task_id: task_id.to_string(),
        pid: task.pid,
        status: match task.status {
            TaskStatus::Running => "running".to_string(),
            TaskStatus::Finished => "finished".to_string(),
        },
        return_code: task.return_code,
        execution_time_ms,
        stdout,
        stderr,
        stdout_path: task.stdout_path.to_string_lossy().to_string(),
        stderr_path: task.stderr_path.to_string_lossy().to_string(),
        truncated: stdout_trunc || stderr_trunc,
    }
}

pub fn cleanup_old_tasks(map: &mut HashMap<String, ShellTask>) {
    let cutoff = Local::now() - chrono::Duration::hours(1);
    let to_remove: Vec<String> = map
        .iter()
        .filter(|(_, t)| {
            t.status == TaskStatus::Finished && t.finished_at.is_some_and(|f| f < cutoff)
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

// --- Commands ---

#[tauri::command]
pub fn check_shell_status(
    task_id: String,
    store: State<'_, ShellStore>,
) -> Result<ShellResult, String> {
    let map = store.0.lock().unwrap();
    match map.get(&task_id) {
        Some(task) => Ok(build_shell_result(&task_id, task)),
        None => Err(format!("Task not found: {}", task_id)),
    }
}

/// PanelDebug shell exit code 分布 chip 用：扫 ShellStore 内当前缓存
/// 的所有 shell task（窗口 ≤ 1 小时 — cleanup_old_tasks 1h cutoff）
/// + 按 return_code 分桶。`success` = code 0；`failure` = code 非
/// 0；`running_or_unknown` = code None（仍 running / 被 kill / 写
/// return_code 之前 panic 等）。让 owner 一眼看 LLM 用 shell tool
/// 的失败率。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellExitCodeStats {
    pub success: u32,
    pub failure: u32,
    pub running_or_unknown: u32,
    pub total: u32,
}

#[tauri::command]
pub fn get_shell_exit_code_stats(
    store: State<'_, ShellStore>,
) -> ShellExitCodeStats {
    let map = store.0.lock().unwrap();
    let mut success = 0u32;
    let mut failure = 0u32;
    let mut running_or_unknown = 0u32;
    for task in map.values() {
        match task.return_code {
            Some(0) => success += 1,
            Some(_) => failure += 1,
            None => running_or_unknown += 1,
        }
    }
    let total = success + failure + running_or_unknown;
    ShellExitCodeStats {
        success,
        failure,
        running_or_unknown,
        total,
    }
}

/// PanelDebug「🔄 重置 ⚙️ shell stats」按钮触发：清空 ShellStore
/// 内**已完成**任务（finished — 含 success / failure）+ 删对应
/// stdout/stderr 文件。**running** 任务保留（仍在执行的子进程
/// 状态需要被 check_shell_status 继续观察；强删除会让正在跑的
/// shell tool 失联）。
///
/// 与 cleanup_old_tasks（1h cutoff）共享 finished + 删文件 pattern
/// 但不看时间 — 立即清。debug 场景「从这里开始重测」：让 ⚙️
/// chip 计数归零，新 shell call 进 clean 累计。
///
/// 返回清掉的 task 数让前端 toast 显反馈（"已清 N 条"）。
#[tauri::command]
pub fn reset_shell_store(store: State<'_, ShellStore>) -> u32 {
    let mut map = store.0.lock().unwrap();
    let to_remove: Vec<String> = map
        .iter()
        .filter(|(_, t)| t.status == TaskStatus::Finished)
        .map(|(id, _)| id.clone())
        .collect();
    let removed = to_remove.len() as u32;
    for id in to_remove {
        if let Some(task) = map.remove(&id) {
            let _ = std::fs::remove_file(&task.stdout_path);
            let _ = std::fs::remove_file(&task.stderr_path);
        }
    }
    removed
}
