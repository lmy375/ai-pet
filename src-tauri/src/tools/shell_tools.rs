use crate::tools::{Tool, ToolContext};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::AsyncWriteExt;

use crate::commands::shell::{
    cleanup_old_tasks, read_with_truncation, save_task_history, DEFAULT_TIMEOUT_MS, MAX_TIMEOUT_MS,
    SHELL_DIR,
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
                "description": "Execute a bash command on the user's machine. stdout/stderr are captured to files.\n\nIMPORTANT: Do NOT use bash to run cat/head/tail to read files — use read_file instead. Do NOT use sed/awk to edit files — use edit_file instead. Do NOT use echo/cat heredoc to create files — use write_file instead. Reserve bash for system commands and terminal operations that require shell execution (e.g. git, npm, cargo, ls, find, curl, etc.).\n\nmacOS app control/reading: you can also run `osascript` here to read from and drive the user's apps — AppleScript for scriptable apps (e.g. Terminal `do script`), and System Events for GUI scripting (keystroke/click) on apps that aren't scriptable (e.g. WeChat). See the tool-usage guide for patterns and the OS permissions involved.\n\nBehavior:\n- Commands finishing within the timeout return results directly.\n- Commands exceeding the timeout return a task_id and keep running in the background.\n- Set run_in_background: true to return immediately with a task_id without waiting.\n- Set repl: true to start a long-running interactive process (Python, Node, sqlite3, bc, psql, etc.) with stdin kept open. The process always runs in background. Use write_stdin to send commands and read output.\n- When a backgrounded command finishes you are notified automatically and the conversation continues — do NOT keep polling check_task_status.\n- The working directory does NOT persist between calls — use absolute paths or set working_directory.\n- You can specify a custom timeout up to 600000ms (10 minutes).",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "The bash command to execute"
                        },
                        "description": {
                            "type": "string",
                            "description": "Clear, concise description of what this command does in active voice (e.g. \"Install dependencies\", \"Run tests\", \"List project files\"). Shown to the user as the command's purpose — provide it for every call."
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
                            "description": "If true, return immediately with a task_id without waiting. You will be notified automatically when it finishes — do not poll. Use check_task_status only to inspect a still-running task."
                        },
                        "repl": {
                            "type": "boolean",
                            "description": "Start as a REPL: stdin stays open, process always runs in background. Use write_stdin to send input lines and read output."
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
    let args = super::parse_args(arguments);
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
    let repl = args["repl"].as_bool().unwrap_or(false);
    // REPL always runs in background; otherwise honour the explicit flag.
    let run_in_background = repl || args["run_in_background"].as_bool().unwrap_or(false);

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

    if repl {
        cmd.stdin(Stdio::piped());
    }

    // Put the command (and its children) in its own process group, so `kill_task`
    // can terminate the whole group by pid. The pgid equals this child's pid.
    #[cfg(unix)]
    cmd.process_group(0);

    if let Some(cwd) = working_directory {
        cmd.current_dir(cwd);
    }

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return format!(r#"{{"error": "failed to spawn: {}"}}"#, e),
    };

    let pid = child.id().unwrap_or(0);
    let started_at = chrono::Local::now();

    // Label shown in completion notifications: the description, else the command.
    let label = if description.is_empty() {
        command.clone()
    } else {
        description.to_string()
    };

    // REPL mode: take the piped stdin handle and forward lines from a channel.
    let stdin_sender = if repl {
        let child_stdin = child.stdin.take();
        let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(32);
        tokio::spawn(async move {
            if let Some(mut stdin) = child_stdin {
                while let Some(line) = rx.recv().await {
                    if stdin.write_all(line.as_bytes()).await.is_err() {
                        break;
                    }
                }
            }
        });
        Some(tx)
    } else {
        None
    };

    // Store task and cleanup old finished tasks
    {
        let mut map = ctx.shell_store.0.lock().unwrap();
        cleanup_old_tasks(&mut map);
        map.insert(
            task_id.clone(),
            crate::commands::shell::ShellTask::new_bash(
                pid,
                stdout_path.clone(),
                stderr_path.clone(),
                started_at,
                label,
                ctx.session_id.clone(),
                command.clone(),
                run_in_background,
                stdin_sender,
            ),
        );
        save_task_history(&map);
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
        let notifier_bg = ctx.notifier.clone();
        let tid_bg = task_id.clone();
        tokio::spawn(async move {
            let exit = child.wait().await;
            let code = exit.ok().and_then(|s| s.code());
            crate::commands::shell::mark_finished_and_notify(
                &store_bg,
                &notifier_bg,
                &tid_bg,
                code,
                None,
            );
        });

        return serde_json::json!({
            "task_id": task_id,
            "pid": pid,
            "status": "running",
            "message": "Command started in background. You will be notified when it finishes — do not poll check_task_status.",
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
                save_task_history(&map);
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
            // Timeout — converts to a notified background task, so it now belongs
            // in the panel. Mark it backgrounded before spawning the waiter.
            {
                let mut map = ctx.shell_store.0.lock().unwrap();
                if let Some(t) = map.get_mut(&task_id) {
                    t.mark_backgrounded();
                }
                save_task_history(&map);
            }
            // Spawn background waiter that notifies on completion.
            let store_bg = ctx.shell_store.0.clone();
            let notifier_bg = ctx.notifier.clone();
            let tid_bg = task_id.clone();
            tokio::spawn(async move {
                let exit = child.wait().await;
                let code = exit.ok().and_then(|s| s.code());
                crate::commands::shell::mark_finished_and_notify(
                    &store_bg,
                    &notifier_bg,
                    &tid_bg,
                    code,
                    None,
                );
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
                "message": format!("Command still running after {}ms. You will be notified when it finishes — do not poll check_task_status.", timeout_ms),
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
        "check_task_status"
    }

    fn definition(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "check_task_status",
                "description": "Check the status of a background task by its task_id (returned by the bash or spawn_subagent tools). Returns execution status, return code (if finished), execution time, and output (bash stdout/stderr, or the sub-agent's result).\n\nNote: when a background task finishes you are notified automatically — use this only if you need to check on a still-running task, not to poll for completion.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "task_id": {
                            "type": "string",
                            "description": "The task_id returned by the bash or spawn_subagent tool"
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
    let args = super::parse_args(arguments);
    let task_id = args["task_id"].as_str().unwrap_or("").to_string();
    if task_id.is_empty() {
        return r#"{"error": "missing 'task_id' parameter"}"#.to_string();
    }

    let map = ctx.shell_store.0.lock().unwrap();
    match map.get(&task_id) {
        // Works for both bash (reads its output files) and sub-agent (stored result).
        Some(task) => {
            serde_json::to_string(&crate::commands::shell::build_shell_result(&task_id, task))
                .unwrap_or_else(|_| r#"{"error": "failed to serialize status"}"#.to_string())
        }
        None => format!(r#"{{"error": "task not found: {}"}}"#, task_id),
    }
}

// ---- write_stdin ----

pub struct WriteStdinTool;

impl Tool for WriteStdinTool {
    fn name(&self) -> &str {
        "write_stdin"
    }

    fn definition(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "write_stdin",
                "description": "Send a line of input to a running REPL process started with bash repl:true. A newline is automatically appended. Waits wait_ms milliseconds then returns current stdout content.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "task_id": {
                            "type": "string",
                            "description": "The task_id returned by the bash (repl:true) call"
                        },
                        "input": {
                            "type": "string",
                            "description": "The line to send to stdin (newline appended automatically)"
                        },
                        "wait_ms": {
                            "type": "integer",
                            "description": "Milliseconds to wait for output before returning (default: 500, max: 10000)"
                        }
                    },
                    "required": ["task_id", "input"]
                }
            }
        })
    }

    fn execute<'a>(
        &'a self,
        arguments: &'a str,
        ctx: &'a ToolContext,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = String> + Send + 'a>> {
        Box::pin(write_stdin_impl(arguments, ctx))
    }
}

async fn write_stdin_impl(arguments: &str, ctx: &ToolContext) -> String {
    let args = super::parse_args(arguments);
    let task_id = args["task_id"].as_str().unwrap_or("").to_string();
    let input = match args["input"].as_str() {
        Some(s) => s.to_string(),
        None => return r#"{"error": "missing 'input' parameter"}"#.to_string(),
    };
    if task_id.is_empty() {
        return r#"{"error": "missing 'task_id' parameter"}"#.to_string();
    }
    let wait_ms = args["wait_ms"].as_u64().unwrap_or(500).min(10_000);

    let sender = {
        let map = ctx.shell_store.0.lock().unwrap();
        match map.get(&task_id) {
            Some(task) => match &task.stdin_sender {
                Some(tx) => tx.clone(),
                None => return format!(r#"{{"error": "task {} has no stdin (not started with repl:true)"}}"#, task_id),
            },
            None => return format!(r#"{{"error": "task not found: {}"}}"#, task_id),
        }
    };

    let line = format!("{}\n", input);
    if sender.send(line).await.is_err() {
        return format!(r#"{{"error": "stdin channel closed — process may have exited for task {}"}}"#, task_id);
    }

    tokio::time::sleep(std::time::Duration::from_millis(wait_ms)).await;

    let map = ctx.shell_store.0.lock().unwrap();
    match map.get(&task_id) {
        Some(task) => {
            let (status, stdout) = crate::commands::shell::task_status_and_stdout(&task_id, task);
            serde_json::json!({
                "task_id": task_id,
                "status": status,
                "stdout": stdout,
            })
            .to_string()
        }
        None => format!(r#"{{"error": "task not found after wait: {}"}}"#, task_id),
    }
}
