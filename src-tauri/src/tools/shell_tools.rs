use crate::tools::{Tool, ToolContext};
use std::path::PathBuf;
use std::process::Stdio;

const MAX_OUTPUT: usize = 4096;
const TIMEOUT_SECS: u64 = 30;
const SHELL_DIR: &str = "/tmp/pet/shell";

// ---- Helpers ----

fn read_output(path: &PathBuf) -> (String, bool) {
    let content = std::fs::read_to_string(path).unwrap_or_default();
    if content.len() <= MAX_OUTPUT {
        (content, false)
    } else {
        let tail = &content[content.len() - MAX_OUTPUT..];
        (
            format!(
                "--- truncated ({} bytes), full: {} ---\n{}",
                content.len(),
                path.display(),
                tail
            ),
            true,
        )
    }
}

// ---- execute_shell ----

pub struct ExecuteShellTool;

impl Tool for ExecuteShellTool {
    fn name(&self) -> &str {
        "execute_shell"
    }

    fn definition(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "execute_shell",
                "description": "Execute a bash command on the user's machine. stdout/stderr are captured to files. Commands finishing within 30s return results directly; longer commands return a task_id for status checking. Returns pid for process management.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "The bash command to execute"
                        }
                    },
                    "required": ["command"]
                }
            }
        })
    }

    fn execute<'a>(
        &'a self,
        arguments: &'a str,
        ctx: &'a ToolContext,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = String> + Send + 'a>> {
        Box::pin(execute_shell_impl(arguments, ctx))
    }
}

async fn execute_shell_impl(arguments: &str, ctx: &ToolContext) -> String {
    let args: serde_json::Value = serde_json::from_str(arguments).unwrap_or_default();
    let command = args["command"].as_str().unwrap_or("").to_string();
    if command.is_empty() {
        return r#"{"error": "missing 'command' parameter"}"#.to_string();
    }

    let _ = std::fs::create_dir_all(SHELL_DIR);
    let task_id = uuid::Uuid::new_v4().to_string();
    let stdout_path = PathBuf::from(format!("{}/{}.stdout", SHELL_DIR, task_id));
    let stderr_path = PathBuf::from(format!("{}/{}.stderr", SHELL_DIR, task_id));

    let stdout_file = match std::fs::File::create(&stdout_path) {
        Ok(f) => f,
        Err(e) => return format!(r#"{{"error": "failed to create stdout file: {}"}}"#, e),
    };
    let stderr_file = match std::fs::File::create(&stderr_path) {
        Ok(f) => f,
        Err(e) => return format!(r#"{{"error": "failed to create stderr file: {}"}}"#, e),
    };

    let mut child = match tokio::process::Command::new("bash")
        .arg("-c")
        .arg(&command)
        .stdout(Stdio::from(stdout_file))
        .stderr(Stdio::from(stderr_file))
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return format!(r#"{{"error": "failed to spawn: {}"}}"#, e),
    };

    let pid = child.id().unwrap_or(0);
    let started_at = chrono::Local::now();

    // Store task
    {
        let mut map = ctx.shell_store.0.lock().unwrap();
        map.insert(
            task_id.clone(),
            crate::commands::shell::ShellTask::new(pid, stdout_path.clone(), stderr_path.clone(), started_at),
        );
    }

    ctx.log(&format!("Shell[{}] pid={} cmd={}", task_id, pid, command));

    // Wait with timeout
    match tokio::time::timeout(std::time::Duration::from_secs(TIMEOUT_SECS), child.wait()).await {
        Ok(Ok(exit_status)) => {
            {
                let mut map = ctx.shell_store.0.lock().unwrap();
                if let Some(t) = map.get_mut(&task_id) {
                    t.mark_finished(exit_status.code());
                }
            }

            let elapsed = chrono::Local::now()
                .signed_duration_since(started_at)
                .num_milliseconds()
                .max(0) as u64;
            let stdout = read_output(&stdout_path);
            let stderr = read_output(&stderr_path);

            serde_json::json!({
                "task_id": task_id,
                "pid": pid,
                "status": "finished",
                "return_code": exit_status.code(),
                "execution_time_ms": elapsed,
                "stdout": stdout.0,
                "stderr": stderr.0,
                "stdout_path": stdout_path.to_string_lossy(),
                "stderr_path": stderr_path.to_string_lossy(),
                "truncated": stdout.1 || stderr.1,
            })
            .to_string()
        }
        Ok(Err(e)) => {
            format!(r#"{{"error": "process error: {}"}}"#, e)
        }
        Err(_) => {
            // Timeout — spawn background waiter
            let store_bg = ctx.shell_store.0.clone();
            let tid_bg = task_id.clone();
            tokio::spawn(async move {
                let exit = child.wait().await;
                let mut map = store_bg.lock().unwrap();
                if let Some(t) = map.get_mut(&tid_bg) {
                    t.mark_finished(exit.ok().and_then(|s| s.code()));
                }
            });

            let elapsed = chrono::Local::now()
                .signed_duration_since(started_at)
                .num_milliseconds()
                .max(0) as u64;

            serde_json::json!({
                "task_id": task_id,
                "pid": pid,
                "status": "running",
                "execution_time_ms": elapsed,
                "message": "Command still running after 30s. Use check_shell_status to poll.",
                "stdout_path": stdout_path.to_string_lossy(),
                "stderr_path": stderr_path.to_string_lossy(),
            })
            .to_string()
        }
    }
}

// ---- check_shell_status ----

pub struct CheckShellStatusTool;

impl Tool for CheckShellStatusTool {
    fn name(&self) -> &str {
        "check_shell_status"
    }

    fn definition(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "check_shell_status",
                "description": "Check the status of a previously executed shell command by its task_id. Returns current execution status, return code (if finished), execution time, and stdout/stderr content.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "task_id": {
                            "type": "string",
                            "description": "The task ID returned by execute_shell"
                        }
                    },
                    "required": ["task_id"]
                }
            }
        })
    }

    fn execute<'a>(
        &'a self,
        arguments: &'a str,
        ctx: &'a ToolContext,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = String> + Send + 'a>> {
        Box::pin(check_shell_status_impl(arguments, ctx))
    }
}

async fn check_shell_status_impl(arguments: &str, ctx: &ToolContext) -> String {
    let args: serde_json::Value = serde_json::from_str(arguments).unwrap_or_default();
    let task_id = args["task_id"].as_str().unwrap_or("").to_string();
    if task_id.is_empty() {
        return r#"{"error": "missing 'task_id' parameter"}"#.to_string();
    }

    let map = ctx.shell_store.0.lock().unwrap();
    match map.get(&task_id) {
        Some(task) => {
            let stdout = read_output(task.stdout_path());
            let stderr = read_output(task.stderr_path());
            let (status, return_code, elapsed) = task.status_info();

            serde_json::json!({
                "task_id": task_id,
                "pid": task.pid(),
                "status": status,
                "return_code": return_code,
                "execution_time_ms": elapsed,
                "stdout": stdout.0,
                "stderr": stderr.0,
                "stdout_path": task.stdout_path().to_string_lossy(),
                "stderr_path": task.stderr_path().to_string_lossy(),
                "truncated": stdout.1 || stderr.1,
            })
            .to_string()
        }
        None => format!(r#"{{"error": "task not found: {}"}}"#, task_id),
    }
}
