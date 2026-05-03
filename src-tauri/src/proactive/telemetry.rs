//! Proactive-loop telemetry: in-memory stashes for panel observability +
//! decision-log + counter recording (Iter QG5e — final QG5 slice).
//!
//! Two clusters in one module:
//! 1. **Static stashes** (Iter E1-E4 / R1) — per-process Mutex<Option<...>>
//!    holding the last prompt / reply / timestamp / tool list / a small ring
//!    buffer of recent turns. Panel modal uses Tauri commands here to
//!    surface "what did the pet just see/say?" without log scraping.
//! 2. **Outcome recorder** (Iter QG3) — `record_proactive_outcome` bumps
//!    counter atomics + pushes a decision-log entry per turn. Both the
//!    background loop and `trigger_proactive_turn` (manual fire) share this
//!    so panel metrics stay consistent across paths.
//!
//! Plus two helpers (`chatty_mode_tag`, `append_outcome_tag`) used by the
//! recorder and the gate-side dispatch tagging.
//!
//! `ProactiveTurnOutcome` (the return type of `run_proactive_turn`)
//! intentionally stays in `proactive.rs` — it's the orchestrator's data
//! type, not telemetry's. We import it via `super::ProactiveTurnOutcome`.
//!
//! Public surface preserved via glob `pub use self::telemetry::*` at the
//! top of `proactive.rs`.

use super::ProactiveTurnOutcome;

// ---- Iter E1-E4 / R1 static stashes ----------------------------------------

/// Iter E1: stash for the most recently constructed proactive prompt — the full
/// system message the LLM saw on the last turn. Cleared on process restart;
/// no persistent backing. Used by `get_last_proactive_prompt` so the panel can
/// expose "what did the pet see right before deciding to speak / stay silent?"
/// without scraping debug logs.
pub static LAST_PROACTIVE_PROMPT: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Iter E2: companion to LAST_PROACTIVE_PROMPT — the raw LLM reply for the same
/// turn (or `<silent>` when the model chose silence). Pair "in" + "out" lets the
/// panel show the full request/response loop without log scraping.
pub static LAST_PROACTIVE_REPLY: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Iter E3: ISO-8601 local timestamp of the most recent proactive turn — when
/// the prompt above was constructed. Lets the panel show "this run was 12
/// minutes ago" so users can tell whether the cached pair is fresh or stale.
pub static LAST_PROACTIVE_TIMESTAMP: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Iter R1: dedup guard so the previous proactive turn is classified into
/// `feedback_history.log` exactly once. Holds the LAST_PROACTIVE_TIMESTAMP
/// value of the last turn we already classified — when the next turn fires,
/// if this matches, we skip; otherwise classify + update.
pub static LAST_FEEDBACK_RECORDED_FOR: std::sync::Mutex<Option<String>> =
    std::sync::Mutex::new(None);

/// Iter E3: distinct tool names the LLM called during the most recent turn
/// (e.g. ["get_active_window", "memory_edit"]). Empty Vec when the turn ran but
/// invoked no tools. Surfacing this answers "did the LLM look at the
/// environment / write to memory this round?" — a chip-style summary in the
/// modal complements the prompt+reply text.
pub static LAST_PROACTIVE_TOOLS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// Iter E4: ring buffer of the last N completed proactive turns. Pushed once
/// per turn at the end (after the LLM has replied). Panel uses it to navigate
/// "prev/next" between recent turns and compare prompt deltas across runs —
/// useful when iterating on prompt rules and wanting to see "did this rule
/// fire across the last 3 turns or just once?".
///
/// Cap intentionally small (5) — UI shows them one at a time, more would
/// crowd the navigator and inflate process memory for diminishing return.
pub const PROACTIVE_TURN_HISTORY_CAP: usize = 5;

#[derive(Clone, serde::Serialize)]
pub struct TurnRecord {
    pub timestamp: String,
    pub prompt: String,
    pub reply: String,
    pub tools_used: Vec<String>,
    /// Iter R25: classification of this turn's outcome — `"spoke"` when the
    /// LLM produced a non-silent reply, `"silent"` when it returned empty
    /// or contained `SILENT_MARKER`. Lets the panel modal label each turn
    /// in the ring buffer (E4) so users can see at a glance "in the last
    /// 5 turns I went silent 3 times" without parsing reply text.
    /// Errors don't reach this path — they short-circuit before TurnRecord
    /// is pushed — so the variant set is finite at two values.
    #[serde(default)]
    pub outcome: String,
}

pub static LAST_PROACTIVE_TURNS: std::sync::Mutex<std::collections::VecDeque<TurnRecord>> =
    std::sync::Mutex::new(std::collections::VecDeque::new());

/// Iter E3: combined accessor for the panel modal — timestamp + tool list in
/// one shot so the frontend doesn't need three round-trips. Empty values when
/// no turn has fired.
#[derive(serde::Serialize)]
pub struct ProactiveTurnMeta {
    pub timestamp: String,
    pub tools_used: Vec<String>,
}

/// Tauri command — return the most recently built proactive prompt, or empty
/// string if none has been built yet (fresh process, never fired).
#[tauri::command]
pub fn get_last_proactive_prompt() -> String {
    LAST_PROACTIVE_PROMPT
        .lock()
        .ok()
        .and_then(|g| g.clone())
        .unwrap_or_default()
}

/// Tauri command — return the LLM's raw reply text from the last proactive turn.
/// Empty string when no turn has fired or when the reply is unavailable.
#[tauri::command]
pub fn get_last_proactive_reply() -> String {
    LAST_PROACTIVE_REPLY
        .lock()
        .ok()
        .and_then(|g| g.clone())
        .unwrap_or_default()
}

/// Iter E4 Tauri command — return the ring buffer of recent turns, newest first.
/// Empty Vec when nothing has fired since process start. Each entry is a complete
/// turn record (prompt + reply + timestamp + tools) so the panel can navigate
/// without a second IPC for sub-fields.
#[tauri::command]
pub fn get_recent_proactive_turns() -> Vec<TurnRecord> {
    LAST_PROACTIVE_TURNS
        .lock()
        .map(|g| {
            // Reverse so index 0 = newest, matching the panel's prev/next intuition.
            let mut out: Vec<TurnRecord> = g.iter().cloned().collect();
            out.reverse();
            out
        })
        .unwrap_or_default()
}

#[tauri::command]
pub fn get_last_proactive_meta() -> ProactiveTurnMeta {
    let timestamp = LAST_PROACTIVE_TIMESTAMP
        .lock()
        .ok()
        .and_then(|g| g.clone())
        .unwrap_or_default();
    let tools_used = LAST_PROACTIVE_TOOLS
        .lock()
        .map(|g| g.clone())
        .unwrap_or_default();
    ProactiveTurnMeta {
        timestamp,
        tools_used,
    }
}

// ---- Outcome recorder + reason-string helpers ------------------------------

/// Format a compact "chatty mode" annotation for the decision log, e.g. `chatty=5/5`.
/// Returns `None` when the threshold is 0 (rule disabled) or when today's count is below
/// it — in those cases tagging would be noise. Pure / testable so we don't drift between
/// the gate-side push and the post-LLM push.
pub fn chatty_mode_tag(today: u64, threshold: u64) -> Option<String> {
    if threshold == 0 || today < threshold {
        None
    } else {
        Some(format!("chatty={}/{}", today, threshold))
    }
}

/// Append a comma-separated tag onto a decision-log reason string. Centralizes the
/// `", "` separator so reasons stay parseable by the panel and so multiple call sites
/// can't drift on the format. The `"-"` placeholder is treated as empty: passing it as
/// a starting reason then appending tags overwrites the dash.
pub fn append_outcome_tag(reason: &mut String, tag: &str) {
    if !reason.is_empty() && reason != "-" {
        reason.push_str(", ");
    } else if reason == "-" {
        reason.clear();
    }
    reason.push_str(tag);
}

/// Centralizes the post-LLM telemetry side effects of a proactive turn so manual
/// triggers (panel "fire now") and the background loop both bump the same counter
/// set and produce the same decision-log entries. Three counters and one log entry
/// are touched per outcome:
///
/// - `counters.llm_outcome.{spoke,silent,error}` — atomic bump of exactly one bucket
/// - `counters.env_tool.record_spoke(&tools)` — only on the Spoke path
/// - `decisions.push(kind, reason)` — `Spoke` / `LlmSilent` / `LlmError`
///
/// `source` is one of `"loop"` / `"manual"` and is embedded as `source=X` in the
/// decision-log reason so the panel can distinguish manual triggers from genuine
/// loop dispatches without inflating the loop's outcome counters with phantom data.
///
/// Note: `prompt_tilt.record_dispatch` is intentionally NOT done here. Tilt depends on
/// the active rule labels, which only the loop computes (the manual trigger bypasses
/// gates and so has no labels to classify). Skipping keeps tilt stats meaningful.
pub fn record_proactive_outcome(
    counters: &crate::commands::debug::ProcessCounters,
    decisions: &crate::decision_log::DecisionLog,
    source: &str,
    chatty_part: &str,
    rules_tag: Option<&str>,
    outcome: &Result<ProactiveTurnOutcome, String>,
) {
    let outcome_counters = &counters.llm_outcome;
    let env_tool_counters = &counters.env_tool;
    let source_tag = format!("source={}", source);
    match outcome {
        Ok(o) if o.reply.is_some() => {
            outcome_counters
                .spoke
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            env_tool_counters.record_spoke(&o.tools);
            let mut reason = chatty_part.to_string();
            append_outcome_tag(&mut reason, &source_tag);
            if let Some(t) = rules_tag {
                append_outcome_tag(&mut reason, t);
            }
            if !o.tools.is_empty() {
                let tools_tag = format!("tools={}", o.tools.join("+"));
                append_outcome_tag(&mut reason, &tools_tag);
            }
            decisions.push("Spoke", reason);
        }
        Ok(_) => {
            outcome_counters
                .silent
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let mut reason = chatty_part.to_string();
            append_outcome_tag(&mut reason, &source_tag);
            if let Some(t) = rules_tag {
                append_outcome_tag(&mut reason, t);
            }
            decisions.push("LlmSilent", reason);
        }
        Err(e) => {
            outcome_counters
                .error
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let mut tail = chatty_part.to_string();
            append_outcome_tag(&mut tail, &source_tag);
            if let Some(t) = rules_tag {
                append_outcome_tag(&mut tail, t);
            }
            decisions.push("LlmError", format!("{} ({})", e, tail));
        }
    }
}
