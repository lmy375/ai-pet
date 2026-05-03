use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tauri::State;

/// Maximum number of in-memory log lines retained in `LogStore`. Older lines are dropped
/// when the buffer overflows. 5000 lines ≈ several hundred LLM turns at typical 10–30
/// lines per turn — comfortably more than a session's worth, but still bounded so a
/// long-running pet doesn't slowly leak. The on-disk app.log is not capped here.
pub const MAX_LOG_LINES: usize = 5000;

#[derive(Clone)]
pub struct LogStore(pub Arc<Mutex<Vec<String>>>);

/// Process-wide cumulative cache counters. Incremented at the end of each LLM turn that
/// fired any cacheable tool, so the panel's hit-ratio display stays accurate even after
/// the in-memory log buffer wraps around its size cap.
#[derive(Default)]
pub struct CacheCounters {
    pub turns: AtomicU64,
    pub hits: AtomicU64,
    pub calls: AtomicU64,
}

// Test-only helper that builds an isolated CacheCounters Arc. Production code accesses
// counters via ProcessCounters; tests still want to exercise the inner struct directly
// without the wrapper layer.
#[cfg(test)]
pub type CacheCountersStore = Arc<CacheCounters>;

#[cfg(test)]
pub fn new_cache_counters() -> CacheCountersStore {
    Arc::new(CacheCounters::default())
}

/// Per-process counters tracking how often the LLM cooperated with the
/// `[motion: X]` prefix convention. Bumped every time `read_mood_for_event` is invoked
/// (i.e. after every successful LLM turn that produced a mood snapshot). Lets the panel
/// render an honest "is the model following the format?" indicator without grepping
/// the log buffer.
#[derive(Default)]
pub struct MoodTagCounters {
    /// Mood was present and started with a valid `[motion: X]` prefix.
    pub with_tag: AtomicU64,
    /// Mood was present but missing/invalid prefix — frontend falls back to keyword match.
    pub without_tag: AtomicU64,
    /// No mood entry yet (typically just after install / first proactive turn).
    pub no_mood: AtomicU64,
}

// Test-only — see new_cache_counters above.
#[cfg(test)]
pub type MoodTagCountersStore = Arc<MoodTagCounters>;

#[cfg(test)]
pub fn new_mood_tag_counters() -> MoodTagCountersStore {
    Arc::new(MoodTagCounters::default())
}

/// Counters tracking the LLM-side outcome of every dispatched proactive Run. Lets the
/// panel show "LLM 沉默率 X/Y" — the share of gate-cleared turns where the model still
/// returned the silent marker. Spikes here usually mean the prompt is too restrictive
/// (e.g. chatty_day_threshold too low) and the user can tune it accordingly. Distinct
/// from gate-side stats because gate decisions never reach the LLM at all.
#[derive(Default)]
pub struct LlmOutcomeCounters {
    /// LLM produced a non-silent reply (the pet actually spoke).
    pub spoke: AtomicU64,
    /// LLM returned the silent marker or empty reply.
    pub silent: AtomicU64,
    /// LLM call errored out (network / API failure / parse error).
    pub error: AtomicU64,
}

#[cfg(test)]
pub type LlmOutcomeCountersStore = Arc<LlmOutcomeCounters>;

#[cfg(test)]
pub fn new_llm_outcome_counters() -> LlmOutcomeCountersStore {
    Arc::new(LlmOutcomeCounters::default())
}

/// Per-tool atomic counters tracking how often the LLM consulted each environment-aware
/// tool before speaking. `spoke_total` denominator + `spoke_with_any` numerator give the
/// "环境感知率" — what fraction of Spoke turns hit at least one env tool. Per-tool sub
/// counts let the tooltip show "weather 3 / window 7 / events 0" so prompt-tuners can
/// see *which* tool the model is ignoring.
#[derive(Default)]
pub struct EnvToolCounters {
    /// Total Spoke turns counted (denominator).
    pub spoke_total: AtomicU64,
    /// Spoke turns where the LLM invoked at least one env-awareness tool.
    pub spoke_with_any: AtomicU64,
    pub active_window: AtomicU64,
    pub weather: AtomicU64,
    pub upcoming_events: AtomicU64,
    pub memory_search: AtomicU64,
}

impl EnvToolCounters {
    /// Bump per-tool counters and the spoke aggregates given the list of tool names the
    /// LLM invoked this turn. Unrecognized tools (mutating ones, MCP, anything outside
    /// the env-aware whitelist) are ignored — the rate denominator only counts Spoke
    /// turns regardless. Pure-ish (touches atomics only) so caller doesn't need to clone
    /// the names list out.
    pub fn record_spoke(&self, tools: &[String]) {
        self.spoke_total.fetch_add(1, Ordering::Relaxed);
        let mut any = false;
        for name in tools {
            match name.as_str() {
                "get_active_window" => {
                    self.active_window.fetch_add(1, Ordering::Relaxed);
                    any = true;
                }
                "get_weather" => {
                    self.weather.fetch_add(1, Ordering::Relaxed);
                    any = true;
                }
                "get_upcoming_events" => {
                    self.upcoming_events.fetch_add(1, Ordering::Relaxed);
                    any = true;
                }
                "memory_search" => {
                    self.memory_search.fetch_add(1, Ordering::Relaxed);
                    any = true;
                }
                _ => {}
            }
        }
        if any {
            self.spoke_with_any.fetch_add(1, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
pub type EnvToolCountersStore = Arc<EnvToolCounters>;

#[cfg(test)]
pub fn new_env_tool_counters() -> EnvToolCountersStore {
    Arc::new(EnvToolCounters::default())
}

/// Counters tracking the prompt's "tilt" each time a proactive Run dispatches. For each
/// Run, we look at the active rule labels and bump exactly one of four atomic buckets:
///
/// - `restraint_dominant`: more restraint rules than engagement rules (e.g., chatty +
///   icebreaker active, no engagement-window) → prompt was nudging the pet to be quiet.
/// - `engagement_dominant`: more engagement rules than restraint rules → prompt was
///   nudging the pet to open up.
/// - `balanced`: equal non-zero counts of both → mixed signals.
/// - `neutral`: zero of both restraint and engagement (only instructional/corrective
///   rules active, or no contextual rules at all) → prompt was directionally inert.
///
/// Sums across these four atomics equal the total number of dispatched Runs since the
/// last reset. Lets the panel show "today the pet's prompt has been 60% restraint,
/// 30% balanced, 10% engagement" — useful for diagnosing why the pet feels "off"
/// even when individual gates look fine.
#[derive(Default)]
pub struct PromptTiltCounters {
    pub restraint_dominant: AtomicU64,
    pub engagement_dominant: AtomicU64,
    pub balanced: AtomicU64,
    pub neutral: AtomicU64,
}

impl PromptTiltCounters {
    /// Classify the active labels for one dispatched Run and bump the matching bucket.
    /// Mirrors the panel's badge-color logic in PanelDebug.tsx so the long-running
    /// stats aggregate the same notion of tilt the user sees in the closed badge.
    pub fn record_dispatch(&self, labels: &[&str]) {
        let mut restraint = 0u64;
        let mut engagement = 0u64;
        for label in labels {
            match *label {
                // Restraint rules: tell the pet to stay quiet/brief.
                "wake-back" | "pre-quiet" | "icebreaker" | "chatty" => restraint += 1,
                // Engagement rules: tell the pet to open up.
                "engagement-window" | "long-idle-no-restraint" => engagement += 1,
                // Anything else (corrective / instructional) doesn't tilt the prompt
                // toward quiet vs active behavior — leave it out of the comparison.
                _ => {}
            }
        }
        let bucket = if restraint > engagement {
            &self.restraint_dominant
        } else if engagement > restraint {
            &self.engagement_dominant
        } else if restraint + engagement == 0 {
            &self.neutral
        } else {
            &self.balanced
        };
        bucket.fetch_add(1, Ordering::Relaxed);
    }
}

#[cfg(test)]
pub type PromptTiltCountersStore = Arc<PromptTiltCounters>;

#[cfg(test)]
pub fn new_prompt_tilt_counters() -> PromptTiltCountersStore {
    Arc::new(PromptTiltCounters::default())
}

/// Container for every per-process counter group the panel surfaces. Bundling them as a
/// single Tauri State keeps `ToolContext` stable when we add a new metric: one field, one
/// `app.state::<ProcessCountersStore>()` lookup, no plumbing through 5 callsites and
/// reconnect paths each time. Add a new sub-struct here and register one Tauri command
/// that reads it; everything else stays put.
#[derive(Default)]
pub struct ProcessCounters {
    pub cache: CacheCounters,
    pub mood_tag: MoodTagCounters,
    pub llm_outcome: LlmOutcomeCounters,
    pub env_tool: EnvToolCounters,
    pub prompt_tilt: PromptTiltCounters,
}

pub type ProcessCountersStore = Arc<ProcessCounters>;

pub fn new_process_counters() -> ProcessCountersStore {
    Arc::new(ProcessCounters::default())
}

/// Return the log directory: ~/.config/pet/logs/
pub fn log_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config/pet/logs")
}

/// Append a line to a file (create if missing). Errors are silently ignored.
pub fn append_to_file(path: &std::path::Path, line: &str) {
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{}", line);
    }
}

/// Write one formatted log line to both the in-memory store and app.log.
pub fn write_log(store: &Arc<Mutex<Vec<String>>>, message: &str) {
    let ts = chrono::Local::now()
        .format("%Y-%m-%d %H:%M:%S%.3f")
        .to_string();
    let line = format!("[{}] {}", ts, message);

    // In-memory
    {
        let mut logs = store.lock().unwrap();
        logs.push(line.clone());
        if logs.len() > MAX_LOG_LINES {
            let drain = logs.len() - MAX_LOG_LINES;
            logs.drain(0..drain);
        }
    }

    // File
    append_to_file(&log_dir().join("app.log"), &line);
}

/// Append a JSON-Lines entry to llm.log with timing info.
#[allow(clippy::too_many_arguments)] // each timing field is independently captured at the call site
pub fn write_llm_log(
    round: usize,
    request: &serde_json::Value,
    response_text: &str,
    tool_calls: &[serde_json::Value],
    request_time: &str,
    first_token_time: Option<&str>,
    done_time: &str,
    first_token_latency_ms: Option<i64>,
    total_latency_ms: i64,
) {
    let entry = serde_json::json!({
        "round": round,
        "request_time": request_time,
        "first_token_time": first_token_time,
        "done_time": done_time,
        "first_token_latency_ms": first_token_latency_ms,
        "total_latency_ms": total_latency_ms,
        "request": request,
        "response": {
            "text": response_text,
            "tool_calls": tool_calls,
        }
    });
    append_to_file(&log_dir().join("llm.log"), &entry.to_string());
}

/// Read llm.log and return the last N entries (JSON strings).
#[tauri::command]
pub fn get_llm_logs(limit: Option<usize>) -> Vec<String> {
    let path = log_dir().join("llm.log");
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return vec![],
    };
    let lines: Vec<String> = content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.to_string())
        .collect();
    let limit = limit.unwrap_or(100);
    if lines.len() > limit {
        lines[lines.len() - limit..].to_vec()
    } else {
        lines
    }
}

#[tauri::command]
pub fn get_logs(store: State<'_, LogStore>) -> Vec<String> {
    store.0.lock().unwrap().clone()
}

#[tauri::command]
pub fn append_log(store: State<'_, LogStore>, message: String) {
    write_log(&store.0, &message);
}

#[tauri::command]
pub fn clear_logs(store: State<'_, LogStore>) {
    store.0.lock().unwrap().clear();
}

/// Aggregated tool-cache statistics derived from "Tool cache summary" lines emitted at
/// the end of each successful LLM turn. Each summary line records one turn; the totals
/// sum every turn this session has produced so the panel can render an honest cumulative
/// hit ratio without re-implementing the parsing in TS.
#[derive(serde::Serialize)]
pub struct CacheStats {
    /// Number of "Tool cache summary" lines parsed (≈ pipeline turns that fired any
    /// cacheable tool).
    pub turns: u64,
    /// Sum of cache hits across all parsed summaries.
    pub total_hits: u64,
    /// Sum of total cacheable calls (hits + misses) across all parsed summaries.
    pub total_calls: u64,
}

impl CacheStats {
    /// Snapshot the cache atomics into a serializable struct. Shared by the public
    /// Tauri command and the bundled `get_debug_snapshot` aggregator (Iter QG6).
    pub fn from_counters(c: &ProcessCounters) -> Self {
        Self {
            turns: c.cache.turns.load(Ordering::Relaxed),
            total_hits: c.cache.hits.load(Ordering::Relaxed),
            total_calls: c.cache.calls.load(Ordering::Relaxed),
        }
    }
}

#[tauri::command]
pub fn get_cache_stats(counters: State<'_, ProcessCountersStore>) -> CacheStats {
    CacheStats::from_counters(counters.inner())
}

/// Zero out the process-wide cache counters. Mirrors `clear_logs` for the stats panel —
/// useful when the user wants to measure hit ratio over a fresh window.
#[tauri::command]
pub fn reset_cache_stats(counters: State<'_, ProcessCountersStore>) {
    counters.cache.turns.store(0, Ordering::Relaxed);
    counters.cache.hits.store(0, Ordering::Relaxed);
    counters.cache.calls.store(0, Ordering::Relaxed);
}

#[derive(serde::Serialize)]
pub struct MoodTagStats {
    pub with_tag: u64,
    pub without_tag: u64,
    pub no_mood: u64,
}

impl MoodTagStats {
    pub fn from_counters(c: &ProcessCounters) -> Self {
        Self {
            with_tag: c.mood_tag.with_tag.load(Ordering::Relaxed),
            without_tag: c.mood_tag.without_tag.load(Ordering::Relaxed),
            no_mood: c.mood_tag.no_mood.load(Ordering::Relaxed),
        }
    }
}

#[tauri::command]
pub fn get_mood_tag_stats(counters: State<'_, ProcessCountersStore>) -> MoodTagStats {
    MoodTagStats::from_counters(counters.inner())
}

/// Zero out the process-wide mood-tag counters. Mirrors `reset_cache_stats` so panel users
/// can measure adherence over a fresh window (e.g. after tweaking the prompt to see if
/// the model improves).
#[tauri::command]
pub fn reset_mood_tag_stats(counters: State<'_, ProcessCountersStore>) {
    counters.mood_tag.with_tag.store(0, Ordering::Relaxed);
    counters.mood_tag.without_tag.store(0, Ordering::Relaxed);
    counters.mood_tag.no_mood.store(0, Ordering::Relaxed);
}

#[derive(serde::Serialize)]
pub struct LlmOutcomeStats {
    pub spoke: u64,
    pub silent: u64,
    pub error: u64,
}

impl LlmOutcomeStats {
    pub fn from_counters(c: &ProcessCounters) -> Self {
        Self {
            spoke: c.llm_outcome.spoke.load(Ordering::Relaxed),
            silent: c.llm_outcome.silent.load(Ordering::Relaxed),
            error: c.llm_outcome.error.load(Ordering::Relaxed),
        }
    }
}

#[tauri::command]
pub fn get_llm_outcome_stats(counters: State<'_, ProcessCountersStore>) -> LlmOutcomeStats {
    LlmOutcomeStats::from_counters(counters.inner())
}

/// Zero out the process-wide LLM-outcome counters. Mirrors the cache/mood-tag reset
/// commands so the user can measure rejection ratios over a fresh window after tweaking
/// the prompt or chatty_day_threshold.
#[tauri::command]
pub fn reset_llm_outcome_stats(counters: State<'_, ProcessCountersStore>) {
    counters.llm_outcome.spoke.store(0, Ordering::Relaxed);
    counters.llm_outcome.silent.store(0, Ordering::Relaxed);
    counters.llm_outcome.error.store(0, Ordering::Relaxed);
}

#[derive(serde::Serialize)]
pub struct EnvToolStats {
    pub spoke_total: u64,
    pub spoke_with_any: u64,
    pub active_window: u64,
    pub weather: u64,
    pub upcoming_events: u64,
    pub memory_search: u64,
}

impl EnvToolStats {
    pub fn from_counters(c: &ProcessCounters) -> Self {
        let e = &c.env_tool;
        Self {
            spoke_total: e.spoke_total.load(Ordering::Relaxed),
            spoke_with_any: e.spoke_with_any.load(Ordering::Relaxed),
            active_window: e.active_window.load(Ordering::Relaxed),
            weather: e.weather.load(Ordering::Relaxed),
            upcoming_events: e.upcoming_events.load(Ordering::Relaxed),
            memory_search: e.memory_search.load(Ordering::Relaxed),
        }
    }
}

#[tauri::command]
pub fn get_env_tool_stats(counters: State<'_, ProcessCountersStore>) -> EnvToolStats {
    EnvToolStats::from_counters(counters.inner())
}

/// Zero out the env-tool counters. Useful after changing the proactive prompt to see if
/// LLM tool-calling behavior shifts.
#[tauri::command]
pub fn reset_env_tool_stats(counters: State<'_, ProcessCountersStore>) {
    let e = &counters.env_tool;
    e.spoke_total.store(0, Ordering::Relaxed);
    e.spoke_with_any.store(0, Ordering::Relaxed);
    e.active_window.store(0, Ordering::Relaxed);
    e.weather.store(0, Ordering::Relaxed);
    e.upcoming_events.store(0, Ordering::Relaxed);
    e.memory_search.store(0, Ordering::Relaxed);
}

#[derive(serde::Serialize)]
pub struct PromptTiltStats {
    pub restraint_dominant: u64,
    pub engagement_dominant: u64,
    pub balanced: u64,
    pub neutral: u64,
}

impl PromptTiltStats {
    pub fn from_counters(c: &ProcessCounters) -> Self {
        let p = &c.prompt_tilt;
        Self {
            restraint_dominant: p.restraint_dominant.load(Ordering::Relaxed),
            engagement_dominant: p.engagement_dominant.load(Ordering::Relaxed),
            balanced: p.balanced.load(Ordering::Relaxed),
            neutral: p.neutral.load(Ordering::Relaxed),
        }
    }
}

#[tauri::command]
pub fn get_prompt_tilt_stats(counters: State<'_, ProcessCountersStore>) -> PromptTiltStats {
    PromptTiltStats::from_counters(counters.inner())
}

#[tauri::command]
pub fn reset_prompt_tilt_stats(counters: State<'_, ProcessCountersStore>) {
    let p = &counters.prompt_tilt;
    p.restraint_dominant.store(0, Ordering::Relaxed);
    p.engagement_dominant.store(0, Ordering::Relaxed);
    p.balanced.store(0, Ordering::Relaxed);
    p.neutral.store(0, Ordering::Relaxed);
}

/// Iter QG6: bundle the 15 panel-debug data sources into one Tauri call.
///
/// PanelDebug previously fired 15 independent invokes per second to populate one
/// snapshot. With Tauri's IPC overhead per call (serialize → bridge → deserialize)
/// that's a measurable per-second cost on the panel and proportionally on the main
/// process. This struct + command consolidates them: one IPC round-trip, one
/// deserialize on the JS side, one big object replacing 15 setState calls.
///
/// Each existing `get_X_stats` Tauri command is preserved for any caller (e.g.
/// PanelPersona uses `get_companionship_days`) — only PanelDebug's hot path
/// switches to this aggregator.
#[derive(serde::Serialize)]
pub struct DebugSnapshot {
    pub logs: Vec<String>,
    pub cache_stats: CacheStats,
    pub decisions: Vec<crate::decision_log::ProactiveDecision>,
    pub mood_tag_stats: MoodTagStats,
    pub recent_speeches: Vec<String>,
    pub tone: crate::proactive::ToneSnapshot,
    pub reminders: Vec<crate::proactive::PendingReminder>,
    pub lifetime_speech_count: u64,
    pub today_speech_count: u64,
    pub week_speech_count: u64,
    pub llm_outcome_stats: LlmOutcomeStats,
    pub env_tool_stats: EnvToolStats,
    pub prompt_tilt_stats: PromptTiltStats,
    pub companionship_days: u64,
    pub redaction_stats: crate::redaction::RedactionStats,
    /// Iter TR3: pending high-risk tool calls awaiting human approve / deny.
    /// Frontend renders a modal when this is non-empty.
    pub pending_tool_reviews: Vec<crate::tool_review::PendingToolReview>,
    /// Iter R4: structured ring buffer of recent tool calls (newest first)
    /// surfacing purpose / risk / review status. Frontend renders a
    /// collapsible "工具调用历史" card so prompt tuning doesn't need
    /// raw-app.log grep.
    pub recent_tool_calls: Vec<crate::tool_call_history::ToolCallRecord>,
    /// Iter R6: recent feedback entries (replied / ignored) from
    /// `feedback_history.log`, newest first. Frontend renders a collapsible
    /// "宠物反馈记录" card to expose the R1 capture data and let the user
    /// see whether the pet is "learning" from outcomes.
    pub recent_feedback: Vec<crate::feedback_history::FeedbackEntry>,
}

#[tauri::command]
#[allow(clippy::too_many_arguments)] // each State is independently injected by Tauri
pub async fn get_debug_snapshot(
    log_store: State<'_, LogStore>,
    counters: State<'_, ProcessCountersStore>,
    decisions: State<'_, crate::decision_log::DecisionLogStore>,
    clock: State<'_, crate::proactive::InteractionClockStore>,
    wake: State<'_, crate::wake_detector::WakeDetectorStore>,
    tool_review: State<'_, crate::tool_review::ToolReviewRegistryStore>,
) -> Result<DebugSnapshot, String> {
    let counters_inner = counters.inner();
    let logs = log_store.0.lock().unwrap().clone();
    let cache_stats = CacheStats::from_counters(counters_inner);
    let mood_tag_stats = MoodTagStats::from_counters(counters_inner);
    let llm_outcome_stats = LlmOutcomeStats::from_counters(counters_inner);
    let env_tool_stats = EnvToolStats::from_counters(counters_inner);
    let prompt_tilt_stats = PromptTiltStats::from_counters(counters_inner);
    let decisions_snap = decisions.snapshot();
    let recent_speeches = crate::speech_history::recent_speeches(10).await;
    let tone =
        crate::proactive::build_tone_snapshot(clock.inner(), wake.inner(), counters_inner).await?;
    let reminders = crate::proactive::get_pending_reminders();
    let lifetime_speech_count = crate::speech_history::lifetime_speech_count().await;
    let today_speech_count = crate::speech_history::today_speech_count().await;
    let week_speech_count = crate::speech_history::week_speech_count().await;
    let companionship_days = crate::companionship::companionship_days().await;
    let redaction_stats = crate::redaction::get_redaction_stats();
    let pending_tool_reviews = tool_review.snapshot();
    let recent_tool_calls = crate::tool_call_history::recent_tool_calls();
    // Iter R6: feedback history (oldest-first internally → reverse for panel).
    let mut recent_feedback = crate::feedback_history::recent_feedback(20).await;
    recent_feedback.reverse();
    Ok(DebugSnapshot {
        logs,
        cache_stats,
        decisions: decisions_snap,
        mood_tag_stats,
        recent_speeches,
        tone,
        reminders,
        lifetime_speech_count,
        today_speech_count,
        week_speech_count,
        llm_outcome_stats,
        env_tool_stats,
        prompt_tilt_stats,
        companionship_days,
        redaction_stats,
        pending_tool_reviews,
        recent_tool_calls,
        recent_feedback,
    })
}

#[cfg(test)]
mod tests {
    use super::{new_cache_counters, write_log, MAX_LOG_LINES};
    use std::sync::atomic::Ordering;
    use std::sync::{Arc, Mutex};

    // ---- CacheCounters atomic accumulation ----

    #[test]
    fn cache_counters_default_to_zero() {
        let c = new_cache_counters();
        assert_eq!(c.turns.load(Ordering::Relaxed), 0);
        assert_eq!(c.hits.load(Ordering::Relaxed), 0);
        assert_eq!(c.calls.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn cache_counters_accumulate_independently_of_log_buffer() {
        // The whole point of moving to atomics: even if 5000+ log lines were written and
        // older summaries scrolled off, the counters keep the truth.
        let c = new_cache_counters();
        for _ in 0..3 {
            c.turns.fetch_add(1, Ordering::Relaxed);
            c.hits.fetch_add(2, Ordering::Relaxed);
            c.calls.fetch_add(5, Ordering::Relaxed);
        }
        assert_eq!(c.turns.load(Ordering::Relaxed), 3);
        assert_eq!(c.hits.load(Ordering::Relaxed), 6);
        assert_eq!(c.calls.load(Ordering::Relaxed), 15);
    }

    #[test]
    fn mood_tag_counters_default_to_zero_and_accumulate() {
        let c = super::new_mood_tag_counters();
        assert_eq!(c.with_tag.load(Ordering::Relaxed), 0);
        assert_eq!(c.without_tag.load(Ordering::Relaxed), 0);
        assert_eq!(c.no_mood.load(Ordering::Relaxed), 0);
        c.with_tag.fetch_add(3, Ordering::Relaxed);
        c.without_tag.fetch_add(1, Ordering::Relaxed);
        c.no_mood.fetch_add(2, Ordering::Relaxed);
        assert_eq!(c.with_tag.load(Ordering::Relaxed), 3);
        assert_eq!(c.without_tag.load(Ordering::Relaxed), 1);
        assert_eq!(c.no_mood.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn mood_tag_counters_can_be_reset_to_zero() {
        let c = super::new_mood_tag_counters();
        c.with_tag.fetch_add(5, Ordering::Relaxed);
        c.without_tag.fetch_add(2, Ordering::Relaxed);
        c.no_mood.fetch_add(7, Ordering::Relaxed);
        // Inline what reset_mood_tag_stats does — Tauri State wrapper is plumbing.
        c.with_tag.store(0, Ordering::Relaxed);
        c.without_tag.store(0, Ordering::Relaxed);
        c.no_mood.store(0, Ordering::Relaxed);
        assert_eq!(c.with_tag.load(Ordering::Relaxed), 0);
        assert_eq!(c.without_tag.load(Ordering::Relaxed), 0);
        assert_eq!(c.no_mood.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn llm_outcome_counters_default_to_zero_and_accumulate() {
        let c = super::new_llm_outcome_counters();
        assert_eq!(c.spoke.load(Ordering::Relaxed), 0);
        assert_eq!(c.silent.load(Ordering::Relaxed), 0);
        assert_eq!(c.error.load(Ordering::Relaxed), 0);
        c.spoke.fetch_add(4, Ordering::Relaxed);
        c.silent.fetch_add(2, Ordering::Relaxed);
        c.error.fetch_add(1, Ordering::Relaxed);
        assert_eq!(c.spoke.load(Ordering::Relaxed), 4);
        assert_eq!(c.silent.load(Ordering::Relaxed), 2);
        assert_eq!(c.error.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn llm_outcome_counters_can_be_reset_to_zero() {
        let c = super::new_llm_outcome_counters();
        c.spoke.fetch_add(9, Ordering::Relaxed);
        c.silent.fetch_add(3, Ordering::Relaxed);
        c.error.fetch_add(1, Ordering::Relaxed);
        c.spoke.store(0, Ordering::Relaxed);
        c.silent.store(0, Ordering::Relaxed);
        c.error.store(0, Ordering::Relaxed);
        assert_eq!(c.spoke.load(Ordering::Relaxed), 0);
        assert_eq!(c.silent.load(Ordering::Relaxed), 0);
        assert_eq!(c.error.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn env_tool_record_spoke_with_known_tools_bumps_counters() {
        let c = super::new_env_tool_counters();
        c.record_spoke(&["get_active_window".to_string(), "get_weather".to_string()]);
        assert_eq!(c.spoke_total.load(Ordering::Relaxed), 1);
        assert_eq!(c.spoke_with_any.load(Ordering::Relaxed), 1);
        assert_eq!(c.active_window.load(Ordering::Relaxed), 1);
        assert_eq!(c.weather.load(Ordering::Relaxed), 1);
        assert_eq!(c.upcoming_events.load(Ordering::Relaxed), 0);
        assert_eq!(c.memory_search.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn env_tool_record_spoke_without_env_tools_only_bumps_total() {
        let c = super::new_env_tool_counters();
        // Only mutating / non-env tools — should count as a spoke turn but no env coverage.
        c.record_spoke(&["memory_edit".to_string(), "bash".to_string()]);
        assert_eq!(c.spoke_total.load(Ordering::Relaxed), 1);
        assert_eq!(c.spoke_with_any.load(Ordering::Relaxed), 0);
        assert_eq!(c.active_window.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn env_tool_record_spoke_empty_tools_only_bumps_total() {
        let c = super::new_env_tool_counters();
        c.record_spoke(&[]);
        assert_eq!(c.spoke_total.load(Ordering::Relaxed), 1);
        assert_eq!(c.spoke_with_any.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn env_tool_counters_accumulate_across_calls() {
        let c = super::new_env_tool_counters();
        c.record_spoke(&["get_weather".to_string()]);
        c.record_spoke(&["get_weather".to_string(), "memory_search".to_string()]);
        c.record_spoke(&[]);
        assert_eq!(c.spoke_total.load(Ordering::Relaxed), 3);
        assert_eq!(c.spoke_with_any.load(Ordering::Relaxed), 2);
        assert_eq!(c.weather.load(Ordering::Relaxed), 2);
        assert_eq!(c.memory_search.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn prompt_tilt_record_dispatch_classifies_correctly() {
        let c = super::new_prompt_tilt_counters();
        // 2 restraint vs 0 engagement → restraint_dominant
        c.record_dispatch(&["chatty", "icebreaker"]);
        // 0 restraint vs 1 engagement → engagement_dominant
        c.record_dispatch(&["engagement-window"]);
        // 1 restraint vs 1 engagement → balanced (mixed but equal)
        c.record_dispatch(&["chatty", "long-idle-no-restraint"]);
        // Only instructional/corrective → neutral
        c.record_dispatch(&["plan", "env-awareness"]);
        // Empty → also neutral (no rules at all)
        c.record_dispatch(&[]);
        assert_eq!(c.restraint_dominant.load(Ordering::Relaxed), 1);
        assert_eq!(c.engagement_dominant.load(Ordering::Relaxed), 1);
        assert_eq!(c.balanced.load(Ordering::Relaxed), 1);
        assert_eq!(c.neutral.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn prompt_tilt_unknown_labels_ignored() {
        // Unknown labels (typos / future labels not yet classified) don't count toward
        // restraint or engagement; they leave the bucket determined by the known ones.
        let c = super::new_prompt_tilt_counters();
        c.record_dispatch(&["chatty", "future-unknown-label"]);
        assert_eq!(c.restraint_dominant.load(Ordering::Relaxed), 1);
        assert_eq!(c.engagement_dominant.load(Ordering::Relaxed), 0);
        assert_eq!(c.neutral.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn prompt_tilt_can_be_reset() {
        let c = super::new_prompt_tilt_counters();
        c.record_dispatch(&["chatty"]);
        c.record_dispatch(&["engagement-window"]);
        c.restraint_dominant.store(0, Ordering::Relaxed);
        c.engagement_dominant.store(0, Ordering::Relaxed);
        c.balanced.store(0, Ordering::Relaxed);
        c.neutral.store(0, Ordering::Relaxed);
        assert_eq!(c.restraint_dominant.load(Ordering::Relaxed), 0);
        assert_eq!(c.engagement_dominant.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn cache_counters_can_be_reset_to_zero() {
        let c = new_cache_counters();
        c.turns.fetch_add(7, Ordering::Relaxed);
        c.hits.fetch_add(20, Ordering::Relaxed);
        c.calls.fetch_add(40, Ordering::Relaxed);
        // Inline what reset_cache_stats does — the Tauri State wrapper is just plumbing.
        c.turns.store(0, Ordering::Relaxed);
        c.hits.store(0, Ordering::Relaxed);
        c.calls.store(0, Ordering::Relaxed);
        assert_eq!(c.turns.load(Ordering::Relaxed), 0);
        assert_eq!(c.hits.load(Ordering::Relaxed), 0);
        assert_eq!(c.calls.load(Ordering::Relaxed), 0);
    }

    // ---- write_log size cap ----

    #[test]
    fn write_log_caps_at_max_lines() {
        let store = Arc::new(Mutex::new(Vec::<String>::new()));
        // Write a few more than the cap. The on-disk side is best-effort and won't fail
        // when log_dir() doesn't exist in CI, so this test only inspects the Vec.
        let total = MAX_LOG_LINES + 50;
        for i in 0..total {
            write_log(&store, &format!("line {}", i));
        }
        let logs = store.lock().unwrap();
        assert_eq!(logs.len(), MAX_LOG_LINES, "buffer must stay at cap");
        // The 50 oldest were dropped; the most recent should be "line 5049".
        let last = logs.last().expect("at least one entry");
        assert!(
            last.contains(&format!("line {}", total - 1)),
            "newest preserved"
        );
        let first = logs.first().expect("at least one entry");
        assert!(
            first.contains(&format!("line {}", total - MAX_LOG_LINES)),
            "oldest in window is line {}, got: {}",
            total - MAX_LOG_LINES,
            first
        );
    }

    #[test]
    fn write_log_under_cap_is_pure_append() {
        let store = Arc::new(Mutex::new(Vec::<String>::new()));
        write_log(&store, "first");
        write_log(&store, "second");
        write_log(&store, "third");
        let logs = store.lock().unwrap();
        assert_eq!(logs.len(), 3);
        assert!(logs[0].contains("first"));
        assert!(logs[2].contains("third"));
    }

    #[test]
    fn from_counters_round_trips_each_stat_struct() {
        // Iter QG6: each stat struct gained a `from_counters(&ProcessCounters)`
        // impl so the bundled `get_debug_snapshot` aggregator and the existing
        // per-stat Tauri commands share one read path. Bump distinct values into
        // every counter group at once and confirm every snapshot field reads back
        // exactly — guards against a future "I added a field but forgot to wire
        // from_counters" regression.
        use super::{
            CacheStats, EnvToolStats, LlmOutcomeStats, MoodTagStats, ProcessCounters,
            PromptTiltStats,
        };
        let c = ProcessCounters::default();
        c.cache.turns.fetch_add(1, Ordering::Relaxed);
        c.cache.hits.fetch_add(2, Ordering::Relaxed);
        c.cache.calls.fetch_add(3, Ordering::Relaxed);
        c.mood_tag.with_tag.fetch_add(4, Ordering::Relaxed);
        c.mood_tag.without_tag.fetch_add(5, Ordering::Relaxed);
        c.mood_tag.no_mood.fetch_add(6, Ordering::Relaxed);
        c.llm_outcome.spoke.fetch_add(7, Ordering::Relaxed);
        c.llm_outcome.silent.fetch_add(8, Ordering::Relaxed);
        c.llm_outcome.error.fetch_add(9, Ordering::Relaxed);
        c.env_tool.spoke_total.fetch_add(10, Ordering::Relaxed);
        c.env_tool.spoke_with_any.fetch_add(11, Ordering::Relaxed);
        c.env_tool.active_window.fetch_add(12, Ordering::Relaxed);
        c.env_tool.weather.fetch_add(13, Ordering::Relaxed);
        c.env_tool.upcoming_events.fetch_add(14, Ordering::Relaxed);
        c.env_tool.memory_search.fetch_add(15, Ordering::Relaxed);
        c.prompt_tilt
            .restraint_dominant
            .fetch_add(16, Ordering::Relaxed);
        c.prompt_tilt
            .engagement_dominant
            .fetch_add(17, Ordering::Relaxed);
        c.prompt_tilt.balanced.fetch_add(18, Ordering::Relaxed);
        c.prompt_tilt.neutral.fetch_add(19, Ordering::Relaxed);

        let cache = CacheStats::from_counters(&c);
        assert_eq!(
            (cache.turns, cache.total_hits, cache.total_calls),
            (1, 2, 3)
        );
        let mt = MoodTagStats::from_counters(&c);
        assert_eq!((mt.with_tag, mt.without_tag, mt.no_mood), (4, 5, 6));
        let llm = LlmOutcomeStats::from_counters(&c);
        assert_eq!((llm.spoke, llm.silent, llm.error), (7, 8, 9));
        let env = EnvToolStats::from_counters(&c);
        assert_eq!(
            (
                env.spoke_total,
                env.spoke_with_any,
                env.active_window,
                env.weather,
                env.upcoming_events,
                env.memory_search,
            ),
            (10, 11, 12, 13, 14, 15),
        );
        let tilt = PromptTiltStats::from_counters(&c);
        assert_eq!(
            (
                tilt.restraint_dominant,
                tilt.engagement_dominant,
                tilt.balanced,
                tilt.neutral,
            ),
            (16, 17, 18, 19),
        );
    }
}
