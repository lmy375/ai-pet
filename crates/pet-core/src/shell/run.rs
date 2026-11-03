use std::future::Future;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Local};
use tokio::io::AsyncWriteExt;

use crate::shell::output::read_with_truncation;
use crate::shell::task::{BashProcess, ShellStore, ShellTask, TaskKind};
use crate::shell::{TaskNotifier, SHELL_DIR};

/// How one unit of work ended, from the point of view of `run_task`.
pub(super) enum TaskOutcome {
    /// Normal completion: `(return_code, optional single-string result)`. Bash
    /// returns `None` for the result because its output lives in files.
    Done(Option<i32>, Option<String>),
    /// The work failed before producing a result; the text is the payload shown
    /// to the caller/notification.
    Failed(Option<i32>, String),
}

/// The result of running a task with the shared skeleton.
pub(super) enum TaskRun {
    Finished {
        return_code: Option<i32>,
        result: Option<String>,
        failed: bool,
        elapsed_ms: u64,
    },
    /// Either explicitly started in the background or auto-converted on timeout.
    Background { timed_out: bool, elapsed_ms: u64 },
}

fn elapsed_ms(started_at: DateTime<Local>) -> u64 {
    Local::now()
        .signed_duration_since(started_at)
        .num_milliseconds()
        .max(0) as u64
}

/// Turn the join result of the work future into a task completion.
fn resolve(
    joined: Result<TaskOutcome, tokio::task::JoinError>,
) -> (Option<i32>, Option<String>, bool) {
    match joined {
        Ok(TaskOutcome::Done(code, result)) => (code, result, false),
        Ok(TaskOutcome::Failed(code, err)) => (code, Some(err), true),
        // A join error means the work panicked: report it as a failed task.
        Err(e) => (
            Some(-1),
            Some(format!(r#"{{"error": "task join error: {}"}}"#, e)),
            true,
        ),
    }
}

/// The shared "register → wait with timeout → background → notify" skeleton.
///
/// Both bash (`spawn_bash`) and string-result work (`run_or_background`, used by
/// sub-agents and heartbeats) go through here, so their timeout/background/panel
/// semantics stay identical. The `task` is pre-built by the caller (bash needs
/// pid/output files); `task_id` must match.
async fn run_task<F>(
    store: &ShellStore,
    notifier: Option<Arc<dyn TaskNotifier>>,
    task_id: String,
    task: ShellTask,
    timeout_ms: u64,
    run_in_background: bool,
    work: F,
) -> TaskRun
where
    F: Future<Output = TaskOutcome> + Send + 'static,
{
    // Bash is stopped by killing its process group, so only string-result work
    // needs a cancel handle (the only way to stop a future with no OS process).
    let records_abort = task.kind != TaskKind::Bash;
    let started_at = task.started_at;
    store.register(task_id.clone(), task);

    let mut handle = tokio::spawn(work);
    if records_abort {
        store.update(&task_id, |t| t.abort = Some(handle.abort_handle()));
    }
    let store_bg = store.clone();
    let notifier_bg = notifier.clone();

    if run_in_background {
        let tid = task_id.clone();
        tokio::spawn(async move {
            let (code, result, _) = resolve(handle.await);
            store_bg.finish_and_notify(&notifier_bg, &tid, code, result);
        });
        return TaskRun::Background {
            timed_out: false,
            elapsed_ms: 0,
        };
    }

    match tokio::time::timeout(Duration::from_millis(timeout_ms), &mut handle).await {
        Ok(joined) => {
            let (code, result, failed) = resolve(joined);
            // Foreground completion: return inline, no notification.
            store.update(&task_id, |t| {
                t.mark_finished(code);
                if result.is_some() {
                    t.result = result.clone();
                }
            });
            TaskRun::Finished {
                return_code: code,
                result,
                failed,
                elapsed_ms: elapsed_ms(started_at),
            }
        }
        Err(_) => {
            // Timed out in the foreground: it converts to a notified background
            // task, so it now belongs in the panel.
            store.update(&task_id, |t| t.mark_backgrounded());
            let tid = task_id.clone();
            tokio::spawn(async move {
                let (code, result, _) = resolve(handle.await);
                store_bg.finish_and_notify(&notifier_bg, &tid, code, result);
            });
            TaskRun::Background {
                timed_out: true,
                elapsed_ms: elapsed_ms(started_at),
            }
        }
    }
}

/// Run `work` (a future producing the final result string) with the same
/// explicit-background + timeout-auto-background semantics as bash. Registers a
/// task in the shared store; on background completion it stores the result and
/// fires the notification. Returns either the result inline (finished in time)
/// or a `{task_id, status:"running"}` JSON (backgrounded).
///
/// Bash goes through `spawn_bash`, which feeds its process wait into the same
/// `run_task` skeleton.
/// Descriptive fields of one string-result background task.
pub struct BackgroundRequest<'a> {
    pub session_id: &'a str,
    pub kind: TaskKind,
    pub label: String,
    pub input: String,
    pub timeout_ms: u64,
    pub run_in_background: bool,
}

pub async fn run_or_background<F>(
    store: &ShellStore,
    notifier: Option<Arc<dyn TaskNotifier>>,
    req: BackgroundRequest<'_>,
    work: F,
) -> String
where
    F: Future<Output = (Option<i32>, String)> + Send + 'static,
{
    let BackgroundRequest {
        session_id,
        kind,
        label,
        input,
        timeout_ms,
        run_in_background,
    } = req;
    let task_id = uuid::Uuid::new_v4().to_string();
    let started_at = Local::now();
    let task = ShellTask::new_background(
        kind,
        label,
        session_id.to_string(),
        started_at,
        input,
        run_in_background,
        notifier.is_some(),
    );
    let work = async move {
        let (code, result) = work.await;
        TaskOutcome::Done(code, Some(result))
    };

    match run_task(
        store,
        notifier,
        task_id.clone(),
        task,

        timeout_ms,
        run_in_background,
        work,
    )
    .await
    {
        TaskRun::Finished { result, .. } => result.unwrap_or_default(),
        TaskRun::Background { timed_out, .. } => {
            // The wording differs from bash on purpose (bash says "Command …"):
            // these two messages are part of the prompt text the model sees.
            let message = if timed_out {
                format!(
                    "{} still running after {}ms. You will be notified when it finishes — do not poll.",
                    kind.as_str(),
                    timeout_ms
                )
            } else {
                format!(
                    "{} started in background. You will be notified when it finishes — do not poll.",
                    kind.as_str()
                )
            };
            serde_json::json!({
                "task_id": task_id,
                "status": "running",
                "message": message,
            })
            .to_string()
        }
    }
}

/// Options for spawning one bash process.
pub(crate) struct BashRequest<'a> {
    pub command: &'a str,
    /// Short user-facing description; empty means "use the command".
    pub description: &'a str,
    pub working_directory: Option<&'a str>,
    pub timeout_ms: u64,
    pub repl: bool,
    pub run_in_background: bool,
}

/// How the spawn ended from the tool's point of view.
pub(crate) enum BashWait {
    Finished {
        return_code: Option<i32>,
        execution_time_ms: u64,
        stdout: String,
        stderr: String,
        truncated: bool,
    },
    Background {
        timed_out: bool,
        elapsed_ms: u64,
    },
    /// Waiting on the process failed (`process error: …`).
    Failed(String),
}

pub(crate) struct BashStart {
    pub task_id: String,
    pub pid: u32,
    pub stdout_path: PathBuf,
    pub stderr_path: PathBuf,
    pub wait: BashWait,
}

/// Spawn a bash command and wait for it through `run_task`: output goes to
/// per-task files, the process gets its own group (so `kill_task` can kill the
/// whole tree), a REPL keeps stdin open and a forwarder pumps lines from a
/// channel.
pub(crate) async fn spawn_bash(
    req: BashRequest<'_>,
    store: &ShellStore,
    session_id: &str,
    notifier: Option<Arc<dyn TaskNotifier>>,
) -> Result<BashStart, String> {
    let _ = std::fs::create_dir_all(SHELL_DIR);
    let task_id = uuid::Uuid::new_v4().to_string();
    let stdout_path = PathBuf::from(format!("{}/{}.stdout", SHELL_DIR, task_id));
    let stderr_path = PathBuf::from(format!("{}/{}.stderr", SHELL_DIR, task_id));

    let stdout_file = std::fs::File::create(&stdout_path)
        .map_err(|e| format!("failed to create stdout file: {}", e))?;
    let stderr_file = std::fs::File::create(&stderr_path)
        .map_err(|e| format!("failed to create stderr file: {}", e))?;

    let mut cmd = tokio::process::Command::new("bash");
    cmd.arg("-c")
        .arg(req.command)
        .stdout(Stdio::from(stdout_file))
        .stderr(Stdio::from(stderr_file));

    if req.repl {
        cmd.stdin(Stdio::piped());
    }

    // Put the command (and its children) in its own process group, so `kill_task`
    // can terminate the whole group by pid. The pgid equals this child's pid.
    #[cfg(unix)]
    cmd.process_group(0);

    // No explicit directory means the session's working directory ($HOME in the
    // GUI unless the owner picked another one, the launch directory in the CLI)
    // — never whatever the app process happens to have been started in.
    match req.working_directory {
        Some(dir) => cmd.current_dir(dir),
        None => cmd.current_dir(crate::workdir::get()),
    };

    let mut child = cmd.spawn().map_err(|e| format!("failed to spawn: {}", e))?;

    let pid = child.id().unwrap_or(0);
    let started_at = Local::now();
    let stdin_sender = repl_stdin_forwarder(req.repl, &mut child);

    let label = if req.description.is_empty() {
        req.command.to_string()
    } else {
        req.description.to_string()
    };

    let task = ShellTask::new_bash(
        BashProcess {
            pid,
            stdout_path: stdout_path.clone(),
            stderr_path: stderr_path.clone(),
            stdin_sender,
        },
        started_at,
        label,
        session_id.to_string(),
        req.command.to_string(),
        req.run_in_background,
        notifier.is_some(),
    );

    let work = async move {
        match child.wait().await {
            Ok(status) => TaskOutcome::Done(status.code(), None),
            Err(e) => TaskOutcome::Failed(Some(-1), format!("process error: {}", e)),
        }
    };

    let run = run_task(
        store,
        notifier,
        task_id.clone(),
        task,

        req.timeout_ms,
        req.run_in_background,
        work,
    )
    .await;

    let wait = match run {
        TaskRun::Finished {
            failed,
            return_code,
            result,
            elapsed_ms,
            ..
        } => {
            if failed {
                // Bash's work only produces a result text when the wait failed.
                BashWait::Failed(result.unwrap_or_default())
            } else {
                let (stdout, out_trunc) = read_with_truncation(&stdout_path);
                let (stderr, err_trunc) = read_with_truncation(&stderr_path);
                BashWait::Finished {
                    return_code,
                    execution_time_ms: elapsed_ms,
                    stdout,
                    stderr,
                    truncated: out_trunc || err_trunc,
                }
            }
        }
        TaskRun::Background {
            timed_out,
            elapsed_ms,
            ..
        } => BashWait::Background {
            timed_out,
            elapsed_ms,
        },
    };

    Ok(BashStart {
        task_id,
        pid,
        stdout_path,
        stderr_path,
        wait,
    })
}

/// REPL mode: take the piped stdin handle and forward lines from a channel.
fn repl_stdin_forwarder(
    repl: bool,
    child: &mut tokio::process::Child,
) -> Option<tokio::sync::mpsc::Sender<String>> {
    if !repl {
        return None;
    }
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
}
