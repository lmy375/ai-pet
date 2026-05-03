//! Per-tool-call structured ring buffer (Iter R4).
//!
//! Surfaces what TR1 (purpose) / TR2 (risk) / TR3 (review) already write to
//! `app.log` as a structured panel-readable timeline. Each chat-pipeline tool
//! invocation pushes one `ToolCallRecord` here, capped at
//! `TOOL_CALL_HISTORY_CAP`. The panel reads via `recent_tool_calls()` (or via
//! `DebugSnapshot.recent_tool_calls`) and renders a collapsible "工具调用历史"
//! card so prompt-tuning sessions don't require grepping app.log.
//!
//! Distinct from `app.log` parsing: this captures the data atomically at the
//! call site (no fragile log-line regex), keeps known-shape fields (risk,
//! review_status), and is bounded so memory stays flat across long sessions.

use std::collections::VecDeque;
use std::sync::Mutex;

use serde::Serialize;

/// Maximum records retained in-process. 30 covers a typical chat session
/// (multi-turn tool use) without growing memory; older records roll off via
/// `pop_front` like `LAST_PROACTIVE_TURNS` (Iter E4).
pub const TOOL_CALL_HISTORY_CAP: usize = 30;

/// Cap on persisted args / result excerpt characters so the ring buffer stays
/// bounded even if a single tool call returns a multi-MB blob.
pub const TOOL_CALL_FIELD_CHARS: usize = 200;

/// Outcome flag the panel renders as a badge. Mirrors the four post-purpose
/// branches in `run_chat_pipeline`'s tool loop. NotRequired covers low/medium
/// risk paths that bypass TR3 entirely.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolCallReviewStatus {
    NotRequired,
    Approved,
    Denied,
    Timeout,
    MissingPurpose,
}

impl ToolCallReviewStatus {
    /// Serializable string form of the variant. Mirrors what serde's
    /// `rename_all = "lowercase"` would produce, but kept as an explicit
    /// helper so backend-side test assertions don't depend on serde
    /// internals (frontend matches on these strings to render badge color).
    #[allow(dead_code)]
    pub fn as_str(self) -> &'static str {
        match self {
            ToolCallReviewStatus::NotRequired => "not_required",
            ToolCallReviewStatus::Approved => "approved",
            ToolCallReviewStatus::Denied => "denied",
            ToolCallReviewStatus::Timeout => "timeout",
            ToolCallReviewStatus::MissingPurpose => "missing_purpose",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolCallRecord {
    pub timestamp: String,
    pub name: String,
    /// Truncated to `TOOL_CALL_FIELD_CHARS`. Originals can grow large (file
    /// content writes, long shell commands) and would balloon the snapshot.
    pub args_excerpt: String,
    /// Empty when the call was rejected for missing purpose.
    pub purpose: String,
    /// "low" / "medium" / "high" — matches `ToolRiskLevel::as_str()`.
    pub risk_level: String,
    pub reasons: Vec<String>,
    pub safe_alternative: Option<String>,
    pub review_status: ToolCallReviewStatus,
    /// First chars of the tool result (real result for execute path; synthetic
    /// JSON for missing_purpose / denied / timeout). Same truncation rule.
    pub result_excerpt: String,
}

pub static TOOL_CALL_HISTORY: Mutex<VecDeque<ToolCallRecord>> = Mutex::new(VecDeque::new());

/// Truncate to `TOOL_CALL_FIELD_CHARS` chars (counting Unicode chars, not
/// bytes) with a `…` suffix when overflow. Pure / testable so the cap rule
/// has a deterministic test surface.
pub fn truncate_excerpt(text: &str) -> String {
    if text.chars().count() <= TOOL_CALL_FIELD_CHARS {
        text.to_string()
    } else {
        let head: String = text.chars().take(TOOL_CALL_FIELD_CHARS).collect();
        format!("{}…", head)
    }
}

/// Push a record onto the in-memory ring buffer. Errors / poison swallowed
/// (consistent with the rest of the static-Mutex stash pattern in this crate).
#[allow(clippy::too_many_arguments)] // each field is captured at the call site from a distinct source
pub fn record_tool_call(
    name: &str,
    args_json: &str,
    purpose: &str,
    risk_level: &str,
    reasons: &[String],
    safe_alternative: Option<&str>,
    review_status: ToolCallReviewStatus,
    result: &str,
) {
    let record = ToolCallRecord {
        timestamp: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        name: name.to_string(),
        args_excerpt: truncate_excerpt(args_json),
        purpose: purpose.to_string(),
        risk_level: risk_level.to_string(),
        reasons: reasons.to_vec(),
        safe_alternative: safe_alternative.map(String::from),
        review_status,
        result_excerpt: truncate_excerpt(result),
    };
    if let Ok(mut g) = TOOL_CALL_HISTORY.lock() {
        g.push_back(record);
        while g.len() > TOOL_CALL_HISTORY_CAP {
            g.pop_front();
        }
    }
}

/// Snapshot the buffer newest-first for the panel. Cloning is cheap relative
/// to the IPC round-trip, and keeps callers from holding the mutex.
pub fn recent_tool_calls() -> Vec<ToolCallRecord> {
    let g = match TOOL_CALL_HISTORY.lock() {
        Ok(g) => g,
        Err(_) => return Vec::new(),
    };
    let mut out: Vec<ToolCallRecord> = g.iter().cloned().collect();
    out.reverse();
    out
}

/// Tauri command for callers that don't pull the full DebugSnapshot.
#[tauri::command]
pub fn get_recent_tool_calls() -> Vec<ToolCallRecord> {
    recent_tool_calls()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Serialize tests that touch the static `TOOL_CALL_HISTORY` so they don't
    /// observe each other's pushes. Cargo runs tests in parallel by default;
    /// without this guard the assertion-on-count tests flake.
    static HISTORY_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn drain_history() {
        if let Ok(mut g) = TOOL_CALL_HISTORY.lock() {
            g.clear();
        }
    }

    #[test]
    fn truncate_excerpt_passes_short_through() {
        assert_eq!(truncate_excerpt("hello"), "hello");
        assert_eq!(truncate_excerpt(""), "");
    }

    #[test]
    fn truncate_excerpt_caps_long_inputs_with_ellipsis() {
        let s: String = "0123456789".chars().cycle().take(500).collect();
        let out = truncate_excerpt(&s);
        // 200 head chars + the ellipsis char = 201.
        assert_eq!(out.chars().count(), TOOL_CALL_FIELD_CHARS + 1);
        assert!(out.ends_with('…'));
    }

    #[test]
    fn truncate_excerpt_handles_chinese_correctly() {
        // Each Chinese char is 3 bytes UTF-8 but counts as 1 char. Truncation
        // must work on chars, not bytes — otherwise we'd cut mid-codepoint.
        let s: String = "中".repeat(300);
        let out = truncate_excerpt(&s);
        // Should produce 200 Chinese chars + ellipsis, never panic on byte boundaries.
        assert_eq!(out.chars().count(), TOOL_CALL_FIELD_CHARS + 1);
        assert!(out.starts_with('中'));
    }

    #[test]
    fn record_tool_call_pushes_with_newest_first_order() {
        let _guard = HISTORY_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        drain_history();
        record_tool_call(
            "read_file",
            r#"{"file_path":"a"}"#,
            "look at config",
            "low",
            &[],
            None,
            ToolCallReviewStatus::NotRequired,
            "ok",
        );
        record_tool_call(
            "bash",
            r#"{"command":"ls"}"#,
            "list files",
            "high",
            &["shell access".to_string()],
            Some("use read_file"),
            ToolCallReviewStatus::Denied,
            r#"{"error":"denied"}"#,
        );
        let recents = recent_tool_calls();
        assert_eq!(recents.len(), 2);
        assert_eq!(recents[0].name, "bash", "newest entry must be index 0");
        assert_eq!(recents[1].name, "read_file");
        assert_eq!(recents[0].review_status, ToolCallReviewStatus::Denied);
        assert_eq!(
            recents[0].safe_alternative.as_deref(),
            Some("use read_file")
        );
        drain_history();
    }

    #[test]
    fn record_tool_call_caps_at_history_max() {
        let _guard = HISTORY_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        drain_history();
        for i in 0..(TOOL_CALL_HISTORY_CAP + 5) {
            record_tool_call(
                &format!("tool{}", i),
                "{}",
                "p",
                "low",
                &[],
                None,
                ToolCallReviewStatus::NotRequired,
                "r",
            );
        }
        let recents = recent_tool_calls();
        assert_eq!(recents.len(), TOOL_CALL_HISTORY_CAP);
        // Newest-first → recents[0] is the last one we pushed.
        assert_eq!(
            recents[0].name,
            format!("tool{}", TOOL_CALL_HISTORY_CAP + 4)
        );
        // Oldest still in buffer is index 5 (5 fell off the front).
        assert_eq!(recents[TOOL_CALL_HISTORY_CAP - 1].name, "tool5");
        drain_history();
    }

    #[test]
    fn review_status_serializes_to_lowercase_string() {
        // The frontend matches on these strings to render the badge color.
        assert_eq!(ToolCallReviewStatus::NotRequired.as_str(), "not_required");
        assert_eq!(ToolCallReviewStatus::Approved.as_str(), "approved");
        assert_eq!(ToolCallReviewStatus::Denied.as_str(), "denied");
        assert_eq!(ToolCallReviewStatus::Timeout.as_str(), "timeout");
        assert_eq!(
            ToolCallReviewStatus::MissingPurpose.as_str(),
            "missing_purpose"
        );
    }
}
