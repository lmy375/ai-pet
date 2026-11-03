use crate::tools::{parse_args, tool_error, Tool, ToolContext, ToolSpec};

use crate::shell::{
    spawn_bash, BashRequest, BashStart, BashWait, DEFAULT_TIMEOUT_MS, MAX_TIMEOUT_MS,
};

// ---- bash ----

pub struct BashTool;

#[derive(serde::Deserialize)]
struct BashArgs {
    command: String,
    description: Option<String>,
    working_directory: Option<String>,
    timeout: Option<u64>,
    run_in_background: Option<bool>,
    repl: Option<bool>,
}

impl Tool for BashTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "bash",
            description: "Execute a bash command on the user's machine. stdout/stderr are captured to files.\n\nIMPORTANT: Do NOT use bash to run cat/head/tail to read files — use read_file instead. Do NOT use sed/awk to edit files — use edit_file instead. Do NOT use echo/cat heredoc to create files — use write_file instead. Reserve bash for system commands and terminal operations that require shell execution (e.g. git, npm, cargo, ls, find, curl, etc.).\n\nmacOS app control/reading: you can also run `osascript` here to read from and drive the user's apps — AppleScript for scriptable apps (e.g. Terminal `do script`), and System Events for GUI scripting (keystroke/click) on apps that aren't scriptable (e.g. WeChat). See the tool-usage guide for patterns and the OS permissions involved.\n\nBehavior:\n- Commands finishing within the timeout return results directly.\n- Commands exceeding the timeout return a task_id and keep running in the background.\n- Set run_in_background: true to return immediately with a task_id without waiting.\n- Set repl: true to start a long-running interactive process (Python, Node, sqlite3, bc, psql, etc.) with stdin kept open. The process always runs in background. Use write_stdin to send commands and read output.\n- When a backgrounded command finishes you are notified automatically and the conversation continues — do NOT keep polling check_task_status.\n- The working directory does NOT persist between calls — use absolute paths or set working_directory.\n- You can specify a custom timeout up to 600000ms (10 minutes).",
            schema: serde_json::json!({
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
                        "description": "Working directory for the command. Defaults to the current working directory, which is stated in the tool-usage guide."
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
            }),
        }
    }

    crate::impl_execute!(bash_impl);
}

async fn bash_impl(arguments: &str, ctx: &ToolContext) -> String {
    let args: BashArgs = match parse_args(arguments) {
        Ok(args) => args,
        Err(e) => return e,
    };
    let command = args.command;
    if command.is_empty() {
        return tool_error("missing 'command' parameter");
    }
    if !ctx.swarm_worker_command_allowed(&command) {
        return tool_error("swarm workers cannot access long-term memory files");
    }

    let description = args.description.as_deref().unwrap_or("");
    let timeout_ms = args.timeout.unwrap_or(DEFAULT_TIMEOUT_MS).min(MAX_TIMEOUT_MS);
    let repl = args.repl.unwrap_or(false);
    // REPL always runs in background; otherwise honour the explicit flag.
    let run_in_background = repl || args.run_in_background.unwrap_or(false);

    let start = match spawn_bash(
        BashRequest {
            command: &command,
            description,
            working_directory: args.working_directory.as_deref(),
            timeout_ms,
            repl,
            run_in_background,
        },
        &ctx.shell_store,
        &ctx.session_id,
        ctx.notifier.clone(),
    )
    .await
    {
        Ok(start) => start,
        Err(e) => return tool_error(e),
    };

    let log_desc = if description.is_empty() {
        command.clone()
    } else {
        format!("{} ({})", description, command)
    };
    ctx.log(&format!(
        "Bash[{}] pid={} {}",
        start.task_id, start.pid, log_desc
    ));

    let BashStart {
        task_id,
        pid,
        stdout_path,
        stderr_path,
        wait,
    } = start;
    let stdout_path = stdout_path.to_string_lossy();
    let stderr_path = stderr_path.to_string_lossy();

    match wait {
        BashWait::Failed(message) => tool_error(message),
        BashWait::Finished {
            return_code,
            execution_time_ms,
            stdout,
            stderr,
            truncated,
        } => serde_json::json!({
            "task_id": task_id,
            "pid": pid,
            "status": "finished",
            "return_code": return_code,
            "execution_time_ms": execution_time_ms,
            "stdout": stdout,
            "stderr": stderr,
            "stdout_path": stdout_path,
            "stderr_path": stderr_path,
            "truncated": truncated,
        })
        .to_string(),
        BashWait::Background {
            timed_out,
            elapsed_ms,
        } => {
            // Wording and the extra `execution_time_ms` on timeout are the tool's
            // wire format — kept exactly as before the shared skeleton.
            if timed_out {
                serde_json::json!({
                    "task_id": task_id,
                    "pid": pid,
                    "status": "running",
                    "execution_time_ms": elapsed_ms,
                    "message": format!("Command still running after {}ms. You will be notified when it finishes — do not poll check_task_status.", timeout_ms),
                    "stdout_path": stdout_path,
                    "stderr_path": stderr_path,
                })
            } else {
                serde_json::json!({
                    "task_id": task_id,
                    "pid": pid,
                    "status": "running",
                    "message": "Command started in background. You will be notified when it finishes — do not poll check_task_status.",
                    "stdout_path": stdout_path,
                    "stderr_path": stderr_path,
                })
            }
            .to_string()
        }
    }
}

// ---- check_shell_status ----

pub struct CheckShellStatusTool;

#[derive(serde::Deserialize)]
struct CheckShellStatusArgs {
    task_id: String,
}

impl Tool for CheckShellStatusTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "check_task_status",
            description: "Check the status of a background task by its task_id (returned by the bash or spawn_subagent tools). Returns execution status, return code (if finished), execution time, and output (bash stdout/stderr, or the sub-agent's result).\n\nNote: when a background task finishes you are notified automatically — use this only if you need to check on a still-running task, not to poll for completion.",
            schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "task_id": {
                        "type": "string",
                        "description": "The task_id returned by the bash or spawn_subagent tool"
                    }
                },
                "required": ["task_id"]
            }),
        }
    }

    crate::impl_execute!(check_shell_status_impl);
}

async fn check_shell_status_impl(arguments: &str, ctx: &ToolContext) -> String {
    let args: CheckShellStatusArgs = match parse_args(arguments) {
        Ok(args) => args,
        Err(e) => return e,
    };
    let task_id = args.task_id;
    if task_id.is_empty() {
        return tool_error("missing 'task_id' parameter");
    }

    match crate::shell::check_task_status(&ctx.shell_store, &task_id) {
        // Works for both bash (reads its output files) and sub-agent (stored result).
        Ok(result) => serde_json::to_string(&result)
            .unwrap_or_else(|_| tool_error("failed to serialize status")),
        Err(e) => tool_error(e),
    }
}

// ---- write_stdin ----

pub struct WriteStdinTool;

#[derive(serde::Deserialize)]
struct WriteStdinArgs {
    task_id: String,
    /// May be an empty line, so it is required as a string but not non-empty.
    input: String,
    wait_ms: Option<u64>,
}

impl Tool for WriteStdinTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "write_stdin",
            description: "Send a line of input to a running REPL process started with bash repl:true. A newline is automatically appended. Waits wait_ms milliseconds then returns current stdout content.",
            schema: serde_json::json!({
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
            }),
        }
    }

    crate::impl_execute!(write_stdin_impl);
}

async fn write_stdin_impl(arguments: &str, ctx: &ToolContext) -> String {
    let args: WriteStdinArgs = match parse_args(arguments) {
        Ok(args) => args,
        Err(e) => return e,
    };
    let WriteStdinArgs { task_id, input, wait_ms } = args;
    if task_id.is_empty() {
        return tool_error("missing 'task_id' parameter");
    }
    let wait_ms = wait_ms.unwrap_or(500).min(10_000);

    let sender = match ctx.shell_store.snapshot(&task_id) {
        Some(task) => match task.stdin_sender {
            Some(tx) => tx,
            None => {
                return tool_error(format!(
                    "task {} has no stdin (not started with repl:true)",
                    task_id
                ))
            }
        },
        None => return tool_error(format!("task not found: {}", task_id)),
    };

    let line = format!("{}\n", input);
    if sender.send(line).await.is_err() {
        return tool_error(format!(
            "stdin channel closed — process may have exited for task {}",
            task_id
        ));
    }

    tokio::time::sleep(std::time::Duration::from_millis(wait_ms)).await;

    match ctx.shell_store.snapshot(&task_id) {
        Some(task) => {
            let result = crate::shell::build_shell_result(&task_id, &task);
            let (status, stdout) = result.status_and_stdout();
            serde_json::json!({
                "task_id": task_id,
                "status": status,
                "stdout": stdout,
            })
            .to_string()
        }
        None => tool_error(format!("task not found after wait: {}", task_id)),
    }
}