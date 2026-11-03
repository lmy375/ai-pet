use std::path::PathBuf;

use chrono::{DateTime, Local};
use serde::Serialize;

use crate::shell::task::{ShellTask, TaskKind};

const MAX_OUTPUT: usize = 32768;

/// Find the nearest valid UTF-8 char boundary at or after `pos`.
fn ceil_char_boundary(s: &str, pos: usize) -> usize {
    let mut i = pos;
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

pub(crate) fn read_with_truncation(path: &PathBuf) -> (String, bool) {
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

fn capture_bash_output(task: &ShellTask) -> (String, String, bool) {
    if !task.stdout_path.as_os_str().is_empty() && task.stdout_path.exists() {
        let (stdout, out_trunc) = read_with_truncation(&task.stdout_path);
        let (stderr, err_trunc) = read_with_truncation(&task.stderr_path);
        (stdout, stderr, out_trunc || err_trunc)
    } else {
        (
            task.persisted_stdout.clone().unwrap_or_default(),
            task.persisted_stderr.clone().unwrap_or_default(),
            task.persisted_truncated,
        )
    }
}

/// A task's `(stdout, stderr, truncated)`. Bash reads its output files;
/// sub-agents and heartbeats have no files, so their final text lives in
/// `result`. Shared by `snapshot_task` and `build_shell_result`.
fn task_outputs(task: &ShellTask) -> (String, String, bool) {
    match task.kind {
        TaskKind::Bash => capture_bash_output(task),
        TaskKind::Subagent | TaskKind::Heartbeat => (
            task.result.clone().unwrap_or_default(),
            String::new(),
            false,
        ),
    }
}

/// The tool-facing shape of a task. `#[serde(skip)]` fields are projection-only
/// and never serialized into the tool-result JSON.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellResult {
    pub(super) task_id: String,
    pub(super) pid: u32,
    pub(super) status: String,
    pub(super) return_code: Option<i32>,
    pub(super) execution_time_ms: u64,
    // The bash command or sub-agent prompt that produced this task.
    pub(super) input: String,
    pub(super) stdout: String,
    pub(super) stderr: String,
    pub(super) stdout_path: String,
    pub(super) stderr_path: String,
    pub(super) truncated: bool,
    // Projection-only fields used to derive `TaskListItem` / `PersistedTask`.
    // Never serialized into the tool-result JSON.
    #[serde(skip)]
    pub(super) kind: TaskKind,
    #[serde(skip)]
    pub(super) label: String,
    #[serde(skip)]
    pub(super) started_at: DateTime<Local>,
    #[serde(skip)]
    pub(super) session_id: String,
}

impl ShellResult {
    /// `(status, stdout)` for `write_stdin`, which returns only those two fields.
    pub(crate) fn status_and_stdout(&self) -> (&str, &str) {
        (&self.status, &self.stdout)
    }

    /// Everything except the (possibly file-backed) output payload. Used by the
    /// panel list, which must not read task output files under the store lock.
    pub(super) fn metadata(task_id: &str, task: &ShellTask) -> Self {
        let (status, return_code, execution_time_ms) = task.status_info();
        Self {
            task_id: task_id.to_string(),
            pid: task.pid,
            status: status.to_string(),
            return_code,
            execution_time_ms,
            input: task.input.clone(),
            stdout: String::new(),
            stderr: String::new(),
            stdout_path: task.stdout_path.to_string_lossy().to_string(),
            stderr_path: task.stderr_path.to_string_lossy().to_string(),
            truncated: false,
            kind: task.kind,
            label: task.label.clone(),
            started_at: task.started_at,
            session_id: task.session_id.clone(),
        }
    }
}

pub(crate) fn build_shell_result(task_id: &str, task: &ShellTask) -> ShellResult {
    let mut result = ShellResult::metadata(task_id, task);
    let (stdout, stderr, truncated) = task_outputs(task);
    result.stdout = stdout;
    result.stderr = stderr;
    result.truncated = truncated;
    result
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
            true,
            true,
        );
        task.mark_finished(Some(0));
        task.result = Some("found 42 files".to_string());

        let r = build_shell_result("t1", &task);
        assert_eq!(r.status, "finished");
        assert_eq!(r.stdout, "found 42 files");
        assert_eq!(r.stderr, "");
        assert_eq!(r.input, "count the files in /tmp");
    }

    /// The projection-only fields on `ShellResult` must never leak into the tool
    /// JSON seen by the model.
    #[test]
    fn shell_result_json_keeps_the_tool_wire_shape() {
        let mut task = ShellTask::new_background(
            TaskKind::Subagent,
            "label".to_string(),
            "sess-1".to_string(),
            Local::now(),
            "prompt".to_string(),
            true,
            true,
        );
        task.mark_finished(Some(0));
        task.result = Some("done".to_string());

        let json = serde_json::to_value(build_shell_result("t1", &task)).unwrap();
        let mut keys: Vec<&str> = json
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "executionTimeMs",
                "input",
                "pid",
                "returnCode",
                "status",
                "stderr",
                "stderrPath",
                "stdout",
                "stdoutPath",
                "taskId",
                "truncated",
            ]
        );
    }
}
