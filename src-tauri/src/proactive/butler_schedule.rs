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

use chrono::Datelike;

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
    // [silent] marker：owner 显式标"不要在 proactive cycle 主动选择"。与
    // blockedBy / snooze 同 filter 层级（不可 actionable）。`silent` 没有时
    // 间维度 —— 只有 owner remove marker 才退出 silent 态。
    let silent_set: std::collections::HashSet<String> = items
        .iter()
        .filter(|(_, d, _)| crate::task_queue::parse_silent(d))
        .map(|(t, _, _)| t.clone())
        .collect();
    let blocked_count = blocked_map.len();
    let snoozed_count = snooze_map.len();
    let silent_count = silent_set.len();
    let filtered_items: Vec<&(String, String, String)> = items
        .iter()
        .filter(|(t, _, _)| {
            !blocked_map.contains_key(t)
                && !snooze_map.contains_key(t)
                && !silent_set.contains(t)
        })
        .collect();
    if filtered_items.is_empty() {
        // 全部被 blocker / snooze / silent 卡住的极端情况：仍输出一行说明，
        // 避免 LLM 完全不知道还有任务存在。max_items == 0 fast-path 已在
        // 上面 return。
        let mut parts: Vec<String> = Vec::with_capacity(3);
        if blocked_count > 0 {
            parts.push(format!("{} 条 [blockedBy: …] 卡住", blocked_count));
        }
        if snoozed_count > 0 {
            parts.push(format!("{} 条 [snooze: …] 暂停", snoozed_count));
        }
        if silent_count > 0 {
            parts.push(format!("{} 条 [silent] owner 标静默", silent_count));
        }
        let reason = if parts.len() == 1 {
            // 单一原因：用更口语化的旧表述
            match (blocked_count, snoozed_count, silent_count) {
                (b, 0, 0) => format!("全部被 [blockedBy: …] 依赖卡住（共 {} 条）", b),
                (0, s, 0) => format!("全部处于 [snooze: …] 暂停期（共 {} 条）", s),
                (0, 0, x) => format!("全部被 owner 标 [silent]（共 {} 条），不在主动 cycle 里出现", x),
                _ => parts.join("、"),
            }
        } else {
            format!("全部不可用（{}）", parts.join("、"))
        };
        return format!(
            "用户委托给你的管家任务：{}，等先决条件解决 / 时刻到达 / [silent] marker 移除后再出现。",
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
    if blocked_count > 0 || snoozed_count > 0 || silent_count > 0 {
        // 透明告知有任务被 blocker / snooze / silent 卡住 —— 让 LLM 知道队列
        // 里还有"沉睡"的工作，但当前不该 pick。数字是 cap 前总过滤数，不会
        // 因 max_items 截断失真。silent 是 owner 主动标"不要选"，与 blocked /
        // snooze（被动 / 时间） 维度不同，独立列出。
        let mut parts: Vec<String> = Vec::with_capacity(3);
        if blocked_count > 0 {
            parts.push(format!("{} 条被 [blockedBy: …] 依赖卡住", blocked_count));
        }
        if snoozed_count > 0 {
            parts.push(format!("{} 条处于 [snooze: …] 暂停期", snoozed_count));
        }
        if silent_count > 0 {
            parts.push(format!("{} 条被 owner 标 [silent] 不选", silent_count));
        }
        lines.push(format!(
            "（另有 {}，先决条件解决 / 时刻到达 / [silent] marker 移除后才会出现在本列表。）",
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
    /// resolves "已经执行过当天的".
    Every(u8, u8),
    /// Weekday-restricted recurring at HH:MM local. `mask` 是 7 位 bitmask，bit 0 =
    /// Monday, bit 6 = Sunday；只有 mask 命中当前 weekday 才视为"今日要 fire"。覆盖
    /// "工作日 standup" (mask = 0b0011111) / "周末整理" (mask = 0b1100000) 等场景。
    EveryOnWeekdays(u8, u8, u8),
    /// Single-fire at the absolute moment.
    Once(chrono::NaiveDateTime),
}

/// 工作日 mask 常量（Mon-Fri = bits 0-4）。导出供 parser / 测试 / 前端镜像复用。
pub const WEEKDAY_MASK_WORKDAYS: u8 = 0b0011111;
/// 周末 mask 常量（Sat-Sun = bits 5-6）。
pub const WEEKDAY_MASK_WEEKEND: u8 = 0b1100000;

/// 把单个 weekday 关键词（中/英）映射到 weekday mask（7 位中的一位）。返
/// 回 None 表示不识别。识别集合：
/// - 周一 / 星期一 / mon / monday → bit 0
/// - 周二 / 星期二 / tue / tuesday → bit 1
/// - … 周日 / 星期日 / sun / sunday → bit 6
pub fn parse_single_weekday_keyword(s: &str) -> Option<u8> {
    let lower = s.trim().to_lowercase();
    match lower.as_str() {
        "mon" | "monday" => return Some(1 << 0),
        "tue" | "tuesday" => return Some(1 << 1),
        "wed" | "wednesday" => return Some(1 << 2),
        "thu" | "thursday" => return Some(1 << 3),
        "fri" | "friday" => return Some(1 << 4),
        "sat" | "saturday" => return Some(1 << 5),
        "sun" | "sunday" => return Some(1 << 6),
        _ => {}
    }
    match s.trim() {
        "周一" | "星期一" | "礼拜一" => Some(1 << 0),
        "周二" | "星期二" | "礼拜二" => Some(1 << 1),
        "周三" | "星期三" | "礼拜三" => Some(1 << 2),
        "周四" | "星期四" | "礼拜四" => Some(1 << 3),
        "周五" | "星期五" | "礼拜五" => Some(1 << 4),
        "周六" | "星期六" | "礼拜六" => Some(1 << 5),
        "周日" | "周天" | "星期日" | "星期天" | "礼拜日" | "礼拜天" => Some(1 << 6),
        _ => None,
    }
}

/// 把 `[every:` 内 HH:MM 之前的可选 weekday-set 关键词解析为 mask。返回 None 表示不识别。
/// 支持的关键词：
/// - "工作日" / "周一到周五" / "weekday" / "weekdays" → 工作日 mask (Mon-Fri)
/// - "周末" / "weekend" / "weekends" → 周末 mask (Sat-Sun)
/// - 单 weekday "周一" / "monday" 等 → 该单天 mask
pub fn parse_weekday_set_keyword(s: &str) -> Option<u8> {
    let raw = s.trim();
    let lower = raw.to_lowercase();
    match lower.as_str() {
        "weekday" | "weekdays" => return Some(WEEKDAY_MASK_WORKDAYS),
        "weekend" | "weekends" => return Some(WEEKDAY_MASK_WEEKEND),
        _ => {}
    }
    match raw {
        "工作日" | "周一到周五" | "工作日子" => return Some(WEEKDAY_MASK_WORKDAYS),
        "周末" | "双休" => return Some(WEEKDAY_MASK_WEEKEND),
        _ => {}
    }
    parse_single_weekday_keyword(raw)
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
        // 尝试 weekday-set 路径：split 末 token 当 HH:MM，前面 token 当
        // weekday-set 关键词。inside 内含空白说明有 weekday-set 前缀。
        if let Some(space_idx) = inside.rfind(char::is_whitespace) {
            let (left, right) = inside.split_at(space_idx);
            let weekday_keyword = left.trim();
            let time_part = right.trim();
            if !weekday_keyword.is_empty() && !time_part.is_empty() {
                let (hh, mm) = time_part.split_once(':')?;
                let hour: u8 = hh.trim().parse().ok()?;
                let minute: u8 = mm.trim().parse().ok()?;
                if hour > 23 || minute > 59 {
                    return None;
                }
                let mask = parse_weekday_set_keyword(weekday_keyword)?;
                return Some((
                    ButlerSchedule::EveryOnWeekdays(mask, hour, minute),
                    topic,
                ));
            }
        }
        // 纯 HH:MM 路径（既有行为）
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
        ButlerSchedule::EveryOnWeekdays(mask, h, m) => {
            // 最近一次 fire = "now 之前最近 weekday ∈ mask 的那天 + HH:MM"。
            // 算法：从今天起向回看 ≤ 7 天，找首个 mask 命中的日期；今日命中且
            // 时刻未到时再往前找一天。mask == 0 → 永远不 fire（返 false 而非
            // 死循环）。
            if *mask == 0 {
                return false;
            }
            let now_weekday_bit = weekday_bit_from_chrono(now.date().weekday());
            // step1：找候选 fire date —— 从今日开始向回扫 7 天
            let target_today = match now.date().and_hms_opt(*h as u32, *m as u32, 0) {
                Some(t) => t,
                None => return false,
            };
            let candidate_today_match =
                (mask & now_weekday_bit) != 0 && now >= target_today;
            let mut fire_date_offset_back: i64 = if candidate_today_match {
                0
            } else {
                1 // 从昨日向回找
            };
            let mut fire_at: Option<chrono::NaiveDateTime> = None;
            while fire_date_offset_back <= 7 {
                let cand_date = now.date() - chrono::Duration::days(fire_date_offset_back);
                let cand_bit = weekday_bit_from_chrono(cand_date.weekday());
                if (mask & cand_bit) != 0 {
                    if let Some(t) = cand_date.and_hms_opt(*h as u32, *m as u32, 0) {
                        fire_at = Some(t);
                        break;
                    }
                }
                fire_date_offset_back += 1;
            }
            let Some(most_recent_fire) = fire_at else {
                return false; // 8 天内无 mask 命中（理论上 mask != 0 时不会发生）
            };
            match last {
                Some(u) => u < most_recent_fire,
                None => true,
            }
        }
    }
}

/// `chrono::Weekday` → 7-bit mask 位号（bit 0 = Mon）。pub for `is_butler_due`
/// + parser tests + 前端 mirror 文档对照。
pub fn weekday_bit_from_chrono(wd: chrono::Weekday) -> u8 {
    1 << wd.num_days_from_monday()
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
        // recurring schedule（Every / EveryOnWeekdays）按定义再次 fire，永远
        // 不算"完成可清理"。is_completed_once 仅给 [once:] 任务用。
        ButlerSchedule::Every(_, _) | ButlerSchedule::EveryOnWeekdays(..) => {
            return false;
        }
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
#[path = "butler_schedule_tests.rs"]
mod tests;
