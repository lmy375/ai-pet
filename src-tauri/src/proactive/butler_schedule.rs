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
    // Task dependencies + snooze: filter out items that are either blocked by
    // an active prerequisite or still within their `[snooze: ...]` window.
    // Both signals mean "not actionable right now" — surfacing them dilutes
    // the prompt. Both sets computed against the full input.
    let pairs: Vec<(String, String)> = items
        .iter()
        .map(|(t, d, _)| (t.clone(), d.clone()))
        .collect();
    let blocked_map = crate::task_queue::unresolved_blockers(&pairs);
    let snooze_map = crate::task_queue::snoozed_until_map(&pairs, now);
    let blocked_count = blocked_map.len();
    let snoozed_count = snooze_map.len();
    let filtered_items: Vec<&(String, String, String)> = items
        .iter()
        .filter(|(t, _, _)| {
            !blocked_map.contains_key(t) && !snooze_map.contains_key(t)
        })
        .collect();
    if filtered_items.is_empty() {
        // 全部被 blocker / snooze 卡住的极端情况：仍输出一行说明，避免 LLM
        // 完全不知道还有任务存在。max_items == 0 fast-path 已在上面 return。
        let reason = match (blocked_count, snoozed_count) {
            (b, 0) => format!("全部被 [blockedBy: …] 依赖卡住（共 {} 条）", b),
            (0, s) => format!("全部处于 [snooze: …] 暂停期（共 {} 条）", s),
            (b, s) => format!(
                "全部不可用（{} 条 [blockedBy: …] 卡住、{} 条 [snooze: …] 暂停）",
                b, s
            ),
        };
        return format!(
            "用户委托给你的管家任务：{}，等先决条件解决 / 时刻到达后再出现。",
            reason
        );
    }
    // Compute pinned / due / error state once per item and stable-sort.
    // `pinned` 是 owner 显式标 `[pinned]` 的 "钉住" 信号；优先级高于 due —— owner
    // 的意图覆盖系统的"到期"信号，让 LLM 先做主人盯紧的事。
    let mut annotated: Vec<(&(String, String, String), bool, bool, bool)> = filtered_items
        .iter()
        .map(|i| {
            let pinned = crate::task_queue::parse_pinned(&i.1);
            let due = parse_butler_schedule_prefix(&i.1)
                .map(|(sched, _)| is_butler_due(&sched, now, &i.2))
                .unwrap_or(false);
            let errored = has_butler_error(&i.1);
            (*i, pinned, due, errored)
        })
        .collect();
    // Sort 顺序：pinned → due → updated_at asc。pinned 最优先体现 owner 标注权重，
    // due 仍是系统级时间窗信号，updated_at asc 防最老任务沉底（与原 "don't let
    // tasks rot" 不变量一致）。errored 不参与 primary 排序 —— 失败常常伴随 due 已
    // 经上浮，单独排还会让非 due 的 stale error 抢顶。
    annotated.sort_by(|(a, a_pin, a_due, _), (b, b_pin, b_due, _)| match (a_pin, b_pin) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => match (a_due, b_due) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.2.cmp(&b.2),
        },
    });
    let n = annotated.len().min(max_items);
    let pin_count = annotated.iter().take(n).filter(|(_, p, _, _)| *p).count();
    let due_count = annotated.iter().take(n).filter(|(_, _, d, _)| *d).count();
    let err_count = annotated.iter().take(n).filter(|(_, _, _, e)| *e).count();
    let mut lines: Vec<String> = Vec::with_capacity(n + 3);
    // Header 拆段拼接：pinned / due / error 三个独立信号各占一段，避免 7+ 分支
    // 笛卡尔积爆。无信号时输出朴素 "按最早委托排在前"。
    let mut header_parts: Vec<String> = Vec::with_capacity(3);
    if pin_count > 0 {
        header_parts.push(format!("{} 条由 owner 钉住（优先做）", pin_count));
    }
    if due_count > 0 {
        header_parts.push(format!("{} 条到期", due_count));
    }
    if err_count > 0 {
        header_parts.push(format!("{} 条上次执行失败需要复查", err_count));
    }
    let header = if header_parts.is_empty() {
        format!("用户委托给你的管家任务（共 {} 条，按最早委托排在前）：", n)
    } else {
        format!(
            "用户委托给你的管家任务（共 {} 条，其中 {}，按 钉住 → 到期 → 最早委托 排在前）：",
            n,
            header_parts.join("、")
        )
    };
    lines.push(header);
    if blocked_count > 0 || snoozed_count > 0 {
        // 透明告知有任务被 blocker / snooze 卡住 —— 让 LLM 知道队列里还有
        // "沉睡"的工作，但当前不该 pick。数字是 cap 前总过滤数，不会因
        // max_items 截断失真。
        let mut parts: Vec<String> = Vec::with_capacity(2);
        if blocked_count > 0 {
            parts.push(format!("{} 条被 [blockedBy: …] 依赖卡住", blocked_count));
        }
        if snoozed_count > 0 {
            parts.push(format!("{} 条处于 [snooze: …] 暂停期", snoozed_count));
        }
        lines.push(format!(
            "（另有 {}，先决条件解决 / 时刻到达后才会出现在本列表。）",
            parts.join("、")
        ));
    }
    for ((title, desc, _), pinned, due, errored) in annotated.iter().take(n) {
        let trimmed = desc.trim();
        let truncated: String = if trimmed.chars().count() <= max_desc_chars {
            trimmed.to_string()
        } else {
            let head: String = trimmed.chars().take(max_desc_chars).collect();
            format!("{}…", head)
        };
        // Marker order: pinned 最先（owner 意图）→ error（最紧急系统信号）→ due
        // （时间窗）。三者可以共存（"钉住的到期任务上次还失败了"）。
        let mut marker = String::new();
        if *pinned {
            marker.push_str("📌 钉住 · ");
        }
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
看到「⏰ 到期」就该这一轮优先处理它。\
看到「📌 钉住」是 owner 显式标的「关键任务」 —— 优先级在到期之上，请这一轮就开始推进（哪怕做一小步也好），不要冷落 owner 反复钉的事。\n\
**记得在你这一轮的开口里简短提一下**：「我帮你写好 today.md 了」「Downloads 整理完了」之类——\
不必描述细节、一句话即可。让用户从 bubble 里直接看到管家工作的反馈，而不是必须打开 panel 才发现你做了事。\n\
**完成时建议补一行 `[result: 你具体做了什么]`**——这条会在面板和周报里被独立展示，让主人能直接看到产物，不必翻 detail.md。例：`[result: 把 30 天前的 38 个文件归档到 ~/Archive/2026-04/]` / `[result: 找到 3 篇相关论文已写入 detail.md]` / `[result: 提醒过了]`。「信息收集」类任务写结论；「文件操作」类写「挪了多少 / 改了哪个文件」；「提醒」类写「提醒过了」。\n\
**任务可以打 #tag**（如 `#organize` `#weekly`），description 里出现的 `#xxx` 词会被周报自动按主题聚合，让用户能看到「本周往哪个主题投入最多」。\n\
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

/// Iter R81: cooldown shrink factor when an Imminent or Overdue deadline is
/// pending. A real partner doesn't keep its quiet rhythm when something with
/// a deadline is bearing down on the user — so we halve the effective cooldown
/// while urgent deadlines exist, letting the proactive loop fire ~2× more
/// often. Pure helper; caller multiplies the result into the cooldown chain
/// alongside `mode_factor` and `feedback_factor`. Returns `1.0` (no shrink)
/// when `urgent_count == 0` — the common steady-state path.
///
/// `urgent_count` comes from `count_urgent_butler_deadlines`. We only branch
/// on zero vs non-zero — the magnitude of urgency is already reflected in the
/// prompt-side hint (R77/R79). The factor is a discrete switch so the chip
/// hover stays readable ("× 0.5 (deadline 紧迫)") rather than a continuous
/// slope that's hard to reason about.
pub fn deadline_urgency_factor(urgent_count: u64) -> f64 {
    if urgent_count >= 1 {
        0.5
    } else {
        1.0
    }
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
pub fn parse_updated_at_local(s: &str) -> Option<chrono::NaiveDateTime> {
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
/// 任务归档候选判定：`butler_tasks` 条目当前已是终态（done / cancelled）
/// 且 `updated_at` 已超出 `retention_days` 时返回 true。pending / error
/// 条目永远不归档（用户还在追踪 / pet 还可能重试）。`retention_days == 0`
/// → 永远 false（关闭归档）。`updated_at` 解析失败也返回 false（保守）。
///
/// 与 `is_completed_once`（仅删除按时执行掉的 [once] 任务）互补：本判定
/// 覆盖一般用户手填或 LLM 自然完成的任务，把老条目挪去 task_archive，
/// 让活跃队列长期保持轻量。Pure，单测可覆盖每一分支。
pub fn is_archive_candidate(
    desc: &str,
    last_updated: &str,
    today: chrono::NaiveDate,
    retention_days: u32,
) -> bool {
    if retention_days == 0 {
        return false;
    }
    let (status, _) = crate::task_queue::classify_status(desc);
    if !matches!(
        status,
        crate::task_queue::TaskStatus::Done | crate::task_queue::TaskStatus::Cancelled
    ) {
        return false;
    }
    let Some(last) = parse_updated_at_local(last_updated) else {
        return false;
    };
    let last_date = last.date();
    let days_old = (today - last_date).num_days();
    days_old >= retention_days as i64
}

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
    fn format_butler_tasks_block_filters_blocked_tasks() {
        // 「先决」未完成 → 「主任务」被卡，不该出现在 prompt block 里。
        let items = vec![
            (
                "先决任务".into(),
                "[task pri=3] 先做这个".into(),
                "2026-04-01T10:00:00+08:00".into(),
            ),
            (
                "主任务".into(),
                "[blockedBy: 先决任务] 等先决完成后再做".into(),
                "2026-04-02T10:00:00+08:00".into(),
            ),
        ];
        let out = format_butler_tasks_block(&items, 6, 100, fixed_now());
        assert!(out.contains("先决任务"));
        assert!(!out.contains("主任务"), "blocked 任务不该出现");
        assert!(
            out.contains("1 条被 [blockedBy: …] 依赖卡住"),
            "header 应透明告知有任务被卡：{out}"
        );
    }

    #[test]
    fn format_butler_tasks_block_unblocks_after_dep_done() {
        // 先决任务已 [done] → 主任务解锁，应出现在 prompt block 里。
        let items = vec![
            (
                "先决任务".into(),
                "[task pri=3] 已经做完 [done]".into(),
                "2026-04-01T10:00:00+08:00".into(),
            ),
            (
                "主任务".into(),
                "[blockedBy: 先决任务] 终于可以做了".into(),
                "2026-04-02T10:00:00+08:00".into(),
            ),
        ];
        let out = format_butler_tasks_block(&items, 6, 100, fixed_now());
        assert!(out.contains("主任务"), "blocker done 后主任务解锁");
        assert!(
            !out.contains("依赖卡住"),
            "全解锁时不该出现 blocked 横幅"
        );
    }

    #[test]
    fn format_butler_tasks_block_filters_snoozed_tasks() {
        // [snooze: future] 的任务不该出现在 prompt block；header 透明告知
        // "另有 N 条处于 snooze 暂停期"。
        // fixed_now() 是 2026-05-04 12:00；snooze 至 2026-05-20 09:00 仍在
        // 未来。
        let items = vec![
            (
                "活跃任务".into(),
                "[task pri=3] 现在就做".into(),
                "2026-04-01T10:00:00+08:00".into(),
            ),
            (
                "暂停任务".into(),
                "[snooze: 2026-05-20 09:00] 等下个 sprint".into(),
                "2026-04-02T10:00:00+08:00".into(),
            ),
        ];
        let out = format_butler_tasks_block(&items, 6, 100, fixed_now());
        assert!(out.contains("活跃任务"));
        assert!(!out.contains("暂停任务"), "snoozed 任务不该出现");
        assert!(
            out.contains("1 条处于 [snooze: …] 暂停期"),
            "header 应透明告知 snooze 数：{out}"
        );
    }

    #[test]
    fn format_butler_tasks_block_past_snooze_passes_through() {
        // [snooze: 过去] → 自然失效，任务恢复出现。
        let items = vec![(
            "原暂停".into(),
            "[snooze: 2020-01-01 09:00] 早就该醒".into(),
            "2026-04-02T10:00:00+08:00".into(),
        )];
        let out = format_butler_tasks_block(&items, 6, 100, fixed_now());
        assert!(out.contains("原暂停"));
        assert!(!out.contains("暂停期"), "过点 snooze 不应触发 header 标记");
    }

    #[test]
    fn format_butler_tasks_block_blocked_and_snoozed_header() {
        // 同时有 blocked 和 snoozed：transparency line 应列两段。
        let items = vec![
            (
                "blocker".into(),
                "[task pri=3] 先决".into(),
                "2026-04-01T10:00:00+08:00".into(),
            ),
            (
                "blocked".into(),
                "[blockedBy: blocker] 等先决".into(),
                "2026-04-02T10:00:00+08:00".into(),
            ),
            (
                "snoozed".into(),
                "[snooze: 2026-05-20 09:00] 等等".into(),
                "2026-04-03T10:00:00+08:00".into(),
            ),
        ];
        let out = format_butler_tasks_block(&items, 6, 100, fixed_now());
        assert!(out.contains("blocker"));
        assert!(!out.contains("blocked："), "blocked 任务不该出现");
        assert!(!out.contains("snoozed："), "snoozed 任务不该出现");
        assert!(out.contains("1 条被 [blockedBy: …] 依赖卡住"));
        assert!(out.contains("1 条处于 [snooze: …] 暂停期"));
    }

    #[test]
    fn format_butler_tasks_block_all_blocked_returns_summary() {
        // 极端：所有任务都被某个 blocker 卡住 → 输出兜底说明而非空。
        let items = vec![
            (
                "blocker".into(),
                "[task pri=3] 先决任务还没做".into(),
                "2026-04-01T10:00:00+08:00".into(),
            ),
            (
                "a".into(),
                "[blockedBy: blocker] a".into(),
                "2026-04-02T10:00:00+08:00".into(),
            ),
            (
                "b".into(),
                "[blockedBy: blocker] b".into(),
                "2026-04-03T10:00:00+08:00".into(),
            ),
        ];
        // 把 blocker 也变成被卡的：构造循环依赖（极端 footgun，应不死锁）。
        // blocker 本身没 blocked_by 所以 active，主任务被卡 —— 输出含 blocker。
        let out = format_butler_tasks_block(&items, 6, 100, fixed_now());
        assert!(out.contains("blocker"));
        assert!(out.contains("依赖卡住"));
        // title 列以 "- {title}：" 形态出现；用全角冒号锚定避免 "- b" 误匹
        // 配到 "- blocker：" 前缀。
        assert!(!out.contains("- a："));
        assert!(!out.contains("- b："));
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
    fn format_butler_tasks_block_pinned_task_bubbles_to_top_with_marker() {
        // owner [pinned] 任务上浮到第一行，line 带 "📌 钉住" marker，header 含
        // "其中 1 条由 owner 钉住"。
        let items = vec![
            (
                "plain-old".into(),
                "do something whenever".into(),
                "2026-04-01T08:00:00+08:00".into(),
            ),
            (
                "key-task".into(),
                "[task pri=3] crucial work [pinned]".into(),
                "2026-05-02T08:00:00+08:00".into(),
            ),
        ];
        let out = format_butler_tasks_block(&items, 6, 100, fixed_now());
        assert!(out.contains("📌 钉住"), "pinned should carry marker: {out}");
        assert!(
            out.contains("1 条由 owner 钉住"),
            "header should reflect pin count: {out}"
        );
        let pinned_idx = out.find("key-task").unwrap();
        let plain_idx = out.find("plain-old").unwrap();
        assert!(pinned_idx < plain_idx, "pinned ranks above plain older");
    }

    #[test]
    fn format_butler_tasks_block_pinned_dominates_due_in_ordering() {
        // pinned 优先级高于 due —— owner 的标注覆盖系统的时间窗信号。
        // due 任务（[every: 09:00] now=12:00）和 pinned 任务都"该做"，
        // pinned 排在前。两者的 marker 都正确显示。
        let items = vec![
            (
                "morning-report".into(),
                "[every: 09:00] write today.md".into(),
                "2026-05-02T09:30:00+08:00".into(),
            ),
            (
                "pinned-task".into(),
                "[task pri=3] 主人盯的 [pinned]".into(),
                "2026-05-02T08:00:00+08:00".into(),
            ),
        ];
        let out = format_butler_tasks_block(&items, 6, 100, fixed_now());
        let pin_idx = out.find("pinned-task").unwrap();
        let due_idx = out.find("morning-report").unwrap();
        assert!(
            pin_idx < due_idx,
            "pinned should outrank due — owner intent over system signal: {out}"
        );
        // 双 marker 都正确显示（不互相吃掉）
        assert!(out.contains("📌 钉住"));
        assert!(out.contains("⏰ 到期"));
        // 两个 count 都在 header 里
        assert!(out.contains("1 条由 owner 钉住"));
        assert!(out.contains("1 条到期"));
    }

    #[test]
    fn format_butler_tasks_block_no_pinned_means_no_pin_phrase_in_header() {
        // 无 pinned 任务时 header 行 + task line 都不出现 "📌 钉住" 标记 ——
        // 避免给 LLM 假信号。footer 始终带 "看到「📌 钉住」" 教学文案，故
        // 仅校验前 N 行（header + task lines）而非整体 output。
        let items = vec![(
            "plain".into(),
            "[task pri=3] 普通任务".into(),
            "2026-04-01T08:00:00+08:00".into(),
        )];
        let out = format_butler_tasks_block(&items, 6, 100, fixed_now());
        let header = out.lines().next().unwrap();
        assert!(
            !header.contains("钉住"),
            "no pinned task → header doesn't mention 钉住: {header}"
        );
        // task line 也不能带 📌 marker
        let task_lines: Vec<&str> = out.lines().filter(|l| l.starts_with("- ")).collect();
        for line in &task_lines {
            assert!(!line.contains("📌"), "task line: {line}");
        }
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

    // -- is_archive_candidate -----------------------------------------------

    fn day(y: i32, m: u32, d: u32) -> chrono::NaiveDate {
        chrono::NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    /// 把本地日期 + 时分铸成 RFC3339 字符串，与 `now_iso()` 写盘格式一致：
    /// `YYYY-MM-DDTHH:MM:SS+HH:MM`。
    fn local_iso(y: i32, m: u32, d: u32, hour: u32, minute: u32) -> String {
        use chrono::TimeZone;
        let naive = day(y, m, d).and_hms_opt(hour, minute, 0).unwrap();
        chrono::Local
            .from_local_datetime(&naive)
            .unwrap()
            .format("%Y-%m-%dT%H:%M:%S%:z")
            .to_string()
    }

    #[test]
    fn is_archive_candidate_done_past_threshold_returns_true() {
        let updated = local_iso(2026, 4, 1, 10, 0);
        assert!(is_archive_candidate(
            "[task pri=1] 整理 [done] [result: 完成]",
            &updated,
            day(2026, 5, 10),
            30,
        ));
    }

    #[test]
    fn is_archive_candidate_cancelled_past_threshold_returns_true() {
        let updated = local_iso(2026, 4, 1, 10, 0);
        assert!(is_archive_candidate(
            "[task pri=1] 整理 [cancelled: 不需要了]",
            &updated,
            day(2026, 5, 10),
            30,
        ));
    }

    #[test]
    fn is_archive_candidate_pending_never_archived() {
        let updated = local_iso(2026, 1, 1, 10, 0);
        assert!(!is_archive_candidate(
            "[task pri=1] 整理",
            &updated,
            day(2026, 5, 10),
            30,
        ));
    }

    #[test]
    fn is_archive_candidate_error_never_archived() {
        let updated = local_iso(2026, 1, 1, 10, 0);
        assert!(!is_archive_candidate(
            "[task pri=1] 整理 [error: 文件不存在]",
            &updated,
            day(2026, 5, 10),
            30,
        ));
    }

    #[test]
    fn is_archive_candidate_within_threshold_returns_false() {
        let updated = local_iso(2026, 5, 5, 10, 0);
        assert!(!is_archive_candidate(
            "[task pri=1] 整理 [done]",
            &updated,
            day(2026, 5, 10),
            30,
        ));
    }

    #[test]
    fn is_archive_candidate_zero_retention_disables_archive() {
        let updated = local_iso(2020, 1, 1, 10, 0);
        assert!(!is_archive_candidate(
            "[task pri=1] 整理 [done]",
            &updated,
            day(2026, 5, 10),
            0,
        ));
    }

    #[test]
    fn is_archive_candidate_unparseable_updated_at_skipped() {
        assert!(!is_archive_candidate(
            "[task pri=1] 整理 [done]",
            "garbage",
            day(2026, 5, 10),
            30,
        ));
        assert!(!is_archive_candidate(
            "[task pri=1] 整理 [done]",
            "",
            day(2026, 5, 10),
            30,
        ));
    }

    #[test]
    fn is_archive_candidate_exactly_at_threshold_archives() {
        // updated_at 距 today 整 30 天 → 满足 >= 30 等号；归档。
        let updated = local_iso(2026, 4, 10, 10, 0);
        assert!(is_archive_candidate(
            "[task pri=1] 整理 [done]",
            &updated,
            day(2026, 5, 10),
            30,
        ));
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

    // -- Iter R81: deadline_urgency_factor tests ----------------------------

    #[test]
    fn deadline_factor_zero_urgent_returns_one() {
        // No urgent deadlines → no shrink. Steady-state.
        assert_eq!(deadline_urgency_factor(0), 1.0);
    }

    #[test]
    fn deadline_factor_single_urgent_halves_cooldown() {
        // One Imminent or Overdue deadline → cooldown × 0.5.
        assert_eq!(deadline_urgency_factor(1), 0.5);
    }

    #[test]
    fn deadline_factor_many_urgent_still_half() {
        // Discrete switch — count > 1 doesn't shrink further. Magnitude is
        // expressed in the prompt-side hint (R77/R79), not the gate factor.
        assert_eq!(deadline_urgency_factor(5), 0.5);
        assert_eq!(deadline_urgency_factor(100), 0.5);
    }
}
