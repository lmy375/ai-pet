//! Human-review gate for high-risk tool calls (Iter TR3).
//!
//! Sits one step downstream of the TR2 risk classifier: when `requires_human_review`
//! is true, the chat pipeline parks the tool call here, emits a polling-friendly
//! `PendingToolReview` snapshot to anyone watching (the panel reads it via
//! `DebugSnapshot`), and awaits up to `REVIEW_TIMEOUT_SECONDS` for an Approve /
//! Deny decision via `submit_tool_review`.
//!
//! Design choices:
//! - **Polling-readable snapshot** rather than a Tauri event push. The panel
//!   already polls `get_debug_snapshot` at 1 Hz (Iter QG6); adding a vec of
//!   pending reviews to that snapshot reuses the existing wire instead of
//!   adding event subscription complexity.
//! - **Default-deny on timeout**: high-risk by definition is something we'd
//!   rather refuse silently than fire-and-forget. The denial result the LLM
//!   sees carries a `safe_alternative` from the TR2 assessment so the model
//!   can self-correct on the next round.
//! - **Optional in `ToolContext`**: telegram / consolidate paths run without
//!   a review surface; passing `None` skips review entirely (high-risk just
//!   executes). Suitable for internal automation; if we later want stricter
//!   behavior there, add a registry to those paths.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tokio::sync::oneshot;

/// Hard ceiling on how long the pipeline waits for a human decision before
/// applying the default-deny policy. 60 s per the TR3 spec — long enough that
/// a focused user can switch to the panel and click, short enough that an
/// absent user doesn't make the LLM hang for minutes.
pub const REVIEW_TIMEOUT_SECONDS: u64 = 60;

/// Decision the human entered for a high-risk tool call. Approve → proceed to
/// the actual tool execution; Deny → return a synthetic error to the LLM with
/// the assessment's `safe_alternative` hint so the model can choose a safer
/// path next round.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolReviewDecision {
    Approve,
    Deny,
}

/// Snapshot of a single pending review request, surfaced to the frontend so it
/// can render the modal (tool name, why it's risky, what the LLM said it
/// wanted to do, what args it would use, what the safer path is).
#[derive(Debug, Clone, Serialize)]
pub struct PendingToolReview {
    pub review_id: String,
    pub tool_name: String,
    pub args_json: String,
    pub purpose: String,
    pub reasons: Vec<String>,
    pub safe_alternative: Option<String>,
    pub timestamp: String,
}

struct PendingEntry {
    sender: oneshot::Sender<ToolReviewDecision>,
    snapshot: PendingToolReview,
}

/// Process-wide registry of in-flight reviews. Each high-risk tool call
/// registers itself with a fresh review_id, gets a `oneshot::Receiver` to
/// await on, and the panel resolves it via `submit`. Entries are dropped on
/// resolution / timeout — the registry never grows unbounded.
pub struct ToolReviewRegistry {
    pending: Mutex<HashMap<String, PendingEntry>>,
    next_id: Mutex<u64>,
}

impl Default for ToolReviewRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolReviewRegistry {
    pub fn new() -> Self {
        Self {
            pending: Mutex::new(HashMap::new()),
            next_id: Mutex::new(1),
        }
    }

    /// Internal id generator. Monotonic counter (process-wide) keeps ids
    /// stable for the panel to reference. Format `tr-{n}` so panel logs stay
    /// readable when multiple reviews queue back-to-back.
    fn allocate_id(&self) -> String {
        let mut g = self.next_id.lock().unwrap();
        let id = format!("tr-{}", *g);
        *g += 1;
        id
    }

    /// Register a new review. Returns `(review_id, receiver)`. Caller awaits
    /// the receiver (typically with a timeout); panel calls `submit` when the
    /// user clicks. Cleanup of pending entry happens inside `submit` (success
    /// path) or `cancel` (timeout path).
    pub fn register(
        &self,
        tool_name: &str,
        args_json: &str,
        purpose: &str,
        reasons: &[String],
        safe_alternative: Option<&str>,
    ) -> (String, oneshot::Receiver<ToolReviewDecision>) {
        let review_id = self.allocate_id();
        let (tx, rx) = oneshot::channel();
        let snapshot = PendingToolReview {
            review_id: review_id.clone(),
            tool_name: tool_name.to_string(),
            args_json: args_json.to_string(),
            purpose: purpose.to_string(),
            reasons: reasons.to_vec(),
            safe_alternative: safe_alternative.map(String::from),
            timestamp: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        };
        self.pending.lock().unwrap().insert(
            review_id.clone(),
            PendingEntry {
                sender: tx,
                snapshot,
            },
        );
        (review_id, rx)
    }

    /// Resolve a pending review with the user's decision. Returns Err if the
    /// id isn't pending — typically means the entry already timed out and was
    /// cancelled, or never existed (panel state stale).
    pub fn submit(&self, review_id: &str, decision: ToolReviewDecision) -> Result<(), String> {
        let mut g = self.pending.lock().unwrap();
        let entry = g
            .remove(review_id)
            .ok_or_else(|| format!("review {} not pending", review_id))?;
        entry
            .sender
            .send(decision)
            .map_err(|_| "review channel already closed".to_string())
    }

    /// Drop a pending entry without sending. Used by the await-side when the
    /// timeout fires — the receiver future has been dropped, but the entry is
    /// still in the map taking up a slot until we clean it.
    pub fn cancel(&self, review_id: &str) {
        self.pending.lock().unwrap().remove(review_id);
    }

    /// Snapshot currently-pending reviews for the panel. Sorted by timestamp
    /// so the oldest pending is first — UX preference for the modal.
    pub fn snapshot(&self) -> Vec<PendingToolReview> {
        let mut v: Vec<PendingToolReview> = self
            .pending
            .lock()
            .unwrap()
            .values()
            .map(|e| e.snapshot.clone())
            .collect();
        v.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        v
    }
}

pub type ToolReviewRegistryStore = Arc<ToolReviewRegistry>;

pub fn new_tool_review_registry() -> ToolReviewRegistryStore {
    Arc::new(ToolReviewRegistry::new())
}

/// Iter R2: kinds used in `decision_log` for the three review outcomes.
/// Centralized so the panel "decisions" view can match on stable strings.
pub const KIND_REVIEW_APPROVE: &str = "ToolReviewApprove";
pub const KIND_REVIEW_DENY: &str = "ToolReviewDeny";
pub const KIND_REVIEW_TIMEOUT: &str = "ToolReviewTimeout";

/// Iter R2: push a tool-review outcome onto the proactive decision log so the
/// panel's "recent decisions" view shows approve / deny / timeout events
/// alongside Spoke / Silent / Skip. Reason format is `{review_id} {tool_name}`
/// — keeps it grep-friendly and parseable.
pub fn record_review_outcome(
    decisions: &crate::decision_log::DecisionLog,
    kind: &str,
    review_id: &str,
    tool_name: &str,
) {
    decisions.push(kind, format!("{} {}", review_id, tool_name));
}

/// Synthetic tool result returned to the LLM when the user denies. The hint
/// surface lets the model self-correct without burning another tool call.
pub fn denied_result_json(reason: &str, safe_alternative: Option<&str>) -> String {
    serde_json::json!({
        "error": "tool call denied by human review",
        "reason": reason,
        "safe_alternative": safe_alternative.unwrap_or("(无可推荐替代)"),
    })
    .to_string()
}

/// Synthetic tool result returned when the review window times out. Same
/// shape as `denied_result_json` so the LLM doesn't need a separate handler.
pub fn timeout_result_json(safe_alternative: Option<&str>) -> String {
    serde_json::json!({
        "error": "tool call review timed out",
        "reason": format!(
            "用户未在 {} 秒内审核，按安全默认策略拒绝",
            REVIEW_TIMEOUT_SECONDS
        ),
        "safe_alternative": safe_alternative.unwrap_or("(无可推荐替代)"),
    })
    .to_string()
}

/// Tauri command: panel approve/deny button calls this. Decision string must
/// be exactly `"approve"` or `"deny"` — anything else is rejected with a
/// stable error message so the panel can surface it.
#[tauri::command]
pub fn submit_tool_review(
    review_id: String,
    decision: String,
    registry: tauri::State<'_, ToolReviewRegistryStore>,
) -> Result<(), String> {
    let parsed = match decision.as_str() {
        "approve" => ToolReviewDecision::Approve,
        "deny" => ToolReviewDecision::Deny,
        other => {
            return Err(format!(
                "unknown decision '{}': must be approve|deny",
                other
            ))
        }
    };
    registry.submit(&review_id, parsed)
}

/// Tauri command: panel polls this (or reads from DebugSnapshot) to render
/// the queue. Returns the same Vec the snapshot field carries; exposed
/// separately for callers that don't want the full snapshot payload.
#[tauri::command]
pub fn list_pending_tool_reviews(
    registry: tauri::State<'_, ToolReviewRegistryStore>,
) -> Vec<PendingToolReview> {
    registry.snapshot()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_reasons(s: &str) -> Vec<String> {
        vec![s.to_string()]
    }

    #[test]
    fn register_returns_unique_ids_and_snapshot_contains_request() {
        let r = ToolReviewRegistry::new();
        let (id1, _rx1) = r.register("bash", "{}", "p1", &make_reasons("shell"), None);
        let (id2, _rx2) = r.register(
            "write_file",
            "{}",
            "p2",
            &make_reasons("write"),
            Some("edit"),
        );
        assert_ne!(id1, id2);
        let snap = r.snapshot();
        assert_eq!(snap.len(), 2);
        assert!(snap
            .iter()
            .any(|p| p.review_id == id1 && p.tool_name == "bash"));
        let wf = snap
            .iter()
            .find(|p| p.review_id == id2)
            .expect("write_file pending entry");
        assert_eq!(wf.safe_alternative.as_deref(), Some("edit"));
        assert_eq!(wf.purpose, "p2");
    }

    #[tokio::test]
    async fn submit_resolves_the_awaited_receiver() {
        let r = Arc::new(ToolReviewRegistry::new());
        let (id, rx) = r.register("bash", "{}", "p", &make_reasons("shell"), None);
        let r2 = r.clone();
        let id2 = id.clone();
        // simulate panel clicking Approve in another task
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            r2.submit(&id2, ToolReviewDecision::Approve).unwrap();
        });
        let decision = rx.await.expect("should resolve");
        assert_eq!(decision, ToolReviewDecision::Approve);
        // Successful submit removes the entry from the registry.
        assert!(r.snapshot().is_empty(), "resolved entry must be cleaned up");
    }

    #[test]
    fn submit_unknown_id_returns_error() {
        let r = ToolReviewRegistry::new();
        let err = r
            .submit("tr-999", ToolReviewDecision::Approve)
            .expect_err("unknown id must error");
        assert!(err.contains("not pending"));
    }

    #[test]
    fn cancel_removes_pending_entry() {
        let r = ToolReviewRegistry::new();
        let (id, _rx) = r.register("bash", "{}", "p", &make_reasons("shell"), None);
        assert_eq!(r.snapshot().len(), 1);
        r.cancel(&id);
        assert!(r.snapshot().is_empty());
    }

    #[test]
    fn snapshot_is_sorted_oldest_first() {
        let r = ToolReviewRegistry::new();
        let (id1, _) = r.register("a", "{}", "p", &[], None);
        // Sleep a hair so the timestamp string differs.
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let (id2, _) = r.register("b", "{}", "p", &[], None);
        let snap = r.snapshot();
        assert_eq!(snap.len(), 2);
        assert_eq!(snap[0].review_id, id1, "oldest first");
        assert_eq!(snap[1].review_id, id2);
    }

    #[test]
    fn denied_result_json_carries_reason_and_alt() {
        let s = denied_result_json("user denied", Some("use edit_file"));
        let v: serde_json::Value = serde_json::from_str(&s).expect("must be valid JSON");
        assert_eq!(
            v["error"].as_str(),
            Some("tool call denied by human review")
        );
        assert_eq!(v["reason"].as_str(), Some("user denied"));
        assert_eq!(v["safe_alternative"].as_str(), Some("use edit_file"));
    }

    #[test]
    fn denied_result_json_handles_quotes_safely() {
        // Reason or alt with embedded quotes must not break JSON. serde handles this.
        let s = denied_result_json("contains \"quotes\"", None);
        let v: serde_json::Value = serde_json::from_str(&s).expect("must be valid JSON");
        assert_eq!(v["reason"].as_str(), Some("contains \"quotes\""));
        assert_eq!(v["safe_alternative"].as_str(), Some("(无可推荐替代)"));
    }

    #[test]
    fn timeout_result_json_mentions_window_and_default_alt() {
        let s = timeout_result_json(None);
        let v: serde_json::Value = serde_json::from_str(&s).expect("must be valid JSON");
        let reason = v["reason"].as_str().unwrap();
        assert!(reason.contains("60") || reason.contains(&REVIEW_TIMEOUT_SECONDS.to_string()));
        assert!(reason.contains("拒绝"), "must signal denial-by-default");
    }

    #[test]
    fn timeout_constant_is_one_minute() {
        assert_eq!(REVIEW_TIMEOUT_SECONDS, 60);
    }

    #[test]
    fn record_review_outcome_pushes_decision_with_id_and_tool() {
        // Iter R2: outcomes (approve / deny / timeout) must land in the same
        // ring buffer the proactive Spoke / Silent / Skip entries use, so the
        // panel "decisions" view is one timeline. Pin the kind strings + the
        // reason format so future panel parsers don't drift.
        use crate::decision_log::DecisionLog;
        let log = DecisionLog::new();
        record_review_outcome(&log, KIND_REVIEW_APPROVE, "tr-1", "bash");
        record_review_outcome(&log, KIND_REVIEW_DENY, "tr-2", "write_file");
        record_review_outcome(&log, KIND_REVIEW_TIMEOUT, "tr-3", "memory_edit");
        let snap = log.snapshot();
        assert_eq!(snap.len(), 3);
        assert_eq!(snap[0].kind, "ToolReviewApprove");
        assert_eq!(snap[0].reason, "tr-1 bash");
        assert_eq!(snap[1].kind, "ToolReviewDeny");
        assert_eq!(snap[1].reason, "tr-2 write_file");
        assert_eq!(snap[2].kind, "ToolReviewTimeout");
        assert_eq!(snap[2].reason, "tr-3 memory_edit");
    }
}
