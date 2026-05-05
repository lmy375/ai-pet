//! Ring buffer of recent proactive-loop decisions.
//!
//! Each tick of the proactive engine produces a `LoopAction` that the spawn loop dispatches
//! on. This module records the last N actions (kind + reason + timestamp) so the panel
//! UI can answer "why didn't the pet say anything in the last 10 minutes?" without making
//! the user grep through the log buffer.
//!
//! Bounded to `CAPACITY` entries — a ring buffer, not a full audit log; the existing
//! `LogStore` already keeps detailed per-line history for that.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use serde::Serialize;

/// Cap on how many decisions we keep in the panel's "recent" view. Bumped from 10 → 16
/// after Iter 78 started pushing two entries per Run (gate + LLM outcome): 16 keeps the
/// effective gate-decision window comparable to before while letting the new
/// Run+outcome pairing fit. Still small enough to scan at a glance in the panel strip.
pub const CAPACITY: usize = 16;

#[derive(Clone, Serialize)]
pub struct ProactiveDecision {
    /// Local time the decision was made.
    pub timestamp: String,
    /// One of "Silent" | "Skip" | "Run" — matches the LoopAction variant.
    pub kind: String,
    /// Human-readable reason: silent gate name (`disabled` / `quiet_hours` / ...), the
    /// Skip's full message, or the Run's idle stats.
    pub reason: String,
}

pub struct DecisionLog {
    buf: Mutex<VecDeque<ProactiveDecision>>,
}

impl DecisionLog {
    pub fn new() -> Self {
        Self {
            buf: Mutex::new(VecDeque::with_capacity(CAPACITY)),
        }
    }

    /// Append a decision; oldest is dropped once we exceed `CAPACITY`.
    pub fn push(&self, kind: &str, reason: String) {
        let entry = ProactiveDecision {
            timestamp: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            kind: kind.to_string(),
            reason,
        };
        let mut g = self.buf.lock().unwrap();
        g.push_back(entry);
        while g.len() > CAPACITY {
            g.pop_front();
        }
    }

    /// Snapshot all currently-held entries oldest first.
    pub fn snapshot(&self) -> Vec<ProactiveDecision> {
        self.buf.lock().unwrap().iter().cloned().collect()
    }

    /// 清空 ring buffer。给调试面板"清空"按钮用 —— 调 prompt 时把无关历史
    /// 抹掉，让用户后续 push 的几条从顶部展示，定位新条目省心。
    pub fn clear(&self) {
        if let Ok(mut g) = self.buf.lock() {
            g.clear();
        }
    }
}

pub type DecisionLogStore = Arc<DecisionLog>;

pub fn new_decision_log() -> DecisionLogStore {
    Arc::new(DecisionLog::new())
}

#[tauri::command]
pub fn get_proactive_decisions(
    store: tauri::State<'_, DecisionLogStore>,
) -> Vec<ProactiveDecision> {
    store.snapshot()
}

/// 清空 in-memory 决策 ring buffer。调试面板「清空」按钮调用入口。
/// 失误清空只丢 16 条 debug 痕迹，零风险，不需二次确认。
#[tauri::command]
pub fn clear_proactive_decisions(store: tauri::State<'_, DecisionLogStore>) {
    store.clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_snapshot_starts_empty() {
        let log = DecisionLog::new();
        assert!(log.snapshot().is_empty());
    }

    #[test]
    fn snapshot_preserves_chronological_order() {
        let log = DecisionLog::new();
        log.push("Silent", "disabled".into());
        log.push("Skip", "cooldown".into());
        log.push("Run", "idle=900".into());
        let snap = log.snapshot();
        assert_eq!(snap.len(), 3);
        assert_eq!(snap[0].kind, "Silent");
        assert_eq!(snap[2].kind, "Run");
    }

    #[test]
    fn buffer_drops_oldest_at_capacity() {
        let log = DecisionLog::new();
        for i in 0..(CAPACITY + 5) {
            log.push("Silent", format!("entry_{}", i));
        }
        let snap = log.snapshot();
        assert_eq!(snap.len(), CAPACITY);
        // The first 5 should have been dropped; oldest in window = entry_5.
        assert_eq!(snap[0].reason, "entry_5");
        assert_eq!(snap[CAPACITY - 1].reason, format!("entry_{}", CAPACITY + 4));
    }

    #[test]
    fn clear_drains_the_buffer() {
        let log = DecisionLog::new();
        log.push("Silent", "a".into());
        log.push("Run", "b".into());
        assert_eq!(log.snapshot().len(), 2);
        log.clear();
        assert!(log.snapshot().is_empty());
        // 清空后仍可继续 push（mutex 没坏）
        log.push("Skip", "c".into());
        assert_eq!(log.snapshot().len(), 1);
    }
}
