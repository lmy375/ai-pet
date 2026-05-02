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
    tokio::fs::write(&path, content).await
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

/// Total number of proactive utterances ever recorded. Used by the proactive prompt as
/// an "icebreaker" signal — when the count is small the pet hasn't spoken to the user
/// much yet and should keep openings exploratory. The count is line-based so it caps at
/// `SPEECH_HISTORY_CAP` after that many lifetime utterances; for the icebreaker use case
/// (first ~3 lines) that's plenty of resolution.
pub async fn count_speeches() -> usize {
    let Some(path) = history_path() else {
        return 0;
    };
    let content = tokio::fs::read_to_string(&path).await.unwrap_or_default();
    content.lines().filter(|l| !l.trim().is_empty()).count()
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
