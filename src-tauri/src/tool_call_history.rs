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

/// 「宠物常用工具」面板视图给前端用。`name` 是工具名，`count` 是其在
/// 当前 ring buffer 里出现次数，`last_used_at` 是最近一次调用 timestamp
/// （格式与 `record_tool_call` 写入一致：`YYYY-MM-DD HH:MM:SS`）。按
/// count 降序、count 相同时 last_used_at 字典序降序（更近优先），稳定可
/// 测。
#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
pub struct ToolUsageStat {
    pub name: String,
    pub count: u64,
    pub last_used_at: String,
}

/// 从一组记录里派生 top N 工具。Pure，单测友好；调用方传 records
/// （任意来源 / 顺序），结果按上面文档的两层排序排好。`top_n == 0` →
/// 空 Vec。
pub fn derive_top_tools(records: &[ToolCallRecord], top_n: usize) -> Vec<ToolUsageStat> {
    if top_n == 0 {
        return Vec::new();
    }
    use std::collections::HashMap;
    // (count, latest_ts) 的中间累加：每条记录递增 count、刷新 latest_ts。
    let mut acc: HashMap<String, (u64, String)> = HashMap::new();
    for r in records {
        let entry = acc.entry(r.name.clone()).or_insert((0, String::new()));
        entry.0 += 1;
        // timestamp 字符串可比 — 同格式 `YYYY-MM-DD HH:MM:SS` 字典序与时序一致。
        if r.timestamp > entry.1 {
            entry.1 = r.timestamp.clone();
        }
    }
    let mut stats: Vec<ToolUsageStat> = acc
        .into_iter()
        .map(|(name, (count, last_used_at))| ToolUsageStat {
            name,
            count,
            last_used_at,
        })
        .collect();
    stats.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| b.last_used_at.cmp(&a.last_used_at))
            .then_with(|| a.name.cmp(&b.name))
    });
    stats.truncate(top_n);
    stats
}

/// Tauri command 形：从当前 ring buffer 派生 top 5。前端在 PanelPersona
/// 「最近常用的工具」一节展示。空 buffer → 空 Vec。
#[tauri::command]
pub fn get_top_tools_used() -> Vec<ToolUsageStat> {
    derive_top_tools(&recent_tool_calls(), 5)
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

    fn rec(name: &str, ts: &str) -> ToolCallRecord {
        ToolCallRecord {
            timestamp: ts.to_string(),
            name: name.to_string(),
            args_excerpt: String::new(),
            purpose: String::new(),
            risk_level: "low".to_string(),
            reasons: Vec::new(),
            safe_alternative: None,
            review_status: ToolCallReviewStatus::NotRequired,
            result_excerpt: String::new(),
        }
    }

    #[test]
    fn derive_top_tools_empty_input_returns_empty() {
        assert!(derive_top_tools(&[], 5).is_empty());
    }

    #[test]
    fn derive_top_tools_zero_top_n_returns_empty() {
        let recs = vec![rec("a", "2026-05-01 10:00:00")];
        assert!(derive_top_tools(&recs, 0).is_empty());
    }

    #[test]
    fn derive_top_tools_orders_by_count_desc() {
        let recs = vec![
            rec("read_file", "2026-05-01 10:00:00"),
            rec("read_file", "2026-05-02 10:00:00"),
            rec("read_file", "2026-05-03 10:00:00"),
            rec("write_file", "2026-05-04 10:00:00"),
        ];
        let out = derive_top_tools(&recs, 5);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].name, "read_file");
        assert_eq!(out[0].count, 3);
        assert_eq!(out[1].name, "write_file");
        assert_eq!(out[1].count, 1);
    }

    #[test]
    fn derive_top_tools_picks_latest_timestamp_per_tool() {
        let recs = vec![
            rec("memory_search", "2026-05-01 09:00:00"),
            rec("memory_search", "2026-05-03 12:00:00"),
            rec("memory_search", "2026-05-02 15:00:00"),
        ];
        let out = derive_top_tools(&recs, 5);
        assert_eq!(out[0].name, "memory_search");
        assert_eq!(out[0].last_used_at, "2026-05-03 12:00:00");
    }

    #[test]
    fn derive_top_tools_truncates_to_top_n() {
        let recs = vec![
            rec("a", "2026-05-01 10:00:00"),
            rec("b", "2026-05-01 10:00:00"),
            rec("c", "2026-05-01 10:00:00"),
            rec("d", "2026-05-01 10:00:00"),
            rec("e", "2026-05-01 10:00:00"),
            rec("f", "2026-05-01 10:00:00"),
        ];
        let out = derive_top_tools(&recs, 3);
        assert_eq!(out.len(), 3);
    }

    #[test]
    fn derive_top_tools_breaks_count_ties_by_recency() {
        // a 和 b 各 1 次；b 更新更近 → 排前。
        let recs = vec![
            rec("a", "2026-05-01 10:00:00"),
            rec("b", "2026-05-02 10:00:00"),
        ];
        let out = derive_top_tools(&recs, 5);
        assert_eq!(out[0].name, "b");
        assert_eq!(out[1].name, "a");
    }

    #[test]
    fn derive_top_tools_breaks_full_tie_alphabetically() {
        let recs = vec![
            rec("zeta", "2026-05-01 10:00:00"),
            rec("alpha", "2026-05-01 10:00:00"),
        ];
        let out = derive_top_tools(&recs, 5);
        assert_eq!(out[0].name, "alpha");
        assert_eq!(out[1].name, "zeta");
    }
}
