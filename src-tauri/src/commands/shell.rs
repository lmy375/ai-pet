use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{DateTime, Local};
use serde::Serialize;
use tauri::State;

const MAX_OUTPUT: usize = 4096;
const TIMEOUT_SECS: u64 = 30;
const SHELL_DIR: &str = "/tmp/pet/shell";

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
    pub fn new(pid: u32, stdout_path: PathBuf, stderr_path: PathBuf, started_at: DateTime<Local>) -> Self {
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

fn read_with_truncation(path: &PathBuf) -> (String, bool) {
    let content = std::fs::read_to_string(path).unwrap_or_default();
    if content.len() <= MAX_OUTPUT {
        (content, false)
    } else {
        let tail = &content[content.len() - MAX_OUTPUT..];
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

fn cleanup_old_tasks(map: &mut HashMap<String, ShellTask>) {
    let cutoff = Local::now() - chrono::Duration::hours(1);
    let to_remove: Vec<String> = map
        .iter()
        .filter(|(_, t)| {
            t.status == TaskStatus::Finished
                && t.finished_at.map_or(false, |f| f < cutoff)
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
pub async fn execute_shell(
    command: String,
    store: State<'_, ShellStore>,
) -> Result<ShellResult, String> {
    let task_id = uuid::Uuid::new_v4().to_string();

    // Ensure output directory exists
    std::fs::create_dir_all(SHELL_DIR).map_err(|e| format!("Failed to create {}: {}", SHELL_DIR, e))?;

    let stdout_path = PathBuf::from(format!("{}/{}.stdout", SHELL_DIR, task_id));
    let stderr_path = PathBuf::from(format!("{}/{}.stderr", SHELL_DIR, task_id));

    // Open files for process output redirection
    let stdout_file = std::fs::File::create(&stdout_path)
        .map_err(|e| format!("Failed to create stdout file: {}", e))?;
    let stderr_file = std::fs::File::create(&stderr_path)
        .map_err(|e| format!("Failed to create stderr file: {}", e))?;

    // Spawn the process
    let mut child = tokio::process::Command::new("bash")
        .arg("-c")
        .arg(&command)
        .stdout(Stdio::from(stdout_file))
        .stderr(Stdio::from(stderr_file))
        .spawn()
        .map_err(|e| format!("Failed to spawn process: {}", e))?;

    let pid = child.id().unwrap_or(0);
    let now = Local::now();
    let task = ShellTask::new(pid, stdout_path.clone(), stderr_path.clone(), now);

    // Insert task and cleanup old ones
    {
        let mut map = store.0.lock().unwrap();
        cleanup_old_tasks(&mut map);
        map.insert(task_id.clone(), task);
    }

    // Wait with timeout
    let store_arc = store.0.clone();
    let tid = task_id.clone();

    match tokio::time::timeout(Duration::from_secs(TIMEOUT_SECS), child.wait()).await {
        Ok(Ok(exit_status)) => {
            // Completed within timeout
            let mut map = store_arc.lock().unwrap();
            if let Some(t) = map.get_mut(&tid) {
                t.mark_finished(exit_status.code());
            }
            let result = build_shell_result(&tid, map.get(&tid).unwrap());
            Ok(result)
        }
        Ok(Err(e)) => {
            // Process error
            let mut map = store_arc.lock().unwrap();
            if let Some(t) = map.get_mut(&tid) {
                t.mark_finished(Some(-1));
            }
            Err(format!("Process error: {}", e))
        }
        Err(_) => {
            // Timeout — spawn background waiter
            let store_bg = store_arc.clone();
            let tid_bg = tid.clone();
            tokio::spawn(async move {
                let exit = child.wait().await;
                let mut map = store_bg.lock().unwrap();
                if let Some(t) = map.get_mut(&tid_bg) {
                    t.mark_finished(exit.ok().and_then(|s| s.code()));
                }
            });

            // Return current state
            let map = store_arc.lock().unwrap();
            let result = build_shell_result(&tid, map.get(&tid).unwrap());
            Ok(result)
        }
    }
}

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
