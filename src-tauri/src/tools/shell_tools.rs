use crate::tools::{Tool, ToolContext};
use std::path::PathBuf;
use std::process::Stdio;

use crate::commands::shell::{
    cleanup_old_tasks, read_with_truncation, DEFAULT_TIMEOUT_MS, MAX_TIMEOUT_MS, SHELL_DIR,
};

// ---- bash ----

pub struct BashTool;

impl Tool for BashTool {
    fn name(&self) -> &str {
        "bash"
    }

    fn definition(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "bash",
                "description": "Execute a bash command on the user's machine. stdout/stderr are captured to files.\n\nIMPORTANT: Do NOT use bash to run cat/head/tail to read files — use read_file instead. Do NOT use sed/awk to edit files — use edit_file instead. Do NOT use echo/cat heredoc to create files — use write_file instead. Reserve bash exclusively for system commands and terminal operations that require shell execution (e.g. git, npm, cargo, ls, find, curl, etc.).\n\nBehavior:\n- Commands finishing within the timeout return results directly.\n- Commands exceeding the timeout return a task_id — use check_shell_status to poll.\n- Set run_in_background: true to return immediately with a task_id without waiting.\n- The working directory does NOT persist between calls — use absolute paths or set working_directory.\n- You can specify a custom timeout up to 600000ms (10 minutes).",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "The bash command to execute"
                        },
                        "description": {
                            "type": "string",
                            "description": "Short description of what this command does (for logging)"
                        },
                        "working_directory": {
                            "type": "string",
                            "description": "Working directory for the command. Defaults to the system default if not specified."
                        },
                        "timeout": {
                            "type": "integer",
                            "description": "Timeout in milliseconds (default: 120000, max: 600000). Command returns a task_id if it exceeds the timeout."
                        },
                        "run_in_background": {
                            "type": "boolean",
                            "description": "If true, return immediately with a task_id without waiting. Use check_shell_status to poll for results."
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
        Box::pin(bash_impl(arguments, ctx))
    }
}

async fn bash_impl(arguments: &str, ctx: &ToolContext) -> String {
    let args: serde_json::Value = serde_json::from_str(arguments).unwrap_or_default();
    let command = args["command"].as_str().unwrap_or("").to_string();
    if command.is_empty() {
        return r#"{"error": "missing 'command' parameter"}"#.to_string();
    }

    let description = args["description"].as_str().unwrap_or("");
    let working_directory = args["working_directory"].as_str();
    let timeout_ms = args["timeout"]
        .as_u64()
        .unwrap_or(DEFAULT_TIMEOUT_MS)
        .min(MAX_TIMEOUT_MS);
    let run_in_background = args["run_in_background"].as_bool().unwrap_or(false);

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

    let mut cmd = tokio::process::Command::new("bash");
    cmd.arg("-c")
        .arg(&command)
        .stdout(Stdio::from(stdout_file))
        .stderr(Stdio::from(stderr_file));

    if let Some(cwd) = working_directory {
        cmd.current_dir(cwd);
    }

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return format!(r#"{{"error": "failed to spawn: {}"}}"#, e),
    };

    let pid = child.id().unwrap_or(0);
    let started_at = chrono::Local::now();

    // Store task and cleanup old finished tasks
    {
        let mut map = ctx.shell_store.0.lock().unwrap();
        cleanup_old_tasks(&mut map);
        map.insert(
            task_id.clone(),
            crate::commands::shell::ShellTask::new(
                pid,
                stdout_path.clone(),
                stderr_path.clone(),
                started_at,
            ),
        );
    }

    let log_desc = if description.is_empty() {
        command.clone()
    } else {
        format!("{} ({})", description, command)
    };
    ctx.log(&format!("Bash[{}] pid={} {}", task_id, pid, log_desc));

    // Background mode: return immediately
    if run_in_background {
        let store_bg = ctx.shell_store.0.clone();
        let tid_bg = task_id.clone();
        tokio::spawn(async move {
            let exit = child.wait().await;
            let mut map = store_bg.lock().unwrap();
            if let Some(t) = map.get_mut(&tid_bg) {
                t.mark_finished(exit.ok().and_then(|s| s.code()));
            }
        });

        return serde_json::json!({
            "task_id": task_id,
            "pid": pid,
            "status": "running",
            "message": "Command started in background. Use check_shell_status to poll.",
            "stdout_path": stdout_path.to_string_lossy(),
            "stderr_path": stderr_path.to_string_lossy(),
        })
        .to_string();
    }

    // Wait with timeout
    let timeout_duration = std::time::Duration::from_millis(timeout_ms);
    match tokio::time::timeout(timeout_duration, child.wait()).await {
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
            let stdout = read_with_truncation(&stdout_path);
            let stderr = read_with_truncation(&stderr_path);

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
                "message": format!("Command still running after {}ms. Use check_shell_status to poll.", timeout_ms),
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
                "description": "Check the status of a background shell command by its task_id (returned by bash tool). Returns execution status, return code (if finished), execution time, and stdout/stderr content.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "task_id": {
                            "type": "string",
                            "description": "The task_id returned by the bash tool"
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
            let stdout = read_with_truncation(task.stdout_path());
            let stderr = read_with_truncation(task.stderr_path());
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
