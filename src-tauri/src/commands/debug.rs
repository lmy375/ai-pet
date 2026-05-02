use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::State;

/// Maximum number of in-memory log lines retained in `LogStore`. Older lines are dropped
/// when the buffer overflows. 5000 lines ≈ several hundred LLM turns at typical 10–30
/// lines per turn — comfortably more than a session's worth, but still bounded so a
/// long-running pet doesn't slowly leak. The on-disk app.log is not capped here.
pub const MAX_LOG_LINES: usize = 5000;

#[derive(Clone)]
pub struct LogStore(pub Arc<Mutex<Vec<String>>>);

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

/// Pure parser — extracts (hits, total) from a single summary line in the form
/// `"... Tool cache summary: <hits>/<total> hits (<pct>%)"`. Returns None if the line
/// doesn't match. Extracted to a separate function so it can be unit-tested without
/// touching the LogStore.
pub fn parse_cache_summary(line: &str) -> Option<(u64, u64)> {
    let body = line.split("Tool cache summary:").nth(1)?.trim();
    let stat_segment = body.split_whitespace().next()?;
    let (hits_str, total_str) = stat_segment.split_once('/')?;
    let hits: u64 = hits_str.trim().parse().ok()?;
    let total: u64 = total_str.trim().parse().ok()?;
    Some((hits, total))
}

#[tauri::command]
pub fn get_cache_stats(store: State<'_, LogStore>) -> CacheStats {
    let logs = store.0.lock().unwrap();
    let mut stats = CacheStats {
        turns: 0,
        total_hits: 0,
        total_calls: 0,
    };
    for line in logs.iter() {
        if let Some((hits, total)) = parse_cache_summary(line) {
            stats.turns += 1;
            stats.total_hits += hits;
            stats.total_calls += total;
        }
    }
    stats
}

#[cfg(test)]
mod tests {
    use super::{parse_cache_summary, write_log, MAX_LOG_LINES};
    use std::sync::{Arc, Mutex};

    #[test]
    fn parses_canonical_summary_line() {
        let line = "[12:34:56] Tool cache summary: 3/5 hits (60%)";
        assert_eq!(parse_cache_summary(line), Some((3, 5)));
    }

    #[test]
    fn parses_zero_hits() {
        let line = "Tool cache summary: 0/4 hits (0%)";
        assert_eq!(parse_cache_summary(line), Some((0, 4)));
    }

    #[test]
    fn parses_perfect_hit_ratio() {
        let line = "Tool cache summary: 7/7 hits (100%)";
        assert_eq!(parse_cache_summary(line), Some((7, 7)));
    }

    #[test]
    fn ignores_unrelated_log_lines() {
        assert_eq!(parse_cache_summary("Tool call: get_weather({})"), None);
        assert_eq!(parse_cache_summary("LLM round 0 (3 messages)"), None);
        assert_eq!(parse_cache_summary(""), None);
    }

    #[test]
    fn rejects_malformed_numbers() {
        assert_eq!(
            parse_cache_summary("Tool cache summary: x/y hits (?%)"),
            None,
        );
        assert_eq!(
            parse_cache_summary("Tool cache summary: 3 hits"),
            None,
            "missing slash"
        );
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
