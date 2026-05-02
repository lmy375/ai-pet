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

pub type CacheCountersStore = Arc<CacheCounters>;

pub fn new_cache_counters() -> CacheCountersStore {
    Arc::new(CacheCounters::default())
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
    let ts = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string();
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

#[tauri::command]
pub fn get_cache_stats(counters: State<'_, CacheCountersStore>) -> CacheStats {
    CacheStats {
        turns: counters.turns.load(Ordering::Relaxed),
        total_hits: counters.hits.load(Ordering::Relaxed),
        total_calls: counters.calls.load(Ordering::Relaxed),
    }
}

/// Zero out the process-wide cache counters. Mirrors `clear_logs` for the stats panel —
/// useful when the user wants to measure hit ratio over a fresh window.
#[tauri::command]
pub fn reset_cache_stats(counters: State<'_, CacheCountersStore>) {
    counters.turns.store(0, Ordering::Relaxed);
    counters.hits.store(0, Ordering::Relaxed);
    counters.calls.store(0, Ordering::Relaxed);
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
        assert!(last.contains(&format!("line {}", total - 1)), "newest preserved");
        let first = logs.first().expect("at least one entry");
        assert!(first.contains(&format!("line {}", total - MAX_LOG_LINES)),
            "oldest in window is line {}, got: {}", total - MAX_LOG_LINES, first);
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
}
