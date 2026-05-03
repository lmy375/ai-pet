//! User-set reminder parsing and due-checking (Iter QG5 extraction from
//! `proactive.rs`).
//!
//! Reminders are user instructions stored as `todo` memory items whose
//! `description` starts with `[remind: HH:MM] topic` or `[remind: YYYY-MM-DD
//! HH:MM] topic`. The proactive loop scans this category each tick and
//! surfaces due items into the prompt; the consolidate sweep deletes stale
//! Absolute targets after a cutoff. Both call into this module.
//!
//! Public surface is exactly what `proactive.rs` exported before the
//! extraction — `pub use` at the top of `proactive.rs` keeps callers like
//! `consolidate.rs` working without change.

/// What kind of time a parsed reminder is targeting. `TodayHour` is the lightweight form
/// the user writes for "later today" (or "early tomorrow morning, before I sleep"); the
/// due check wraps across midnight to support that. `Absolute` is the full date-qualified
/// form the LLM should write when the user says "tomorrow 9am" / "in 2 days" — those
/// can't be expressed by HH:MM alone.
#[derive(Debug, PartialEq, Eq)]
pub enum ReminderTarget {
    TodayHour(u8, u8),
    Absolute(chrono::NaiveDateTime),
}

/// Parse a "user-set reminder" prefix from a memory item's description. Convention:
///   - `[remind: HH:MM] topic`              — today (or wraps a few minutes past midnight)
///   - `[remind: YYYY-MM-DD HH:MM] topic`   — specific moment (24-hour clock)
///
/// Returns `(target, topic)` when the prefix parses cleanly, `None` otherwise.
pub fn parse_reminder_prefix(desc: &str) -> Option<(ReminderTarget, String)> {
    let trimmed = desc.trim_start();
    let after_open = trimmed.strip_prefix("[remind:")?;
    let close_idx = after_open.find(']')?;
    let inside = after_open[..close_idx].trim();
    let topic = after_open[close_idx + 1..].trim().to_string();
    if topic.is_empty() {
        return None;
    }
    // Try the date-qualified form first: "YYYY-MM-DD HH:MM" — has a space inside.
    if let Some((date_str, time_str)) = inside.split_once(' ') {
        let date = chrono::NaiveDate::parse_from_str(date_str.trim(), "%Y-%m-%d").ok()?;
        let time = chrono::NaiveTime::parse_from_str(time_str.trim(), "%H:%M").ok()?;
        return Some((ReminderTarget::Absolute(date.and_time(time)), topic));
    }
    // Fall back to today-style HH:MM.
    let (hh, mm) = inside.split_once(':')?;
    let hour: u8 = hh.trim().parse().ok()?;
    let minute: u8 = mm.trim().parse().ok()?;
    if hour > 23 || minute > 59 {
        return None;
    }
    Some((ReminderTarget::TodayHour(hour, minute), topic))
}

/// Returns true if the reminder time is past `now` by no more than `window_minutes`.
/// We don't fire reminders that are still in the future or that we missed by too much.
/// `TodayHour` form additionally wraps across midnight when the gap is small enough.
pub fn is_reminder_due(
    target: &ReminderTarget,
    now: chrono::NaiveDateTime,
    window_minutes: u64,
) -> bool {
    let window = chrono::Duration::minutes(window_minutes as i64);
    let zero = chrono::Duration::zero();
    match target {
        ReminderTarget::Absolute(dt) => {
            let delta = now - *dt;
            delta >= zero && delta <= window
        }
        ReminderTarget::TodayHour(h, m) => {
            let Some(today_t) = now.date().and_hms_opt(*h as u32, *m as u32, 0) else {
                return false;
            };
            let delta = now - today_t;
            if delta >= zero && delta <= window {
                return true;
            }
            // Maybe target was yesterday's HH:MM and we're early in the new day.
            let yesterday_t = today_t - chrono::Duration::days(1);
            let yd = now - yesterday_t;
            yd >= zero && yd <= window
        }
    }
}

/// Format a reminder target for display in prompt / panel. TodayHour shows just the
/// HH:MM (compact, since context is "today"); Absolute spells out the full date.
pub fn format_target(target: &ReminderTarget) -> String {
    match target {
        ReminderTarget::TodayHour(h, m) => format!("{:02}:{:02}", h, m),
        ReminderTarget::Absolute(dt) => dt.format("%Y-%m-%d %H:%M").to_string(),
    }
}

/// Whether this reminder is "stale" — past its target by more than `cutoff_hours`.
/// Only `Absolute` targets can go stale; `TodayHour` is intentionally recurring-friendly
/// and doesn't carry a creation date in the memory entry, so we never auto-delete those.
/// Used by the consolidate sweep to clean up forgotten one-shot reminders.
pub fn is_stale_reminder(
    target: &ReminderTarget,
    now: chrono::NaiveDateTime,
    cutoff_hours: u64,
) -> bool {
    match target {
        ReminderTarget::Absolute(dt) => {
            let cutoff = chrono::Duration::hours(cutoff_hours as i64);
            (now - *dt) > cutoff
        }
        ReminderTarget::TodayHour(_, _) => false,
    }
}

/// Pure formatter for the reminders hint block. Items are `(formatted_time, topic, title)`.
/// `redact` is applied to topic and title before they're inserted, so user-authored
/// reminder content (and the LLM-picked entry title) doesn't leak private terms back
/// into the prompt. Empty list returns empty string.
///
/// Iter QG4: gained the redaction pass — previously topic and title were formatted raw,
/// letting any privacy-pattern term in the user's reminder ("提醒我跟 {private name}
/// 喝咖啡") flow straight through to the next proactive prompt.
pub fn format_reminders_hint(
    items: &[(String, String, String)],
    redact: &dyn Fn(&str) -> String,
) -> String {
    if items.is_empty() {
        return String::new();
    }
    let mut lines = vec!["你有以下到期的用户提醒（请挑最相关的一条带进开口）：".to_string()];
    for (time, topic, title) in items {
        lines.push(format!(
            "· {} {}（条目标题: {}）",
            time,
            redact(topic),
            redact(title),
        ));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{NaiveDate, NaiveDateTime};

    fn ndt(y: i32, m: u32, d: u32, hh: u32, mm: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(hh, mm, 0)
            .unwrap()
    }

    #[test]
    fn parse_today_form() {
        let (target, topic) = parse_reminder_prefix("[remind: 23:00] 吃药").unwrap();
        assert_eq!(target, ReminderTarget::TodayHour(23, 0));
        assert_eq!(topic, "吃药");
    }

    #[test]
    fn parse_absolute_form() {
        let (target, topic) = parse_reminder_prefix("[remind: 2026-05-04 09:00] 项目早会").unwrap();
        assert_eq!(target, ReminderTarget::Absolute(ndt(2026, 5, 4, 9, 0)));
        assert_eq!(topic, "项目早会");
    }

    #[test]
    fn parse_tolerates_extra_whitespace() {
        let (target, topic) = parse_reminder_prefix("  [remind:  9:30  ]   去开会  ").unwrap();
        assert_eq!(target, ReminderTarget::TodayHour(9, 30));
        assert_eq!(topic, "去开会");
    }

    #[test]
    fn parse_rejects_empty_topic() {
        assert!(parse_reminder_prefix("[remind: 12:00]").is_none());
        assert!(parse_reminder_prefix("[remind: 2026-05-04 09:00]").is_none());
    }

    #[test]
    fn parse_rejects_invalid_time() {
        assert!(parse_reminder_prefix("[remind: 25:00] hi").is_none());
        assert!(parse_reminder_prefix("[remind: 9:60] hi").is_none());
        assert!(parse_reminder_prefix("[remind: x:y] hi").is_none());
        assert!(parse_reminder_prefix("[remind: 2026-13-01 09:00] hi").is_none());
        assert!(parse_reminder_prefix("[remind: 2026-05-04 25:00] hi").is_none());
    }

    #[test]
    fn parse_no_prefix_returns_none() {
        assert!(parse_reminder_prefix("just a regular note").is_none());
        assert!(parse_reminder_prefix("[other] not a reminder").is_none());
    }

    // ---- TodayHour due semantics ----

    #[test]
    fn today_hour_within_window() {
        let target = ReminderTarget::TodayHour(12, 0);
        assert!(is_reminder_due(&target, ndt(2026, 5, 3, 12, 5), 30));
    }

    #[test]
    fn today_hour_at_exact_target() {
        let target = ReminderTarget::TodayHour(12, 0);
        assert!(is_reminder_due(&target, ndt(2026, 5, 3, 12, 0), 30));
    }

    #[test]
    fn today_hour_future_not_due() {
        let target = ReminderTarget::TodayHour(12, 0);
        assert!(!is_reminder_due(&target, ndt(2026, 5, 3, 11, 55), 30));
    }

    #[test]
    fn today_hour_too_far_past_not_due() {
        let target = ReminderTarget::TodayHour(8, 0);
        assert!(!is_reminder_due(&target, ndt(2026, 5, 3, 11, 0), 30));
    }

    #[test]
    fn today_hour_wraps_midnight() {
        // target 23:55 yesterday-relative; now 00:05 today → 10 min past → due.
        let target = ReminderTarget::TodayHour(23, 55);
        assert!(is_reminder_due(&target, ndt(2026, 5, 3, 0, 5), 30));
    }

    // ---- Absolute due semantics ----

    #[test]
    fn absolute_within_window() {
        let target = ReminderTarget::Absolute(ndt(2026, 5, 4, 9, 0));
        assert!(is_reminder_due(&target, ndt(2026, 5, 4, 9, 10), 30));
    }

    #[test]
    fn absolute_future_not_due() {
        let target = ReminderTarget::Absolute(ndt(2026, 5, 4, 9, 0));
        assert!(!is_reminder_due(&target, ndt(2026, 5, 3, 23, 0), 30));
    }

    #[test]
    fn absolute_far_past_not_due() {
        let target = ReminderTarget::Absolute(ndt(2026, 5, 1, 9, 0));
        assert!(!is_reminder_due(&target, ndt(2026, 5, 4, 9, 0), 30));
    }

    #[test]
    fn absolute_does_not_wrap_midnight() {
        // Absolute is anchored to a specific date — no wrap. 23:55 May 1 vs now 00:05 May 3
        // is over a day late, must be False.
        let target = ReminderTarget::Absolute(ndt(2026, 5, 1, 23, 55));
        assert!(!is_reminder_due(&target, ndt(2026, 5, 3, 0, 5), 30));
    }

    // ---- stale reminder ----

    #[test]
    fn absolute_stale_after_cutoff() {
        // Target was May 1 09:00; now is May 2 10:00 = 25h past, cutoff 24h → stale.
        let target = ReminderTarget::Absolute(ndt(2026, 5, 1, 9, 0));
        assert!(is_stale_reminder(&target, ndt(2026, 5, 2, 10, 0), 24));
    }

    #[test]
    fn absolute_within_cutoff_not_stale() {
        // Target May 1 09:00; now May 2 08:00 = 23h past → not stale at 24h cutoff.
        let target = ReminderTarget::Absolute(ndt(2026, 5, 1, 9, 0));
        assert!(!is_stale_reminder(&target, ndt(2026, 5, 2, 8, 0), 24));
    }

    #[test]
    fn absolute_in_future_not_stale() {
        let target = ReminderTarget::Absolute(ndt(2026, 5, 4, 9, 0));
        assert!(!is_stale_reminder(&target, ndt(2026, 5, 3, 12, 0), 24));
    }

    #[test]
    fn today_hour_never_stale() {
        // TodayHour is intentionally recurring-friendly — never auto-purged.
        let target = ReminderTarget::TodayHour(9, 0);
        assert!(!is_stale_reminder(&target, ndt(2026, 5, 3, 12, 0), 24));
    }
}
