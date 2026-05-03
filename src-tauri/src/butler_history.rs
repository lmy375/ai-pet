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
/// How many daily summaries to retain in `butler_daily.log`. 90 days is plenty for the
/// panel's "last week" view and supports future "monthly retro" features.
pub const BUTLER_DAILY_CAP: usize = 90;

fn history_path() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("pet").join("butler_history.log"))
}

fn daily_path() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("pet").join("butler_daily.log"))
}

/// Append a butler event. Best-effort — IO failure must not break memory_edit's
/// happy path (the user will lose the log line but their task data is fine).
pub async fn record_event(action: &str, title: &str, description: &str) {
    let _ = record_event_inner(action, title, description).await;
}

async fn record_event_inner(action: &str, title: &str, description: &str) -> std::io::Result<()> {
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
    let flat = description.replace(['\n', '\r'], " ");
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

// ---- Iter Cη: per-day summaries -------------------------------------------------

/// Pure helper — read butler_history log lines (already in `<ts> <action> <title> :: <desc>`
/// form) and produce a single human-readable summary for the given local date.
/// Updates and deletes are folded separately; titles are deduped per action so a task
/// touched 3 times in one day collapses to one mention. Returns `None` when no events
/// match the date so callers can choose to skip persistence on quiet days.
///
/// Format example: `今天我帮你 推进了「早报」「文件整理」，撤销/移除了「过期任务」`
pub fn summarize_events_for_date(events: &[String], date: chrono::NaiveDate) -> Option<String> {
    let date_prefix = date.format("%Y-%m-%d").to_string();
    let mut updates: Vec<String> = Vec::new();
    let mut deletes: Vec<String> = Vec::new();

    for line in events {
        if !line.starts_with(&date_prefix) {
            continue;
        }
        // Skip the "<ts> " head; the body is "<action> <title> :: <desc>".
        let after_ts = match line.split_once(' ') {
            Some((_, rest)) => rest,
            None => continue,
        };
        let (action_title, _desc) = after_ts.split_once(" :: ").unwrap_or((after_ts, ""));
        let Some((action, title)) = action_title.split_once(' ') else {
            continue;
        };
        let title = title.trim().to_string();
        if title.is_empty() {
            continue;
        }
        match action {
            "update" => updates.push(title),
            "delete" => deletes.push(title),
            _ => {}
        }
    }

    fn dedup_keep_order(v: Vec<String>) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        v.into_iter().filter(|x| seen.insert(x.clone())).collect()
    }
    let updates = dedup_keep_order(updates);
    let deletes = dedup_keep_order(deletes);

    if updates.is_empty() && deletes.is_empty() {
        return None;
    }

    let mut parts: Vec<String> = Vec::new();
    if !updates.is_empty() {
        parts.push(format!("推进了「{}」", updates.join("」「")));
    }
    if !deletes.is_empty() {
        parts.push(format!("撤销/移除了「{}」", deletes.join("」「")));
    }
    Some(format!("今天我帮你 {}", parts.join("，")))
}

/// Upsert a per-day summary line into `butler_daily.log`. Lines are
/// `<YYYY-MM-DD> <summary>`. Any existing line for the same date is replaced so
/// re-running consolidate the same day overwrites the previous summary.
/// Best-effort — IO failure is silent.
pub async fn record_daily_summary(date: chrono::NaiveDate, summary: &str) {
    let _ = record_daily_summary_inner(date, summary).await;
}

async fn record_daily_summary_inner(date: chrono::NaiveDate, summary: &str) -> std::io::Result<()> {
    let Some(path) = daily_path() else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let date_str = date.format("%Y-%m-%d").to_string();
    let existing = tokio::fs::read_to_string(&path).await.unwrap_or_default();
    let flat = summary.replace(['\n', '\r'], " ");
    let new_line = format!("{} {}", date_str, flat);
    let kept: Vec<String> = existing
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with(&format!("{} ", date_str)))
        .map(String::from)
        .collect();
    let mut all = kept;
    all.push(new_line);
    if all.len() > BUTLER_DAILY_CAP {
        let drop = all.len() - BUTLER_DAILY_CAP;
        all.drain(0..drop);
    }
    let mut content = all.join("\n");
    content.push('\n');
    tokio::fs::write(&path, content).await?;
    Ok(())
}

/// Read up to the last `n` daily summary lines (oldest first).
pub async fn recent_summaries(n: usize) -> Vec<String> {
    if n == 0 {
        return vec![];
    }
    let Some(path) = daily_path() else {
        return vec![];
    };
    let content = tokio::fs::read_to_string(&path).await.unwrap_or_default();
    parse_recent(&content, n)
}

/// Tauri command — recent daily summaries. Default n=7 (a week).
#[tauri::command]
pub async fn get_butler_daily_summaries(n: Option<usize>) -> Vec<String> {
    recent_summaries(n.unwrap_or(7)).await
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

    fn date(y: i32, m: u32, d: u32) -> chrono::NaiveDate {
        chrono::NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn summarize_events_for_date_no_events_returns_none() {
        let events: Vec<String> = vec![];
        assert_eq!(summarize_events_for_date(&events, date(2026, 5, 3)), None);
    }

    #[test]
    fn summarize_events_for_date_only_other_dates_returns_none() {
        let events = vec![
            "2026-05-02T09:00:00+08:00 update 早报 :: ok".to_string(),
            "2026-05-04T09:00:00+08:00 delete 旧任务 :: ok".to_string(),
        ];
        assert_eq!(summarize_events_for_date(&events, date(2026, 5, 3)), None);
    }

    #[test]
    fn summarize_events_for_date_one_update() {
        let events = vec!["2026-05-03T09:15:00+08:00 update 早报 :: 已生成 ~/today.md".to_string()];
        assert_eq!(
            summarize_events_for_date(&events, date(2026, 5, 3)),
            Some("今天我帮你 推进了「早报」".to_string())
        );
    }

    #[test]
    fn summarize_events_for_date_mixes_updates_and_deletes() {
        let events = vec![
            "2026-05-03T09:00:00+08:00 update 早报 :: ok".to_string(),
            "2026-05-03T10:00:00+08:00 update 文件整理 :: ok".to_string(),
            "2026-05-03T11:00:00+08:00 delete 过期任务 :: ok".to_string(),
        ];
        let out = summarize_events_for_date(&events, date(2026, 5, 3)).unwrap();
        assert!(out.starts_with("今天我帮你 "));
        assert!(out.contains("推进了「早报」「文件整理」"));
        assert!(out.contains("撤销/移除了「过期任务」"));
        // Updates section should come before deletes section.
        let upd_idx = out.find("推进了").unwrap();
        let del_idx = out.find("撤销/移除了").unwrap();
        assert!(upd_idx < del_idx);
    }

    #[test]
    fn summarize_events_for_date_dedups_repeated_actions() {
        // Same task touched 3x in a day → one mention.
        let events = vec![
            "2026-05-03T09:00:00+08:00 update 早报 :: round1".to_string(),
            "2026-05-03T10:00:00+08:00 update 早报 :: round2".to_string(),
            "2026-05-03T11:00:00+08:00 update 早报 :: round3".to_string(),
        ];
        let out = summarize_events_for_date(&events, date(2026, 5, 3)).unwrap();
        // "「早报」" should appear exactly once.
        assert_eq!(out.matches("「早报」").count(), 1);
    }

    #[test]
    fn summarize_events_for_date_filters_strictly_by_date_prefix() {
        // A line that starts with a different date but happens to contain today's
        // date string mid-line should NOT match.
        let events = vec![
            "2026-05-02T23:59:00+08:00 update 跨夜任务 :: 描述里提到 2026-05-03 但日期不在前缀"
                .to_string(),
        ];
        assert_eq!(summarize_events_for_date(&events, date(2026, 5, 3)), None);
    }
}
