//! Butler-tasks schedule parsing, due-checking, completion sweep helpers, and
//! the prompt-block formatter (Iter QG5b extraction from `proactive.rs`).
//!
//! Public surface is identical to what `proactive.rs` exported before the
//! extraction — `pub use self::butler_schedule::*` at the top of `proactive.rs`
//! keeps callers (`consolidate.rs::sweep_completed_once_butler_tasks`,
//! `proactive::build_butler_tasks_hint`) reaching the same paths.
//!
//! The IO-bound `build_butler_tasks_hint` (memory_list + redaction) intentionally
//! stays in `proactive.rs`, mirroring how `build_reminders_hint` was kept there
//! during QG5a — pure formatters move, env-touching functions stay.

/// Cap on how many `butler_tasks` entries to surface in the proactive prompt. Above
/// this the block dominates the prompt; the LLM can still call `memory_list` to see
/// the full backlog if it needs to triage.
pub const BUTLER_TASKS_HINT_MAX_ITEMS: usize = 6;
/// Per-task description char cap. Long task specs become noisy when stacked; the
/// detail.md is one read_file call away when the LLM actually picks one up.
pub const BUTLER_TASKS_HINT_DESC_CHARS: usize = 100;

/// Pure formatter for the butler-tasks block. Items are `(title, description, updated_at)`.
/// Items with a `[every: HH:MM]` / `[once: ...]` prefix that are *due now* (per
/// `is_butler_due`) bubble to the top with a "⏰ 到期" marker; the rest follow sorted by
/// `updated_at` ascending so the oldest pending tasks aren't lost at the bottom.
/// Empty list / zero cap → empty string.
///
/// Iter Cγ introduced the block; Iter Cζ added schedule-awareness via `now`. Distinct
/// from reminders_hint (user's nudges) — this is the pet's own assignment queue.
pub fn format_butler_tasks_block(
    items: &[(String, String, String)],
    max_items: usize,
    max_desc_chars: usize,
    now: chrono::NaiveDateTime,
) -> String {
    if items.is_empty() || max_items == 0 {
        return String::new();
    }
    // Compute due-ness + error state once per item and stable-sort.
    let mut annotated: Vec<(&(String, String, String), bool, bool)> = items
        .iter()
        .map(|i| {
            let due = parse_butler_schedule_prefix(&i.1)
                .map(|(sched, _)| is_butler_due(&sched, now, &i.2))
                .unwrap_or(false);
            let errored = has_butler_error(&i.1);
            (i, due, errored)
        })
        .collect();
    // Due → not-due primary, updated_at ascending secondary. Errored items keep
    // their primary slot — they're often also due (last execution failed) so they
    // bubble up naturally; if not due, they stay in normal order so the user
    // doesn't drown in stale errors.
    annotated.sort_by(|(a, a_due, _), (b, b_due, _)| match (a_due, b_due) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.2.cmp(&b.2),
    });
    let n = annotated.len().min(max_items);
    let due_count = annotated.iter().take(n).filter(|(_, d, _)| *d).count();
    let err_count = annotated.iter().take(n).filter(|(_, _, e)| *e).count();
    let mut lines: Vec<String> = Vec::with_capacity(n + 2);
    let header = match (due_count, err_count) {
        (0, 0) => format!("用户委托给你的管家任务（共 {} 条，按最早委托排在前）：", n),
        (d, 0) => format!(
            "用户委托给你的管家任务（共 {} 条，其中 {} 条到期，按到期 → 最早委托排在前）：",
            n, d
        ),
        (0, e) => format!(
            "用户委托给你的管家任务（共 {} 条，其中 {} 条上次执行失败需要复查）：",
            n, e
        ),
        (d, e) => format!(
            "用户委托给你的管家任务（共 {} 条，{} 条到期、{} 条上次失败）：",
            n, d, e
        ),
    };
    lines.push(header);
    for ((title, desc, _), due, errored) in annotated.iter().take(n) {
        let trimmed = desc.trim();
        let truncated: String = if trimmed.chars().count() <= max_desc_chars {
            trimmed.to_string()
        } else {
            let head: String = trimmed.chars().take(max_desc_chars).collect();
            format!("{}…", head)
        };
        // Marker order: error first (most urgent) → due second. Both can co-occur.
        let mut marker = String::new();
        if *errored {
            marker.push_str("❌ 错误 · ");
        }
        if *due {
            marker.push_str("⏰ 到期 · ");
        }
        lines.push(format!("- {}{}：{}", marker, title.trim(), truncated));
    }
    lines.push(
        "执行完一项后用 `memory_edit update` 更新进度（标题前加 [done] / 写最后执行时间），\
完全不需要的用 `memory_edit delete` 移除。带 `[every: HH:MM]` 或 `[once: ...]` 前缀的任务标记了到期窗口——\
看到「⏰ 到期」就该这一轮优先处理它。\n\
**记得在你这一轮的开口里简短提一下**：「我帮你写好 today.md 了」「Downloads 整理完了」之类——\
不必描述细节、一句话即可。让用户从 bubble 里直接看到管家工作的反馈，而不是必须打开 panel 才发现你做了事。\n\
**执行失败处理**：如果你这一轮调用 read_file / write_file / edit_file / bash 时失败（文件不存在、权限不够、命令报错等），\
用 `memory_edit update` 在 description 里加一段 `[error: 简短原因]`（保留原有 `[every:]` / `[once:]` 前缀，error 段贴在它后面）。\
下次重试成功时记得移除这段 error 标记。看到「❌ 错误」标记的任务说明上次失败了，请检查描述里的失败原因再决定要不要重试。"
            .to_string(),
    );
    lines.join("\n")
}

/// Iter Cπ: detect whether a butler task description is currently flagged as errored.
/// Convention: LLM prepends or embeds `[error: brief reason]` after a tool failure
/// during execution. We only check the substring `[error` — case-sensitive, no
/// regex — to keep this cheap and tolerant of `[error:`, `[error :`, `[error]`
/// variants the LLM might write.
pub fn has_butler_error(desc: &str) -> bool {
    desc.contains("[error")
}

/// Schedule for a butler task (Iter Cζ). Distinct from `ReminderTarget` semantically:
/// reminders are nudges *for the user*, schedules tell *the pet* when to act on a task.
/// Both share the time-arithmetic shape, but the firing logic and "already done" check
/// differ — schedules need to know whether the most recent fire already triggered work.
#[derive(Debug, PartialEq, Eq)]
pub enum ButlerSchedule {
    /// Daily recurring at HH:MM local. Implicit window — see `is_butler_due` for how it
    /// resolves "already executed today".
    Every(u8, u8),
    /// Single-fire at the absolute moment.
    Once(chrono::NaiveDateTime),
}

/// Parse a schedule prefix from a butler_tasks description. Conventions:
///   - `[every: HH:MM] topic`              — daily recurring
///   - `[once: YYYY-MM-DD HH:MM] topic`    — one-shot
///
/// Returns `(schedule, topic)` on clean parse, `None` otherwise. Tasks without a prefix
/// are unscheduled — the LLM picks them up on its own judgment, not by clock.
pub fn parse_butler_schedule_prefix(desc: &str) -> Option<(ButlerSchedule, String)> {
    let trimmed = desc.trim_start();
    if let Some(after_open) = trimmed.strip_prefix("[every:") {
        let close_idx = after_open.find(']')?;
        let inside = after_open[..close_idx].trim();
        let topic = after_open[close_idx + 1..].trim().to_string();
        if topic.is_empty() {
            return None;
        }
        let (hh, mm) = inside.split_once(':')?;
        let hour: u8 = hh.trim().parse().ok()?;
        let minute: u8 = mm.trim().parse().ok()?;
        if hour > 23 || minute > 59 {
            return None;
        }
        return Some((ButlerSchedule::Every(hour, minute), topic));
    }
    if let Some(after_open) = trimmed.strip_prefix("[once:") {
        let close_idx = after_open.find(']')?;
        let inside = after_open[..close_idx].trim();
        let topic = after_open[close_idx + 1..].trim().to_string();
        if topic.is_empty() {
            return None;
        }
        let (date_str, time_str) = inside.split_once(' ')?;
        let date = chrono::NaiveDate::parse_from_str(date_str.trim(), "%Y-%m-%d").ok()?;
        let time = chrono::NaiveTime::parse_from_str(time_str.trim(), "%H:%M").ok()?;
        return Some((ButlerSchedule::Once(date.and_time(time)), topic));
    }
    None
}

// -- Iter R77: deadline prefix ------------------------------------------------

/// Iter R77: deadline-bound butler task. Distinct from `[once:]` which means
/// "execute at this time" — a deadline means "user must complete this BEFORE
/// this time". Pet doesn't auto-execute a deadline task; pet *reminds* the user
/// as it approaches. Format: `[deadline: YYYY-MM-DD HH:MM] description`.
pub fn parse_butler_deadline_prefix(desc: &str) -> Option<(chrono::NaiveDateTime, String)> {
    let trimmed = desc.trim_start();
    let after_open = trimmed.strip_prefix("[deadline:")?;
    let close_idx = after_open.find(']')?;
    let inside = after_open[..close_idx].trim();
    let topic = after_open[close_idx + 1..].trim().to_string();
    if topic.is_empty() {
        return None;
    }
    let (date_str, time_str) = inside.split_once(' ')?;
    let date = chrono::NaiveDate::parse_from_str(date_str.trim(), "%Y-%m-%d").ok()?;
    let time = chrono::NaiveTime::parse_from_str(time_str.trim(), "%H:%M").ok()?;
    Some((date.and_time(time), topic))
}

/// Iter R77: deadline urgency tier. Drives whether to surface the deadline in
/// the prompt and what tone to use. Distant tasks shouldn't crowd the prompt;
/// imminent / overdue ones earn a directive nudge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeadlineUrgency {
    /// > 6 hours away. Don't surface — too far to be actionable in this turn.
    Distant,
    /// 1-6 hours away. Surface gently — "deadline 在 N 小时后".
    Approaching,
    /// 0-1 hours (inclusive). Surface urgently — "deadline 还有 N 分钟".
    Imminent,
    /// Past the deadline. Surface as overdue.
    Overdue,
}

/// Iter R78: pure helper counting deadlines that are Imminent (<1h) or Overdue
/// in a list of `(deadline, _topic)` pairs. Approaching (1-6h away) and
/// Distant don't count — chip surface is for "must pay attention now",
/// not awareness-of-the-day. Pure / testable.
pub fn count_urgent_butler_deadlines(
    items: &[(chrono::NaiveDateTime, String)],
    now: chrono::NaiveDateTime,
) -> u64 {
    items
        .iter()
        .map(|(d, _)| compute_deadline_urgency(*d, now))
        .filter(|u| matches!(u, DeadlineUrgency::Imminent | DeadlineUrgency::Overdue))
        .count() as u64
}

/// Iter R77: pure classifier. `now < deadline by ≤ 1h` → Imminent; `1-6h ahead`
/// → Approaching; `> 6h ahead` → Distant; `now ≥ deadline` → Overdue. Pure /
/// testable — caller passes both args.
pub fn compute_deadline_urgency(
    deadline: chrono::NaiveDateTime,
    now: chrono::NaiveDateTime,
) -> DeadlineUrgency {
    if now >= deadline {
        return DeadlineUrgency::Overdue;
    }
    let delta = deadline - now;
    let hours = delta.num_hours();
    if hours >= 6 {
        DeadlineUrgency::Distant
    } else if hours >= 1 {
        DeadlineUrgency::Approaching
    } else {
        DeadlineUrgency::Imminent
    }
}

/// Iter R77: format the deadline-hint section of the proactive prompt. Filters
/// to non-Distant items only (Approaching / Imminent / Overdue), then renders
/// them as bullet lines. Empty when nothing actionable is on the horizon.
/// Pure — caller passes the prefiltered (deadline, topic) pairs + now.
pub fn format_butler_deadlines_hint(
    items: &[(chrono::NaiveDateTime, String)],
    now: chrono::NaiveDateTime,
) -> String {
    let mut bullets: Vec<String> = Vec::new();
    for (deadline, topic) in items {
        let urgency = compute_deadline_urgency(*deadline, now);
        let bullet = match urgency {
            DeadlineUrgency::Distant => continue,
            DeadlineUrgency::Approaching => {
                let hours = (*deadline - now).num_hours().max(1);
                format!("· {}（约 {} 小时后到 deadline）", topic, hours)
            }
            DeadlineUrgency::Imminent => {
                let mins = (*deadline - now).num_minutes().max(0);
                format!("· {}（仅剩 {} 分钟到 deadline）", topic, mins)
            }
            DeadlineUrgency::Overdue => {
                let mins = (now - *deadline).num_minutes();
                if mins < 60 {
                    format!("· {}（deadline 已过 {} 分钟）", topic, mins)
                } else {
                    let hours = mins / 60;
                    format!("· {}（deadline 已过 {} 小时）", topic, hours)
                }
            }
        };
        bullets.push(bullet);
    }
    if bullets.is_empty() {
        return String::new();
    }
    format!(
        "[逼近的 deadline]\n{}\n如果用户当前没在专注其他事，可以提一下；如果在专注中，仅在 imminent / overdue 时才打断。",
        bullets.join("\n")
    )
}

/// Parse a stored `updated_at` string ("YYYY-MM-DDTHH:MM:SS+HH:MM") to a local
/// `NaiveDateTime`. Returns `None` on malformed input — caller decides what that
/// means (typically "treat as never updated").
fn parse_updated_at_local(s: &str) -> Option<chrono::NaiveDateTime> {
    chrono::DateTime::parse_from_rfc3339(s.trim())
        .ok()
        .map(|dt| dt.with_timezone(&chrono::Local).naive_local())
}

/// Decide whether a scheduled butler task is *due now* — past its most recent fire AND
/// not yet executed since that fire. `last_updated` is the task's `updated_at`; an
/// unparseable / empty value is treated as "never executed" (always due if past target).
///
/// Semantics:
/// - `Every(h, m)`: most-recent-fire = today h:m if `now >= today h:m` else yesterday h:m.
///   Due iff `last_updated < most-recent-fire`. So a task touched after today's fire is
///   suppressed until tomorrow's; a task touched before today's fire is due now.
/// - `Once(dt)`: due iff `now >= dt && last_updated < dt`. Past + unexecuted.
pub fn is_butler_due(
    schedule: &ButlerSchedule,
    now: chrono::NaiveDateTime,
    last_updated: &str,
) -> bool {
    let last = parse_updated_at_local(last_updated);
    match schedule {
        ButlerSchedule::Once(dt) => {
            if now < *dt {
                return false;
            }
            match last {
                Some(u) => u < *dt,
                None => true, // unparseable → never executed → due
            }
        }
        ButlerSchedule::Every(h, m) => {
            let today = now.date();
            let target_today = match today.and_hms_opt(*h as u32, *m as u32, 0) {
                Some(t) => t,
                None => return false, // shouldn't happen — parser bounds-checks
            };
            let most_recent_fire = if now >= target_today {
                target_today
            } else {
                target_today - chrono::Duration::days(1)
            };
            match last {
                Some(u) => u < most_recent_fire,
                None => true, // never updated → due (will need first execution)
            }
        }
    }
}

/// Iter Cλ: pure decider — given a butler task's description, updated_at, current
/// time, and grace hours, return true iff this is a `[once: ...]` task that has
/// been executed (updated_at >= target) AND is now safely past the configured
/// retention grace period. Used by `sweep_completed_once_butler_tasks` so the
/// consolidate loop can auto-clean finished one-shot tasks the way it already
/// cleans stale reminders. Recurring `[every: ...]` tasks return false — they
/// re-fire and shouldn't be deleted.
pub fn is_completed_once(
    desc: &str,
    last_updated: &str,
    now: chrono::NaiveDateTime,
    grace_hours: u64,
) -> bool {
    let Some((sched, _)) = parse_butler_schedule_prefix(desc) else {
        return false;
    };
    let target = match sched {
        ButlerSchedule::Once(dt) => dt,
        ButlerSchedule::Every(_, _) => return false,
    };
    let Some(last) = parse_updated_at_local(last_updated) else {
        return false;
    };
    if last < target {
        return false; // executed before target = invalid; treat as not-yet-done
    }
    let grace_end = target + chrono::Duration::hours(grace_hours as i64);
    now >= grace_end
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed_now() -> chrono::NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(2026, 5, 3)
            .unwrap()
            .and_hms_opt(14, 30, 0)
            .unwrap()
    }

    #[test]
    fn format_butler_tasks_block_empty_returns_empty() {
        assert_eq!(
            format_butler_tasks_block(&[], 6, 100, fixed_now()),
            String::new()
        );
    }

    #[test]
    fn format_butler_tasks_block_zero_max_returns_empty() {
        let items = vec![("t".into(), "d".into(), "2026-05-03T10:00:00+08:00".into())];
        assert_eq!(
            format_butler_tasks_block(&items, 0, 100, fixed_now()),
            String::new()
        );
    }

    #[test]
    fn format_butler_tasks_block_sorts_oldest_first() {
        let items = vec![
            (
                "新任务".into(),
                "d-new".into(),
                "2026-05-03T10:00:00+08:00".into(),
            ),
            (
                "老任务".into(),
                "d-old".into(),
                "2026-04-01T10:00:00+08:00".into(),
            ),
            (
                "中任务".into(),
                "d-mid".into(),
                "2026-04-20T10:00:00+08:00".into(),
            ),
        ];
        let out = format_butler_tasks_block(&items, 6, 100, fixed_now());
        let old_idx = out.find("老任务").unwrap();
        let mid_idx = out.find("中任务").unwrap();
        let new_idx = out.find("新任务").unwrap();
        assert!(
            old_idx < mid_idx,
            "oldest should be first (don't let tasks rot)"
        );
        assert!(mid_idx < new_idx);
    }

    #[test]
    fn format_butler_tasks_block_footer_teaches_speech_mention() {
        // Iter D6: pin the "记得在开口里简短提一下" guidance so a future refactor
        // can't silently drop it.
        let items = vec![(
            "morning-report".into(),
            "[every: 09:00] write today.md".into(),
            "2026-05-03T09:30:00+08:00".into(),
        )];
        let out = format_butler_tasks_block(&items, 6, 100, fixed_now());
        assert!(
            out.contains("记得在你这一轮的开口里简短提一下") || out.contains("简短提一下"),
            "footer should instruct LLM to mention butler execution in its speech"
        );
        assert!(
            out.contains("我帮你") || out.contains("整理完了"),
            "footer should give concrete example phrasings"
        );
    }

    #[test]
    fn format_butler_tasks_block_caps_count_and_includes_footer() {
        let items: Vec<(String, String, String)> = (0..10)
            .map(|i| {
                (
                    format!("task-{i}"),
                    format!("desc-{i}"),
                    format!("2026-05-{:02}T10:00:00+08:00", i + 1),
                )
            })
            .collect();
        let out = format_butler_tasks_block(&items, 3, 100, fixed_now());
        assert!(out.contains("共 3 条"));
        assert!(out.contains("task-0"));
        assert!(out.contains("task-2"));
        assert!(!out.contains("task-3"), "4th-oldest should be excluded");
        assert!(
            out.contains("memory_edit update") || out.contains("memory_edit delete"),
            "footer should tell LLM how to retire completed tasks"
        );
    }

    #[test]
    fn has_butler_error_detects_marker() {
        assert!(has_butler_error("[error: file not found] write report"));
        assert!(has_butler_error(
            "[every: 09:00] [error: permission denied] morning"
        ));
        assert!(has_butler_error("some text [error] more text"));
        assert!(has_butler_error("[error :spaced] x"));
    }

    #[test]
    fn has_butler_error_negative_cases() {
        assert!(!has_butler_error(""));
        assert!(!has_butler_error("normal task description"));
        assert!(!has_butler_error("[every: 09:00] write daily.md"));
        assert!(!has_butler_error("[once: 2026-05-10 14:00] one-shot"));
        assert!(!has_butler_error("had an error earlier but recovered"));
    }

    #[test]
    fn format_butler_tasks_block_marks_errored_tasks() {
        let items = vec![(
            "morning-report".into(),
            "[every: 09:00] [error: file not found] write today.md".into(),
            "2026-05-03T09:30:00+08:00".into(),
        )];
        let out = format_butler_tasks_block(&items, 6, 200, fixed_now());
        assert!(out.contains("❌ 错误"));
        let header = out.lines().next().unwrap();
        assert!(header.contains("上次执行失败"), "header: {}", header);
    }

    #[test]
    fn format_butler_tasks_block_due_and_errored_co_occur() {
        let items = vec![(
            "report".into(),
            "[every: 09:00] [error: prev fail] write today.md".into(),
            "2026-05-02T08:00:00+08:00".into(),
        )];
        let out = format_butler_tasks_block(&items, 6, 200, fixed_now());
        let body = out.lines().nth(1).unwrap();
        let err_idx = body.find("❌ 错误").unwrap();
        let due_idx = body.find("⏰ 到期").unwrap();
        assert!(err_idx < due_idx, "error marker should precede due marker");
        let header = out.lines().next().unwrap();
        assert!(header.contains("到期"));
        assert!(header.contains("失败"));
    }

    #[test]
    fn format_butler_tasks_block_truncates_long_descriptions() {
        let long = "条".repeat(150);
        let items = vec![("task".into(), long, "2026-05-03T10:00:00+08:00".into())];
        let out = format_butler_tasks_block(&items, 6, 30, fixed_now());
        assert!(out.contains("…"));
        let body_chars = out
            .lines()
            .nth(1)
            .unwrap()
            .chars()
            .filter(|c| *c == '条')
            .count();
        assert_eq!(body_chars, 30);
    }

    #[test]
    fn format_butler_tasks_block_due_task_bubbles_to_top_with_marker() {
        let items = vec![
            (
                "plain-old".into(),
                "do something whenever".into(),
                "2026-04-01T08:00:00+08:00".into(),
            ),
            (
                "morning-report".into(),
                "[every: 09:00] write today.md".into(),
                "2026-05-02T09:30:00+08:00".into(),
            ),
        ];
        let out = format_butler_tasks_block(&items, 6, 100, fixed_now());
        assert!(
            out.contains("⏰ 到期"),
            "due task should carry ⏰ 到期 marker"
        );
        assert!(out.contains("其中 1 条到期"));
        let due_idx = out.find("morning-report").unwrap();
        let plain_idx = out.find("plain-old").unwrap();
        assert!(due_idx < plain_idx, "due task ranks above plain older one");
    }

    fn count_task_lines_with_marker(out: &str) -> usize {
        out.lines()
            .filter(|l| l.starts_with("- ") && l.contains("⏰ 到期 · "))
            .count()
    }

    #[test]
    fn format_butler_tasks_block_already_done_today_not_due() {
        let items = vec![(
            "morning-report".into(),
            "[every: 09:00] write today.md".into(),
            "2026-05-03T09:15:00+08:00".into(),
        )];
        let out = format_butler_tasks_block(&items, 6, 100, fixed_now());
        assert_eq!(
            count_task_lines_with_marker(&out),
            0,
            "no task line should carry the marker"
        );
        let header = out.lines().next().unwrap();
        assert!(!header.contains("条到期"), "header: {}", header);
        assert!(out.contains("morning-report"));
    }

    #[test]
    fn parse_butler_schedule_prefix_parses_every() {
        let (sched, topic) = parse_butler_schedule_prefix("[every: 09:00] write today.md").unwrap();
        assert_eq!(sched, ButlerSchedule::Every(9, 0));
        assert_eq!(topic, "write today.md");
    }

    #[test]
    fn parse_butler_schedule_prefix_parses_once() {
        let (sched, topic) =
            parse_butler_schedule_prefix("[once: 2026-05-10 14:00] one-shot").unwrap();
        let expected = chrono::NaiveDate::from_ymd_opt(2026, 5, 10)
            .unwrap()
            .and_hms_opt(14, 0, 0)
            .unwrap();
        assert_eq!(sched, ButlerSchedule::Once(expected));
        assert_eq!(topic, "one-shot");
    }

    #[test]
    fn parse_butler_schedule_prefix_rejects_malformed() {
        assert!(parse_butler_schedule_prefix("no prefix").is_none());
        assert!(parse_butler_schedule_prefix("[every: 25:00] x").is_none());
        assert!(parse_butler_schedule_prefix("[every: 09:60] x").is_none());
        assert!(parse_butler_schedule_prefix("[once: not-a-date] x").is_none());
        assert!(
            parse_butler_schedule_prefix("[every: 09:00]").is_none(),
            "empty topic"
        );
        assert!(parse_butler_schedule_prefix("[remind: 09:00] reminder").is_none());
    }

    #[test]
    fn is_butler_due_every_basic_window() {
        let now = fixed_now();
        assert!(is_butler_due(
            &ButlerSchedule::Every(9, 0),
            now,
            "2026-05-02T08:00:00+08:00"
        ));
        assert!(is_butler_due(
            &ButlerSchedule::Every(9, 0),
            now,
            "2026-05-02T10:00:00+08:00"
        ));
        assert!(!is_butler_due(
            &ButlerSchedule::Every(9, 0),
            now,
            "2026-05-03T09:30:00+08:00"
        ));
    }

    #[test]
    fn is_butler_due_every_before_today_target() {
        let now = chrono::NaiveDate::from_ymd_opt(2026, 5, 3)
            .unwrap()
            .and_hms_opt(8, 0, 0)
            .unwrap();
        assert!(!is_butler_due(
            &ButlerSchedule::Every(9, 0),
            now,
            "2026-05-02T09:30:00+08:00"
        ));
        assert!(is_butler_due(
            &ButlerSchedule::Every(9, 0),
            now,
            "2026-05-01T08:00:00+08:00"
        ));
    }

    #[test]
    fn is_butler_due_once_semantics() {
        let now = fixed_now();
        let target = ButlerSchedule::Once(
            chrono::NaiveDate::from_ymd_opt(2026, 5, 3)
                .unwrap()
                .and_hms_opt(10, 0, 0)
                .unwrap(),
        );
        assert!(is_butler_due(&target, now, ""));
        assert!(is_butler_due(&target, now, "2026-05-03T09:00:00+08:00"));
        assert!(!is_butler_due(&target, now, "2026-05-03T11:00:00+08:00"));
        let future = ButlerSchedule::Once(
            chrono::NaiveDate::from_ymd_opt(2026, 5, 10)
                .unwrap()
                .and_hms_opt(10, 0, 0)
                .unwrap(),
        );
        assert!(!is_butler_due(&future, now, ""));
    }

    #[test]
    fn is_completed_once_basic_flow() {
        let desc = "[once: 2026-05-03 10:00] do something";
        let target_done = "2026-05-03T10:30:00+08:00";
        let now1 = chrono::NaiveDate::from_ymd_opt(2026, 5, 3)
            .unwrap()
            .and_hms_opt(11, 30, 0)
            .unwrap();
        assert!(!is_completed_once(desc, target_done, now1, 48));
        let now2 = chrono::NaiveDate::from_ymd_opt(2026, 5, 5)
            .unwrap()
            .and_hms_opt(11, 0, 0)
            .unwrap();
        assert!(is_completed_once(desc, target_done, now2, 48));
    }

    #[test]
    fn is_completed_once_not_yet_executed() {
        let desc = "[once: 2026-05-03 10:00] do something";
        let last = "2026-05-02T08:00:00+08:00";
        let now = chrono::NaiveDate::from_ymd_opt(2026, 5, 6)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        assert!(!is_completed_once(desc, last, now, 48));
    }

    #[test]
    fn is_completed_once_skips_every_tasks() {
        let desc = "[every: 09:00] daily report";
        let last = "2026-05-03T09:30:00+08:00";
        let now = chrono::NaiveDate::from_ymd_opt(2026, 5, 10)
            .unwrap()
            .and_hms_opt(15, 0, 0)
            .unwrap();
        assert!(!is_completed_once(desc, last, now, 48));
    }

    #[test]
    fn is_completed_once_skips_unprefixed_tasks() {
        let desc = "no schedule prefix here";
        let last = "2026-05-03T09:30:00+08:00";
        let now = chrono::NaiveDate::from_ymd_opt(2026, 5, 10)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        assert!(!is_completed_once(desc, last, now, 48));
    }

    #[test]
    fn is_completed_once_unparseable_updated_at_keeps_task() {
        let desc = "[once: 2026-05-03 10:00] x";
        let now = chrono::NaiveDate::from_ymd_opt(2026, 6, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        assert!(!is_completed_once(desc, "garbage", now, 48));
        assert!(!is_completed_once(desc, "", now, 48));
    }

    #[test]
    fn is_butler_due_unparseable_updated_at_treated_as_never() {
        let now = fixed_now();
        assert!(is_butler_due(
            &ButlerSchedule::Every(9, 0),
            now,
            "not-a-timestamp"
        ));
        assert!(is_butler_due(&ButlerSchedule::Every(9, 0), now, ""));
    }

    // -- Iter R77: deadline parsing + urgency + format ----------------------

    fn dt(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> chrono::NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(year, month, day)
            .unwrap()
            .and_hms_opt(hour, minute, 0)
            .unwrap()
    }

    #[test]
    fn parse_deadline_clean() {
        let (when, topic) =
            parse_butler_deadline_prefix("[deadline: 2026-05-10 14:00] reply to email").unwrap();
        assert_eq!(when, dt(2026, 5, 10, 14, 0));
        assert_eq!(topic, "reply to email");
    }

    #[test]
    fn parse_deadline_rejects_malformed() {
        assert!(parse_butler_deadline_prefix("no prefix").is_none());
        assert!(parse_butler_deadline_prefix("[deadline: not-a-date 14:00] x").is_none());
        assert!(parse_butler_deadline_prefix("[deadline: 2026-05-10 25:00] x").is_none());
        assert!(parse_butler_deadline_prefix("[deadline: 2026-05-10 14:00]").is_none());
        // Wrong prefix kind shouldn't match.
        assert!(parse_butler_deadline_prefix("[once: 2026-05-10 14:00] x").is_none());
        assert!(parse_butler_deadline_prefix("[every: 09:00] x").is_none());
    }

    #[test]
    fn urgency_overdue_when_now_at_or_past_deadline() {
        let dl = dt(2026, 5, 10, 14, 0);
        assert_eq!(compute_deadline_urgency(dl, dl), DeadlineUrgency::Overdue);
        assert_eq!(
            compute_deadline_urgency(dl, dt(2026, 5, 10, 15, 0)),
            DeadlineUrgency::Overdue
        );
    }

    #[test]
    fn urgency_imminent_within_one_hour() {
        let dl = dt(2026, 5, 10, 14, 0);
        // 30 min away.
        assert_eq!(
            compute_deadline_urgency(dl, dt(2026, 5, 10, 13, 30)),
            DeadlineUrgency::Imminent
        );
        // 59 min away — still Imminent (< 1h boundary).
        assert_eq!(
            compute_deadline_urgency(dl, dt(2026, 5, 10, 13, 1)),
            DeadlineUrgency::Imminent
        );
    }

    #[test]
    fn urgency_approaching_between_1_and_6_hours() {
        let dl = dt(2026, 5, 10, 14, 0);
        // 1h 30min away.
        assert_eq!(
            compute_deadline_urgency(dl, dt(2026, 5, 10, 12, 30)),
            DeadlineUrgency::Approaching
        );
        // Exactly 6h away — boundary case (≥ 6 → Distant per impl).
        assert_eq!(
            compute_deadline_urgency(dl, dt(2026, 5, 10, 8, 0)),
            DeadlineUrgency::Distant
        );
        // 5h 30min away — Approaching.
        assert_eq!(
            compute_deadline_urgency(dl, dt(2026, 5, 10, 8, 30)),
            DeadlineUrgency::Approaching
        );
    }

    #[test]
    fn urgency_distant_when_far_away() {
        let dl = dt(2026, 5, 10, 14, 0);
        assert_eq!(
            compute_deadline_urgency(dl, dt(2026, 5, 9, 14, 0)),
            DeadlineUrgency::Distant
        );
    }

    #[test]
    fn format_deadlines_hint_skips_distant_only() {
        let now = dt(2026, 5, 10, 8, 0);
        let items = vec![(dt(2026, 5, 12, 14, 0), "tomorrow's report".to_string())];
        assert_eq!(format_butler_deadlines_hint(&items, now), "");
    }

    #[test]
    fn format_deadlines_hint_renders_imminent_with_minutes() {
        let now = dt(2026, 5, 10, 13, 30);
        let items = vec![(dt(2026, 5, 10, 14, 0), "send draft".to_string())];
        let out = format_butler_deadlines_hint(&items, now);
        assert!(out.contains("[逼近的 deadline]"));
        assert!(out.contains("send draft"));
        assert!(out.contains("仅剩 30 分钟"));
    }

    #[test]
    fn format_deadlines_hint_renders_approaching_with_hours() {
        let now = dt(2026, 5, 10, 11, 0);
        let items = vec![(dt(2026, 5, 10, 14, 0), "review PR".to_string())];
        let out = format_butler_deadlines_hint(&items, now);
        assert!(out.contains("review PR"));
        assert!(out.contains("约 3 小时后"));
    }

    #[test]
    fn format_deadlines_hint_renders_overdue_minutes_then_hours() {
        let now = dt(2026, 5, 10, 14, 30);
        let items = vec![(dt(2026, 5, 10, 14, 0), "overdue thing".to_string())];
        let out = format_butler_deadlines_hint(&items, now);
        assert!(out.contains("已过 30 分钟"));
        // Overdue > 1 hour → format in hours.
        let now2 = dt(2026, 5, 10, 17, 0);
        let out2 = format_butler_deadlines_hint(&items, now2);
        assert!(out2.contains("已过 3 小时"));
    }

    #[test]
    fn format_deadlines_hint_handles_mixed_items() {
        // Mix of distant + approaching + overdue. Distant skipped, others rendered.
        let now = dt(2026, 5, 10, 12, 0);
        let items = vec![
            (dt(2026, 5, 15, 14, 0), "future task".to_string()), // Distant
            (dt(2026, 5, 10, 14, 0), "soon thing".to_string()),  // Approaching
            (dt(2026, 5, 10, 11, 0), "missed thing".to_string()), // Overdue
        ];
        let out = format_butler_deadlines_hint(&items, now);
        assert!(!out.contains("future task")); // distant filtered
        assert!(out.contains("soon thing"));
        assert!(out.contains("missed thing"));
    }

    // -- Iter R78: count_urgent_butler_deadlines tests ----------------------

    #[test]
    fn urgent_count_zero_for_distant_and_approaching_only() {
        // Approaching (1-6h) and Distant (>6h) don't count toward "urgent".
        let now = dt(2026, 5, 10, 8, 0);
        let items = vec![
            (dt(2026, 5, 10, 12, 0), "approaching".to_string()), // 4h away
            (dt(2026, 5, 12, 12, 0), "distant".to_string()),     // 2 days away
        ];
        assert_eq!(count_urgent_butler_deadlines(&items, now), 0);
    }

    #[test]
    fn urgent_count_includes_imminent_and_overdue() {
        let now = dt(2026, 5, 10, 12, 0);
        let items = vec![
            (dt(2026, 5, 10, 12, 30), "imminent".to_string()), // 30 min away
            (dt(2026, 5, 10, 11, 0), "overdue".to_string()),   // 1h ago
            (dt(2026, 5, 10, 14, 0), "approaching".to_string()), // 2h away — no
        ];
        assert_eq!(count_urgent_butler_deadlines(&items, now), 2);
    }

    #[test]
    fn urgent_count_empty_input_zero() {
        let now = dt(2026, 5, 10, 12, 0);
        assert_eq!(count_urgent_butler_deadlines(&[], now), 0);
    }
}
