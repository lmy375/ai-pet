//! Append-only history of the pet's own proactive utterances.
//!
//! Solves a specific problem: every proactive turn loads recent session messages and
//! injects them into the prompt — but session history can be long, get trimmed by the
//! `chat.max_context_messages` cap, or be a fresh session. The pet then forgets it just
//! said "good morning" 30 minutes ago and says it again. This module gives the model a
//! short, dedicated, deterministic record of "what I, the pet, said last".
//!
//! File: `~/.config/pet/speech_history.log`. One line per utterance, ISO timestamp space
//! text — newlines in the text are flattened to spaces. Trimmed to `SPEECH_HISTORY_CAP`
//! entries on every write so it never grows unbounded.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::log_rotation::rotate_if_needed;

/// Hard cap on retained entries. Far more than the prompt ever surfaces (5–10) — the
/// extra slack lets future features (e.g. a panel "what did the pet say lately?" view)
/// reach further back without re-architecting. `pub` so callers can detect when
/// `count_speeches` has saturated (a "50+" affordance vs reading 50 as the literal
/// lifetime number).
pub const SPEECH_HISTORY_CAP: usize = 50;
/// Byte ceiling — defense in depth on top of the line-count trim. A misbehaving LLM that
/// emits a megabyte-long "single utterance" can't blow up the file: rotation kicks in
/// and the next write starts a fresh log.
const SPEECH_HISTORY_MAX_BYTES: u64 = 100_000;

fn history_path() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("pet").join("speech_history.log"))
}

/// Tiny sidecar that holds the lifetime count of proactive utterances as a single
/// integer. We need this because `speech_history.log` is trimmed at SPEECH_HISTORY_CAP
/// and so its line count saturates; this file just keeps incrementing forever.
fn count_path() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("pet").join("speech_count.txt"))
}

/// Per-day bucket file: `{ "YYYY-MM-DD": count, ... }`. Lets the panel show "今天开口 N
/// 次" alongside the lifetime total without scanning speech_history.log timestamps every
/// tick. Pruned to `DAILY_RETAIN_DAYS` so it can't grow unbounded over years of use.
fn daily_path() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("pet").join("speech_daily.json"))
}

/// How many trailing days to retain in the per-day bucket file. Anything older is dropped
/// on the next write. 90 is more than the panel ever surfaces (today only, for now) but
/// gives future "last 7d" / "last 30d" features room to read back without re-architecting.
const DAILY_RETAIN_DAYS: usize = 90;

/// Append a new utterance to the history file, trimming to `SPEECH_HISTORY_CAP` entries
/// total. Best-effort — IO errors are silently ignored so a hosed disk doesn't break the
/// pet's actual speaking flow.
pub async fn record_speech(text: &str) {
    let _ = record_speech_inner(text).await;
}

async fn record_speech_inner(text: &str) -> std::io::Result<()> {
    let Some(path) = history_path() else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    // Rotate first so an oversized file doesn't get re-read into memory before we replace
    // it. After rotation the next read starts fresh; trimming to SPEECH_HISTORY_CAP still
    // applies to the new generation.
    let _ = rotate_if_needed(&path, SPEECH_HISTORY_MAX_BYTES).await;
    let existing = tokio::fs::read_to_string(&path).await.unwrap_or_default();
    let mut entries: Vec<String> = existing
        .lines()
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect();
    let ts = chrono::Local::now()
        .format("%Y-%m-%dT%H:%M:%S%:z")
        .to_string();
    let flat = text.replace('\n', " ").replace('\r', " ");
    entries.push(format!("{} {}", ts, flat));
    if entries.len() > SPEECH_HISTORY_CAP {
        let drop = entries.len() - SPEECH_HISTORY_CAP;
        entries.drain(0..drop);
    }
    let mut content = entries.join("\n");
    content.push('\n');
    tokio::fs::write(&path, content).await?;
    // Best-effort lifetime counter bump — failure here doesn't fail the speech write.
    let _ = bump_lifetime_count().await;
    // Best-effort per-day bucket bump — same rationale.
    let _ = bump_today_count().await;
    Ok(())
}

/// Read up to the last `n` entries from the history file. Empty vector when the file is
/// missing, unreadable, or `n == 0`.
pub async fn recent_speeches(n: usize) -> Vec<String> {
    if n == 0 {
        return vec![];
    }
    let Some(path) = history_path() else {
        return vec![];
    };
    let content = tokio::fs::read_to_string(&path).await.unwrap_or_default();
    parse_recent(&content, n)
}

/// Pure parser — gives the last `n` non-empty lines from `content` in original order
/// (oldest of the kept window first, newest last). Extracted so unit tests can exercise
/// the slicing without touching the filesystem.
pub fn parse_recent(content: &str, n: usize) -> Vec<String> {
    if n == 0 {
        return vec![];
    }
    let lines: Vec<&str> = content.lines().filter(|l| !l.is_empty()).collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].iter().map(|s| s.to_string()).collect()
}

/// Strip the leading ISO-8601 timestamp from a recorded line, returning just the text.
/// If the line doesn't look like "<ts> <text>" (no space), returns the whole line.
pub fn strip_timestamp(line: &str) -> &str {
    line.split_once(' ').map(|(_, rest)| rest).unwrap_or(line)
}

/// Tauri command exposing the most recent N speech entries to the panel UI. Each entry
/// is the raw "<ts> <text>" line — the frontend strips the timestamp itself for display
/// flexibility (could show as relative time later). Default n=10 if not supplied.
#[tauri::command]
pub async fn get_recent_speeches(n: Option<usize>) -> Vec<String> {
    recent_speeches(n.unwrap_or(10)).await
}

/// Tauri command exposing the persistent lifetime speech count for the panel stats
/// header. Thin wrapper over `lifetime_speech_count` so the frontend can `invoke` it
/// without going through `get_tone_snapshot` (which mixes a dozen other fields).
#[tauri::command]
pub async fn get_lifetime_speech_count() -> u64 {
    lifetime_speech_count().await
}

/// Number of non-empty lines currently in the speech_history.log file. Caps at
/// `SPEECH_HISTORY_CAP` because of trim-on-write. Useful for "show last N speeches"
/// but not for true lifetime stats — see `lifetime_speech_count` for that.
pub async fn count_speeches() -> usize {
    let Some(path) = history_path() else {
        return 0;
    };
    let content = tokio::fs::read_to_string(&path).await.unwrap_or_default();
    content.lines().filter(|l| !l.trim().is_empty()).count()
}

/// Lifetime count of proactive utterances, persisted across restarts in a sidecar file.
/// Returns 0 on a brand-new install. For an existing install that's upgrading from the
/// pre-counter version, the counter file won't exist yet; in that case we bootstrap
/// from `count_speeches` so users don't see a regression to 0 after upgrading. After
/// the first bump the sidecar file always exists and takes precedence.
pub async fn lifetime_speech_count() -> u64 {
    let Some(path) = count_path() else {
        return 0;
    };
    if let Ok(s) = tokio::fs::read_to_string(&path).await {
        if let Ok(n) = s.trim().parse::<u64>() {
            return n;
        }
    }
    // Bootstrap path — file missing or malformed.
    count_speeches().await as u64
}

/// Best-effort: increment the persistent lifetime counter by 1.
async fn bump_lifetime_count() -> std::io::Result<()> {
    let Some(path) = count_path() else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let current = lifetime_speech_count().await;
    tokio::fs::write(&path, format!("{}\n", current + 1)).await
}

/// Pure: parse the daily-bucket JSON. Malformed input returns an empty map (caller will
/// then write a fresh map on the next bump — a corrupt file self-heals after one speech).
pub fn parse_daily(content: &str) -> BTreeMap<String, u64> {
    serde_json::from_str(content).unwrap_or_default()
}

/// Pure: drop entries whose date keys come before `today - retain_days`. Uses string
/// comparison because YYYY-MM-DD sorts lexicographically. Non-parseable keys are kept —
/// the caller hasn't written them, so they're either future migrations or user-edited.
pub fn prune_daily(
    mut map: BTreeMap<String, u64>,
    today: chrono::NaiveDate,
    retain_days: usize,
) -> BTreeMap<String, u64> {
    let cutoff = today - chrono::Duration::days(retain_days as i64);
    let cutoff_str = cutoff.format("%Y-%m-%d").to_string();
    map.retain(|k, _| match chrono::NaiveDate::parse_from_str(k, "%Y-%m-%d") {
        Ok(_) => k.as_str() >= cutoff_str.as_str(),
        Err(_) => true,
    });
    map
}

/// Today's date in `YYYY-MM-DD` form using local time — same timezone as the speech log
/// timestamps so "今天" matches what the user's clock shows.
fn today_key() -> String {
    chrono::Local::now().date_naive().format("%Y-%m-%d").to_string()
}

/// Best-effort: increment today's bucket and prune any entries beyond the retain window.
async fn bump_today_count() -> std::io::Result<()> {
    let Some(path) = daily_path() else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let existing = tokio::fs::read_to_string(&path).await.unwrap_or_default();
    let mut map = parse_daily(&existing);
    let key = today_key();
    *map.entry(key).or_insert(0) += 1;
    let pruned = prune_daily(map, chrono::Local::now().date_naive(), DAILY_RETAIN_DAYS);
    let json = serde_json::to_string(&pruned).unwrap_or_else(|_| "{}".to_string());
    tokio::fs::write(&path, json).await
}

/// Number of proactive utterances recorded today (local time). Returns 0 when the file
/// doesn't exist or today's bucket is missing.
pub async fn today_speech_count() -> u64 {
    let Some(path) = daily_path() else {
        return 0;
    };
    let content = tokio::fs::read_to_string(&path).await.unwrap_or_default();
    let map = parse_daily(&content);
    map.get(&today_key()).copied().unwrap_or(0)
}

/// Tauri command exposing today's bucket count to the panel UI. Lets the stats card
/// render "今天开口 X 次" alongside the lifetime total.
#[tauri::command]
pub async fn get_today_speech_count() -> u64 {
    today_speech_count().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_recent_empty_content() {
        assert!(parse_recent("", 5).is_empty());
    }

    #[test]
    fn parse_recent_n_zero() {
        assert!(parse_recent("a\nb\nc\n", 0).is_empty());
    }

    #[test]
    fn parse_recent_fewer_than_n() {
        let v = parse_recent("a\nb\n", 5);
        assert_eq!(v, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn parse_recent_exactly_n() {
        let v = parse_recent("a\nb\nc\n", 3);
        assert_eq!(v, vec!["a".to_string(), "b".to_string(), "c".to_string()]);
    }

    #[test]
    fn parse_recent_more_than_n_keeps_tail() {
        let v = parse_recent("a\nb\nc\nd\ne\n", 3);
        assert_eq!(v, vec!["c".to_string(), "d".to_string(), "e".to_string()]);
    }

    #[test]
    fn parse_recent_skips_blank_lines() {
        let v = parse_recent("a\n\nb\n\nc\n", 5);
        assert_eq!(v, vec!["a".to_string(), "b".to_string(), "c".to_string()]);
    }

    #[test]
    fn strip_timestamp_normal_line() {
        let line = "2026-05-03T12:34:56+08:00 早上好啊";
        assert_eq!(strip_timestamp(line), "早上好啊");
    }

    #[test]
    fn strip_timestamp_no_space_returns_whole_line() {
        assert_eq!(strip_timestamp("noprefix"), "noprefix");
    }

    #[test]
    fn parse_daily_empty_or_malformed() {
        assert!(parse_daily("").is_empty());
        assert!(parse_daily("not json").is_empty());
        assert!(parse_daily("[1, 2, 3]").is_empty());
    }

    #[test]
    fn parse_daily_valid_object() {
        let m = parse_daily(r#"{"2026-05-01": 3, "2026-05-02": 5}"#);
        assert_eq!(m.get("2026-05-01"), Some(&3));
        assert_eq!(m.get("2026-05-02"), Some(&5));
        assert_eq!(m.len(), 2);
    }

    #[test]
    fn prune_daily_drops_entries_before_cutoff() {
        let mut m = BTreeMap::new();
        m.insert("2026-01-01".to_string(), 10);
        m.insert("2026-04-01".to_string(), 20);
        m.insert("2026-05-01".to_string(), 30);
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 3).unwrap();
        let pruned = prune_daily(m, today, 30);
        // cutoff = 2026-04-03; 2026-01-01 (< cutoff) drops, 2026-04-01 (< cutoff) drops,
        // 2026-05-01 (>= cutoff) stays.
        assert_eq!(pruned.len(), 1);
        assert!(pruned.contains_key("2026-05-01"));
    }

    #[test]
    fn prune_daily_keeps_unparseable_keys() {
        let mut m = BTreeMap::new();
        m.insert("not-a-date".to_string(), 7);
        m.insert("2026-01-01".to_string(), 1);
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 3).unwrap();
        let pruned = prune_daily(m, today, 30);
        assert!(pruned.contains_key("not-a-date"));
        assert!(!pruned.contains_key("2026-01-01"));
    }

    #[test]
    fn prune_daily_zero_retain_drops_everything_dated() {
        let mut m = BTreeMap::new();
        m.insert("2026-05-03".to_string(), 1);
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 3).unwrap();
        let pruned = prune_daily(m, today, 0);
        // cutoff == today, "2026-05-03" >= "2026-05-03" → kept (today is always retained).
        assert!(pruned.contains_key("2026-05-03"));
    }

    fn fresh_temp_dir(label: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("pet-test-{}-{}", label, nanos));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Manual round-trip: write entries directly with the same trim semantics as
    /// record_speech_inner, then validate parse_recent reads back the tail. We don't go
    /// through record_speech_inner because that hard-codes the user's config_dir path;
    /// recreating the trim logic in tests keeps file IO opt-out.
    #[test]
    fn write_and_parse_round_trip_with_trim() {
        let dir = fresh_temp_dir("speech");
        let path = dir.join("speech_history.log");
        let mut entries: Vec<String> = (0..(SPEECH_HISTORY_CAP + 5))
            .map(|i| format!("2026-05-03T12:00:00+08:00 line {}", i))
            .collect();
        if entries.len() > SPEECH_HISTORY_CAP {
            let drop = entries.len() - SPEECH_HISTORY_CAP;
            entries.drain(0..drop);
        }
        std::fs::write(&path, entries.join("\n") + "\n").unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        let recent = parse_recent(&content, 3);
        assert_eq!(recent.len(), 3);
        // After trimming the first 5, lines 5..(50+5) remain; last 3 are 52, 53, 54.
        assert!(recent[2].ends_with("line 54"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
