//! Append-only history of the pet's own mood entries (Iter 103, route A).
//!
//! Mirrors the design of `speech_history` — one line per recorded mood, capped at a
//! manageable line count, size-bounded for safety, and parsed by pure helpers so the
//! "what's my mood trend over the last week?" question can be answered without
//! re-running an LLM.
//!
//! Format: `<ISO8601 timestamp> <MOTION> | <free text>` where MOTION is one of
//! `Tap | Flick | Flick3 | Idle | -` (- when the mood entry was missing the
//! `[motion: X]` prefix). The pipe lets pure parsers split text from motion without
//! ambiguity even when the text contains spaces.

use std::path::PathBuf;

use crate::log_rotation::rotate_if_needed;

/// Hard cap on retained entries. ~200 entries comfortably covers a few weeks of
/// proactive mood updates at typical cadence; the trend summary only ever reads the
/// tail (≤ 50 lines), so older entries are pruned without loss.
pub const MOOD_HISTORY_CAP: usize = 200;
const MOOD_HISTORY_MAX_BYTES: u64 = 200_000;

fn history_path() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("pet").join("mood_history.log"))
}

/// Append a mood entry (best-effort). De-dupes against the most recent line so the
/// log captures *changes* rather than every proactive tick where the mood happened
/// to still be "Idle / 平静"; otherwise the trend summary would be dominated by
/// repetition. IO errors are swallowed — mood logging never blocks the chat path.
pub async fn record_mood(text: &str, motion: &Option<String>) {
    let _ = record_mood_inner(text, motion).await;
}

async fn record_mood_inner(text: &str, motion: &Option<String>) -> std::io::Result<()> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    let Some(path) = history_path() else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let _ = rotate_if_needed(&path, MOOD_HISTORY_MAX_BYTES).await;

    let existing = tokio::fs::read_to_string(&path).await.unwrap_or_default();
    let motion_str = motion.as_deref().unwrap_or("-");

    // Skip when the most recent line records exactly the same motion + text — keeps
    // the log focused on transitions instead of identical re-reads.
    if let Some(last) = existing.lines().rfind(|l| !l.is_empty()) {
        if let Some((last_motion, last_text)) = parse_motion_text(last) {
            if last_motion == motion_str && last_text == trimmed {
                return Ok(());
            }
        }
    }

    let mut entries: Vec<String> = existing
        .lines()
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect();
    let ts = chrono::Local::now()
        .format("%Y-%m-%dT%H:%M:%S%:z")
        .to_string();
    let flat = trimmed.replace(['\n', '\r'], " ");
    entries.push(format!("{} {} | {}", ts, motion_str, flat));
    if entries.len() > MOOD_HISTORY_CAP {
        let drop = entries.len() - MOOD_HISTORY_CAP;
        entries.drain(0..drop);
    }
    let mut content = entries.join("\n");
    content.push('\n');
    tokio::fs::write(&path, content).await
}

/// Pure: split a logged line into (motion, text). Format is
/// `<ts> <motion> | <text>` — splits on the first ` | ` to allow `|` inside the
/// text without breaking parsing. Returns None on malformed lines.
pub fn parse_motion_text(line: &str) -> Option<(&str, &str)> {
    let (head, text) = line.split_once(" | ")?;
    // head is "<ts> <motion>" — split on last space.
    let (_, motion) = head.rsplit_once(' ')?;
    Some((motion, text))
}

/// Pure: from `content`, take the last `n` non-empty lines and tally motions by
/// occurrence. Returns a list of `(motion, count)` sorted by count descending. The
/// `-` motion (mood without prefix) is included; callers can filter if they want
/// a strict "tagged-only" view.
pub fn summarize_recent_motions(content: &str, n: usize) -> Vec<(String, u64)> {
    if n == 0 {
        return vec![];
    }
    let lines: Vec<&str> = content.lines().filter(|l| !l.is_empty()).collect();
    let start = lines.len().saturating_sub(n);
    let window = &lines[start..];
    let mut counts: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
    for line in window {
        if let Some((motion, _)) = parse_motion_text(line) {
            *counts.entry(motion.to_string()).or_insert(0) += 1;
        }
    }
    let mut sorted: Vec<(String, u64)> = counts.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    sorted
}

/// Pure: build the natural-language trend hint from the recent-mood window. None when
/// the log has too few entries to say anything useful (< `min_entries`); avoids the
/// "your mood has been Idle ×1 (most common!)" pseudo-insight on day 0.
pub fn format_trend_hint(content: &str, n: usize, min_entries: u64) -> Option<String> {
    let counts = summarize_recent_motions(content, n);
    let total: u64 = counts.iter().map(|(_, c)| c).sum();
    if total < min_entries {
        return None;
    }
    let parts: Vec<String> = counts
        .iter()
        .filter(|(m, _)| m != "-")
        .map(|(m, c)| format!("{} × {}", m, c))
        .collect();
    if parts.is_empty() {
        return None;
    }
    Some(format!(
        "你最近 {} 次心情记录里：{}（按出现次数排序）。这是你长期的情绪谱——可以让 ta 渗进当下语气，但不必生硬带出。",
        total,
        parts.join("、"),
    ))
}

/// Read the file and return the trend hint, or empty string when no log / too few
/// entries. Convenience for proactive prompt construction.
pub async fn build_trend_hint(window: usize, min_entries: u64) -> String {
    let Some(path) = history_path() else {
        return String::new();
    };
    let content = tokio::fs::read_to_string(&path).await.unwrap_or_default();
    format_trend_hint(&content, window, min_entries).unwrap_or_default()
}

/// Tauri command returning the formatted mood-trend hint (Iter 105). Same window /
/// min-entries the proactive prompt uses, so the panel and the LLM see the exact
/// same trend description — no source-of-truth divergence.
#[tauri::command]
pub async fn get_mood_trend_hint() -> String {
    build_trend_hint(50, 5).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_motion_text_normal_line() {
        let line = "2026-05-03T12:34:56+08:00 Tap | 看用户在专心写代码";
        assert_eq!(parse_motion_text(line), Some(("Tap", "看用户在专心写代码")));
    }

    #[test]
    fn parse_motion_text_handles_pipe_in_text() {
        // Body containing ' | ' should still parse — split on the first occurrence.
        let line = "2026-05-03T12:00:00+08:00 Idle | 想了想 | 没说话";
        assert_eq!(parse_motion_text(line), Some(("Idle", "想了想 | 没说话")));
    }

    #[test]
    fn parse_motion_text_dash_for_missing_motion() {
        let line = "2026-05-03T12:00:00+08:00 - | 平静";
        assert_eq!(parse_motion_text(line), Some(("-", "平静")));
    }

    #[test]
    fn parse_motion_text_malformed_returns_none() {
        assert!(parse_motion_text("no separator here").is_none());
    }

    #[test]
    fn summarize_recent_motions_counts_and_sorts() {
        let content = "\
2026-05-01T10:00:00+08:00 Tap | a
2026-05-01T11:00:00+08:00 Tap | b
2026-05-02T10:00:00+08:00 Idle | c
2026-05-02T11:00:00+08:00 Tap | d
2026-05-03T10:00:00+08:00 Flick | e
";
        let counts = summarize_recent_motions(content, 10);
        // Tap leads (3), Flick + Idle tie at 1 — alphabetical tiebreak.
        assert_eq!(counts[0], ("Tap".to_string(), 3));
        assert_eq!(counts[1], ("Flick".to_string(), 1));
        assert_eq!(counts[2], ("Idle".to_string(), 1));
    }

    #[test]
    fn summarize_recent_motions_takes_only_window() {
        let content = "\
2026-05-01T10:00:00+08:00 Tap | a
2026-05-02T10:00:00+08:00 Idle | b
2026-05-03T10:00:00+08:00 Flick | c
";
        let counts = summarize_recent_motions(content, 1);
        // Only the last line counts.
        assert_eq!(counts, vec![("Flick".to_string(), 1)]);
    }

    #[test]
    fn format_trend_hint_below_min_returns_none() {
        let content = "\
2026-05-01T10:00:00+08:00 Tap | a
2026-05-02T10:00:00+08:00 Idle | b
";
        // 2 entries < min 5 → None.
        assert!(format_trend_hint(content, 50, 5).is_none());
    }

    #[test]
    fn format_trend_hint_above_min_includes_motions_in_order() {
        let content = "\
2026-05-01T10:00:00+08:00 Tap | a
2026-05-01T11:00:00+08:00 Tap | b
2026-05-02T10:00:00+08:00 Idle | c
2026-05-02T11:00:00+08:00 Tap | d
2026-05-03T10:00:00+08:00 Flick | e
";
        let hint = format_trend_hint(content, 50, 3).unwrap();
        // Tap 出现最多排第一；Idle 和 Flick 各 1 次按字母排（Flick 在前）。
        let tap_pos = hint.find("Tap × 3").unwrap();
        let flick_pos = hint.find("Flick × 1").unwrap();
        assert!(tap_pos < flick_pos);
    }

    #[test]
    fn format_trend_hint_filters_dash_motion() {
        // Untagged mood entries (motion = "-") are not informative for trend context;
        // they still count toward total but don't show up in the hint body.
        let content = "\
2026-05-01T10:00:00+08:00 Tap | a
2026-05-01T11:00:00+08:00 - | b
2026-05-02T10:00:00+08:00 Tap | c
";
        let hint = format_trend_hint(content, 50, 1).unwrap();
        assert!(hint.contains("Tap"));
        assert!(!hint.contains("-"));
    }

    #[test]
    fn format_trend_hint_only_dash_returns_none() {
        // If all entries are untagged, the body would be empty after filtering — return
        // None rather than emit a "你最近 N 次心情记录里：（无）" awkward placeholder.
        let content = "\
2026-05-01T10:00:00+08:00 - | a
2026-05-02T10:00:00+08:00 - | b
";
        assert!(format_trend_hint(content, 50, 1).is_none());
    }
}
