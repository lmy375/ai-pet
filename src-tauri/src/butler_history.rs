//! Append-only log of butler-task touches — every time the LLM updates or deletes a
//! `butler_tasks` memory entry the event lands here. The pet uses this as the
//! "what did I do for the user lately?" surface (panel display + future
//! consolidate-time daily summary).
//!
//! Distinct from `speech_history.log`: speeches are what the pet *said*, butler
//! events are what the pet *did*. They overlap when the LLM both speaks and
//! marks a task done in the same proactive turn, but the conceptual axes differ.
//!
//! File: `~/.config/pet/butler_history.log`. One line per event:
//!   `<ts> <action> <title> :: <desc-snippet>`
//! Newlines in the snippet are flattened. Cap at `BUTLER_HISTORY_CAP` lines.

use std::path::PathBuf;

use crate::log_rotation::rotate_if_needed;

/// Hard cap on retained entries. Higher than the panel ever surfaces (3–10) so future
/// daily-summary or weekly-rollup features can read further back without re-architecting.
pub const BUTLER_HISTORY_CAP: usize = 100;
/// Byte ceiling — defense in depth on top of the line-count trim.
const BUTLER_HISTORY_MAX_BYTES: u64 = 100_000;
/// How many chars of the description to keep in the log line. The full description is
/// still in the memory entry; this just keeps the log human-scannable.
pub const BUTLER_HISTORY_DESC_CHARS: usize = 80;

fn history_path() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("pet").join("butler_history.log"))
}

/// Append a butler event. Best-effort — IO failure must not break memory_edit's
/// happy path (the user will lose the log line but their task data is fine).
pub async fn record_event(action: &str, title: &str, description: &str) {
    let _ = record_event_inner(action, title, description).await;
}

async fn record_event_inner(
    action: &str,
    title: &str,
    description: &str,
) -> std::io::Result<()> {
    let Some(path) = history_path() else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let _ = rotate_if_needed(&path, BUTLER_HISTORY_MAX_BYTES).await;
    let existing = tokio::fs::read_to_string(&path).await.unwrap_or_default();
    let mut entries: Vec<String> = existing
        .lines()
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect();
    let ts = chrono::Local::now()
        .format("%Y-%m-%dT%H:%M:%S%:z")
        .to_string();
    let line = format!("{} {}", ts, format_event_body(action, title, description));
    entries.push(line);
    if entries.len() > BUTLER_HISTORY_CAP {
        let drop = entries.len() - BUTLER_HISTORY_CAP;
        entries.drain(0..drop);
    }
    let mut content = entries.join("\n");
    content.push('\n');
    tokio::fs::write(&path, content).await?;
    Ok(())
}

/// Pure helper that formats the body (everything after the timestamp) of one log line.
/// Format: `<action> <title> :: <desc-snippet>`. Description gets newlines flattened
/// and is truncated to `BUTLER_HISTORY_DESC_CHARS` characters with `…`.
pub fn format_event_body(action: &str, title: &str, description: &str) -> String {
    let flat = description.replace('\n', " ").replace('\r', " ");
    let trimmed = flat.trim();
    let snippet: String = if trimmed.chars().count() <= BUTLER_HISTORY_DESC_CHARS {
        trimmed.to_string()
    } else {
        let head: String = trimmed.chars().take(BUTLER_HISTORY_DESC_CHARS).collect();
        format!("{}…", head)
    };
    format!("{} {} :: {}", action, title.trim(), snippet)
}

/// Read up to the last `n` entries (oldest first, newest last) from the log.
pub async fn recent_events(n: usize) -> Vec<String> {
    if n == 0 {
        return vec![];
    }
    let Some(path) = history_path() else {
        return vec![];
    };
    let content = tokio::fs::read_to_string(&path).await.unwrap_or_default();
    parse_recent(&content, n)
}

/// Pure parser — last `n` non-empty lines from `content`, in original order.
pub fn parse_recent(content: &str, n: usize) -> Vec<String> {
    if n == 0 {
        return vec![];
    }
    let lines: Vec<&str> = content.lines().filter(|l| !l.is_empty()).collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].iter().map(|s| s.to_string()).collect()
}

/// Tauri command exposing recent butler events to the panel. Default n=10.
#[tauri::command]
pub async fn get_butler_history(n: Option<usize>) -> Vec<String> {
    recent_events(n.unwrap_or(10)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_event_body_short_desc_kept_verbatim() {
        assert_eq!(
            format_event_body("update", "早报", "已生成 ~/today.md"),
            "update 早报 :: 已生成 ~/today.md"
        );
    }

    #[test]
    fn format_event_body_truncates_long_desc() {
        let long = "字".repeat(120);
        let body = format_event_body("update", "task", &long);
        assert!(body.contains("…"));
        let kept = body.chars().filter(|c| *c == '字').count();
        assert_eq!(kept, BUTLER_HISTORY_DESC_CHARS);
    }

    #[test]
    fn format_event_body_flattens_newlines() {
        let body = format_event_body("update", "t", "line1\nline2\rline3");
        assert!(!body.contains('\n'));
        assert!(!body.contains('\r'));
        assert!(body.contains("line1 line2 line3"));
    }

    #[test]
    fn format_event_body_trims_title_and_desc_whitespace() {
        let body = format_event_body("delete", "  早报  ", "  已撤销  ");
        assert_eq!(body, "delete 早报 :: 已撤销");
    }

    #[test]
    fn parse_recent_handles_empty_and_zero() {
        assert!(parse_recent("", 5).is_empty());
        assert!(parse_recent("a\nb\n", 0).is_empty());
    }

    #[test]
    fn parse_recent_returns_tail_in_order() {
        let content = "line1\nline2\nline3\nline4\n";
        let out = parse_recent(content, 2);
        assert_eq!(out, vec!["line3".to_string(), "line4".to_string()]);
    }

    #[test]
    fn parse_recent_caps_at_available() {
        let content = "a\nb\n";
        let out = parse_recent(content, 5);
        assert_eq!(out, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn parse_recent_skips_blank_lines() {
        let content = "a\n\nb\n\n\nc\n";
        let out = parse_recent(content, 10);
        assert_eq!(out, vec!["a".to_string(), "b".to_string(), "c".to_string()]);
    }
}
