//! 周报合成：每周日（默认 20:00 后）由 consolidate 后台 loop 触发一次，
//! 把本周的 **管家事件 / 主动开口 / 心情趋势 / 陪伴天数** 汇总成 markdown，
//! 写入 `ai_insights/weekly_summary_YYYY-Www`。
//!
//! 这条流水线**确定性**：不依赖 LLM，只靠日志解析 + 模板拼装。让"每周
//! 必有一份周报"成为可信契约 —— API key 失效 / consolidate 被禁用 都不
//! 影响周报落地。
//!
//! 本模块只装**纯函数**：
//! - `should_trigger_weekly_summary` 门控
//! - `aggregate_*` 三个日志聚合器（输入是 .log 原文 + 日期范围）
//! - `format_weekly_summary_detail` / `_description` 输出模板
//!
//! IO（读 .log、写 ai_insights、查进程内幂等缓存）由 `consolidate.rs` 在
//! 外层调用 `maybe_run_weekly_summary` 时处理。这条边界与 daily_review /
//! morning_briefing / task_heartbeat 一致 —— 让所有可单测的逻辑都集中
//! 在一处。

use chrono::{Datelike, IsoWeek, NaiveDate, NaiveDateTime, Weekday};
use std::collections::HashMap;

/// 默认 closing hour：周日 20:00 算"本周已结束"。早一点（如 18:00）容易
/// 把还没真正过完的下午误判为"本周已结束"；晚一点（如 23:00）容易因为
/// loop 唤醒间隔（默认 6h）错过窗口。20:00 在两者之间。
pub const DEFAULT_CLOSING_HOUR: u8 = 20;

/// 进程内幂等缓存：上一次合成的 ISO 周。`None` 表示进程刚启动还没合成过
/// 本会话。跨重启的幂等性靠 caller 在写入前 `read_ai_insights_item` 校验
/// 标题是否已存在。本静态只做会话内快路径。
pub static LAST_WEEKLY_SUMMARY_WEEK: std::sync::Mutex<Option<IsoWeek>> =
    std::sync::Mutex::new(None);

/// 一周的统计数据，由 aggregator 拼出来后交给 formatter 渲染 markdown。
/// 字段全是确定性数值 / 字符串列表 —— 不依赖任何 IO，便于单测格式化路径。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeeklyStats {
    pub week: IsoWeek,
    pub week_start: NaiveDate, // 周一
    pub week_end: NaiveDate,   // 周日
    pub speech_count: u64,
    pub butler_create: u32,
    pub butler_update: u32,
    pub butler_delete: u32,
    /// 本周里 description 含 `[done]` 或 `[cancelled` 的 update / delete
    /// 事件标题（去重，按时间正序）。这个名字 `completed_titles` 略不准
    /// （包含取消），但语义上都属于"已结束"，统一展示比拆两段更紧凑。
    pub completed_titles: Vec<String>,
    /// motion → count，按 count 降序，长度 ≤ 3。`-`（无 motion）也算入计
    /// 数 —— 用户没标 motion 不代表那一刻没情绪。
    pub mood_top: Vec<(String, u64)>,
    pub companionship_days: u64,
    /// 任务-记忆联动：本周已完成 / 已取消的任务的 (title, result)。result
    /// 是 description 里 `[result: ...]` 标记的内容，没有则为 None。和
    /// `completed_titles` 互补 —— 这条带产物详情，用于周报"完成清单"段。
    pub completed_with_results: Vec<(String, Option<String>)>,
    /// 本周描述里出现的 `#tag` 频次 top 5（按 count 降序，count 同则字典序）。
    /// 跨任务聚合 —— 一条任务多次更新出现同 tag 计数+1，让"用户本周往哪个
    /// 主题投入最多"自然显形。
    pub tag_top: Vec<(String, u64)>,
}

/// 门控。返回 `Some(week)` 表示该 week 现在应当被合成；`None` 表示跳过。
///
/// 判定语义：
/// 1. 找出"最近一个已结束的 ISO 周"。"已结束"= `now >= 该周周日 closing_hour:00`。
///    - 今天是周日 ∧ `now >= today closing_hour:00` → 该 week = `now.iso_week()`
///    - 否则 → 上一个 ISO 周（取 last_sunday 的 iso_week）
/// 2. 若 `last_summary_week == target` → 已合成，跳过
/// 3. 否则返回 `Some(target)`
///
/// `closing_hour == 0` 视为禁用，恒返 `None`。`closing_hour > 23` 同理 —
/// 防御越界配置不 panic。
pub fn should_trigger_weekly_summary(
    now: NaiveDateTime,
    last_summary_week: Option<IsoWeek>,
    closing_hour: u8,
) -> Option<IsoWeek> {
    if closing_hour == 0 || closing_hour > 23 {
        return None;
    }
    let today = now.date();
    let close_today = today.and_hms_opt(closing_hour as u32, 0, 0)?;
    let target_sunday = if today.weekday() == Weekday::Sun && now >= close_today {
        // 今天是周日 + 已过 closing 时刻 → 收今天这周
        today
    } else {
        // 否则取最近过去的周日；today 本身可能就是周日但还没到 closing，
        // 此时也回退到上周日。
        last_sunday_before(today, closing_hour, now)?
    };
    let target_week = target_sunday.iso_week();
    if last_summary_week == Some(target_week) {
        return None;
    }
    Some(target_week)
}

/// 找 `today` 之前最近一个"已结束"的周日。如果今天本身就是周日但还没
/// 到 `closing_hour`，回退到上周日。
fn last_sunday_before(today: NaiveDate, _closing_hour: u8, _now: NaiveDateTime) -> Option<NaiveDate> {
    let weekday = today.weekday();
    let days_since_sunday: i64 = match weekday {
        Weekday::Sun => 7, // 今天是周日但调用方判定"还没到 closing" → 取上一周日
        Weekday::Mon => 1,
        Weekday::Tue => 2,
        Weekday::Wed => 3,
        Weekday::Thu => 4,
        Weekday::Fri => 5,
        Weekday::Sat => 6,
    };
    today.checked_sub_signed(chrono::Duration::days(days_since_sunday))
}

/// 给定一个 ISO 周返回 `(monday, sunday)`。ISO 8601: 周从 Mon 开始。
pub fn iso_week_bounds(week: IsoWeek) -> (NaiveDate, NaiveDate) {
    let mon = NaiveDate::from_isoywd_opt(week.year(), week.week(), Weekday::Mon)
        .expect("valid IsoWeek -> Mon");
    let sun = NaiveDate::from_isoywd_opt(week.year(), week.week(), Weekday::Sun)
        .expect("valid IsoWeek -> Sun");
    (mon, sun)
}

/// 从一行 log（`<RFC3339 ts> <body>`）抽日期。失败返回 None。aggregator
/// 用此判断该行是否落在 [start, end] 区间。
pub fn parse_log_line_date(line: &str) -> Option<NaiveDate> {
    let ts_end = line.find(' ')?;
    let ts = &line[..ts_end];
    chrono::DateTime::parse_from_rfc3339(ts)
        .ok()
        .map(|dt| dt.with_timezone(&chrono::Local).date_naive())
}

/// 聚合 `speech_history.log` 的一周开口数。每行一个 speech，按时间戳过滤。
pub fn aggregate_speech_count(content: &str, week_start: NaiveDate, week_end: NaiveDate) -> u64 {
    content
        .lines()
        .filter(|l| !l.is_empty())
        .filter_map(parse_log_line_date)
        .filter(|d| *d >= week_start && *d <= week_end)
        .count() as u64
}

/// 一次性聚合 `butler_history.log`：动作计数 + 已完成 / 取消的标题 +
/// 完成时的产物（来自 description 的 `[result: ...]` 标记）+ 本周 tag 频次。
/// butler_history 行格式：`<ts> <action> <title> :: <desc>`。
pub fn aggregate_butler_events(
    content: &str,
    week_start: NaiveDate,
    week_end: NaiveDate,
) -> ButlerStats {
    let mut create = 0u32;
    let mut update = 0u32;
    let mut delete = 0u32;
    let mut completed: Vec<String> = Vec::new();
    let mut completed_with_results: Vec<(String, Option<String>)> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut tag_counts: std::collections::HashMap<String, u64> =
        std::collections::HashMap::new();
    for line in content.lines().filter(|l| !l.is_empty()) {
        let Some(date) = parse_log_line_date(line) else {
            continue;
        };
        if date < week_start || date > week_end {
            continue;
        }
        // 跳过 timestamp，剩下 "<action> <title> :: <desc>"
        let after_ts = match line.split_once(' ') {
            Some((_, rest)) => rest,
            None => continue,
        };
        let (action_title, desc) = after_ts.split_once(" :: ").unwrap_or((after_ts, ""));
        let Some((action, title)) = action_title.split_once(' ') else {
            continue;
        };
        let title = title.trim();
        if title.is_empty() {
            continue;
        }
        match action {
            "create" => create += 1,
            "update" => update += 1,
            "delete" => delete += 1,
            _ => continue,
        }
        // tag 聚合：每条事件的 description 都贡献一次 — 一条任务多次更新
        // 出现同 tag 会计数+1，让"用户本周往哪个主题投入最多"自然显形。
        for tag in crate::task_queue::parse_task_tags(desc) {
            *tag_counts.entry(tag).or_insert(0) += 1;
        }
        // 收"完成 / 取消"的标题：description 里出现 [done] 或 [cancelled
        // 时算结束态；delete 直接当作结束（用户 / LLM 显式删除等同结束）。
        let finished = action == "delete"
            || desc.contains("[done]")
            || desc.contains("[done ")
            || desc.contains("[cancelled");
        if finished && seen.insert(title.to_string()) {
            completed.push(title.to_string());
            // 取该完成事件的 description 里的 result 文本（delete 事件的
            // description 通常没有 result，自然是 None）
            let result = crate::task_queue::parse_task_result(desc);
            completed_with_results.push((title.to_string(), result));
        }
    }
    let mut tag_top: Vec<(String, u64)> = tag_counts.into_iter().collect();
    tag_top.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    tag_top.truncate(5);
    ButlerStats {
        create,
        update,
        delete,
        completed_titles: completed,
        completed_with_results,
        tag_top,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ButlerStats {
    pub create: u32,
    pub update: u32,
    pub delete: u32,
    pub completed_titles: Vec<String>,
    /// 已完成 / 取消的任务及其产物（[result: ...] 标记的内容）。与
    /// completed_titles 长度一致，配对顺序相同。
    pub completed_with_results: Vec<(String, Option<String>)>,
    /// 本周 tag 频次 top 5。
    pub tag_top: Vec<(String, u64)>,
}

/// 聚合 `mood_history.log` 一周的 motion 频次。返回 top n（默认 3），
/// 按 count 降序，count 同则按 motion 字典序。`-`（无 motion）也算入。
pub fn aggregate_mood_top(
    content: &str,
    week_start: NaiveDate,
    week_end: NaiveDate,
    top_n: usize,
) -> Vec<(String, u64)> {
    let mut counts: HashMap<String, u64> = HashMap::new();
    for line in content.lines().filter(|l| !l.is_empty()) {
        let Some(date) = parse_log_line_date(line) else {
            continue;
        };
        if date < week_start || date > week_end {
            continue;
        }
        if let Some((motion, _)) = crate::mood_history::parse_motion_text(line) {
            *counts.entry(motion.to_string()).or_insert(0) += 1;
        }
    }
    let mut sorted: Vec<(String, u64)> = counts.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    sorted.truncate(top_n);
    sorted
}

/// 渲染周报的 markdown 正文。空段（无任务 / 无开口 / 无心情）显示
/// 「（本周无记录）」而非省略 — 让用户能区分"功能没跑"vs"真的安静"。
pub fn format_weekly_summary_detail(stats: &WeeklyStats) -> String {
    let mut out = String::new();
    let week = stats.week;
    let (mon, sun) = (stats.week_start, stats.week_end);
    out.push_str(&format!(
        "# 周报 — {}-W{:02} ({} — {})\n\n",
        week.year(),
        week.week(),
        mon.format("%-m月%-d日"),
        sun.format("%-m月%-d日"),
    ));

    out.push_str("## 任务\n");
    let total = stats.butler_create + stats.butler_update + stats.butler_delete;
    if total == 0 {
        out.push_str("（本周无记录）\n\n");
    } else {
        out.push_str(&format!(
            "本周管家事件 {} 条（创建 {} / 更新 {} / 删除 {}）。\n",
            total, stats.butler_create, stats.butler_update, stats.butler_delete
        ));
        if stats.completed_with_results.is_empty() {
            out.push_str("\n");
        } else {
            out.push_str("完成或取消：\n");
            for (title, result) in &stats.completed_with_results {
                match result.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                    Some(r) => out.push_str(&format!("- {} — {}\n", title, r)),
                    None => out.push_str(&format!("- {}\n", title)),
                }
            }
            out.push_str("\n");
        }
        if !stats.tag_top.is_empty() {
            let parts: Vec<String> = stats
                .tag_top
                .iter()
                .map(|(t, c)| format!("#{} × {}", t, c))
                .collect();
            out.push_str(&format!("主题 tag：{}\n\n", parts.join("、")));
        }
    }

    out.push_str("## 对话\n");
    if stats.speech_count == 0 {
        out.push_str("（本周无记录）\n\n");
    } else {
        out.push_str(&format!("本周主动开口 {} 次。\n\n", stats.speech_count));
    }

    out.push_str("## 情绪\n");
    if stats.mood_top.is_empty() {
        out.push_str("（本周无记录）\n\n");
    } else {
        let parts: Vec<String> = stats
            .mood_top
            .iter()
            .map(|(m, c)| format!("{} × {}", m, c))
            .collect();
        out.push_str(&format!("本周心情主要是 {}。\n\n", parts.join("、")));
    }

    out.push_str("## 陪伴\n");
    out.push_str(&format!("累计陪伴 {} 天。\n", stats.companionship_days));

    out
}

/// 一行索引描述。和 daily_review 的 `[review] ...` 同形：保留 `[weekly]`
/// 机器标签便于未来 consolidate 整理时识别为周报条目。
pub fn format_weekly_summary_description(stats: &WeeklyStats) -> String {
    let total = stats.butler_create + stats.butler_update + stats.butler_delete;
    format!(
        "[weekly] 主动开口 {} 次，管家事件 {} 条，陪伴 {} 天",
        stats.speech_count, total, stats.companionship_days
    )
}

/// 把 ISO 周渲染成标题键 `weekly_summary_YYYY-Www`，与 daily_review 的
/// `daily_review_YYYY-MM-DD` 保持同形（含 type 前缀 + 唯一身份）。
pub fn weekly_summary_title(week: IsoWeek) -> String {
    format!("weekly_summary_{}-W{:02}", week.year(), week.week())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn dt(y: i32, m: u32, d: u32, hh: u32, mm: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(hh, mm, 0)
            .unwrap()
    }

    fn iso(y: i32, m: u32, d: u32, hh: u32, mm: u32) -> String {
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:00+08:00",
            y, m, d, hh, mm
        )
    }

    // --------- should_trigger ---------

    #[test]
    fn skips_when_closing_hour_zero() {
        // 0 = 用户禁用周报
        let now = dt(2026, 5, 3, 22, 0); // 周日深夜
        assert_eq!(should_trigger_weekly_summary(now, None, 0), None);
    }

    #[test]
    fn skips_when_closing_hour_invalid() {
        let now = dt(2026, 5, 3, 22, 0);
        assert_eq!(should_trigger_weekly_summary(now, None, 25), None);
    }

    #[test]
    fn fires_on_sunday_at_closing_hour() {
        // 2026-05-03 是周日（W18）。20:00 整点应当触发本周。
        let now = dt(2026, 5, 3, 20, 0);
        let res = should_trigger_weekly_summary(now, None, 20);
        let week = NaiveDate::from_ymd_opt(2026, 5, 3).unwrap().iso_week();
        assert_eq!(res, Some(week));
    }

    #[test]
    fn skips_on_sunday_before_closing_hour() {
        // 周日下午 19:00：上一周（W17）应当已经被合成了，所以跳过。
        let now = dt(2026, 5, 3, 19, 0);
        let last_week = NaiveDate::from_ymd_opt(2026, 4, 26).unwrap().iso_week();
        assert_eq!(
            should_trigger_weekly_summary(now, Some(last_week), 20),
            None
        );
    }

    #[test]
    fn fires_on_sunday_before_closing_when_last_week_missed() {
        // 周日下午，但上周（4 月 26 日那周）没合成过 → 仍应补发上周
        let now = dt(2026, 5, 3, 19, 0);
        let res = should_trigger_weekly_summary(now, None, 20);
        let last_week = NaiveDate::from_ymd_opt(2026, 4, 26).unwrap().iso_week();
        assert_eq!(res, Some(last_week));
    }

    #[test]
    fn fires_monday_morning_for_just_ended_week() {
        // 周一早上 09:00：上一周（含上一周日 5 月 3 日）应当合成
        let now = dt(2026, 5, 4, 9, 0);
        let res = should_trigger_weekly_summary(now, None, 20);
        let target = NaiveDate::from_ymd_opt(2026, 5, 3).unwrap().iso_week();
        assert_eq!(res, Some(target));
    }

    #[test]
    fn skips_when_target_already_summarized() {
        let now = dt(2026, 5, 4, 9, 0);
        let target = NaiveDate::from_ymd_opt(2026, 5, 3).unwrap().iso_week();
        assert_eq!(
            should_trigger_weekly_summary(now, Some(target), 20),
            None
        );
    }

    #[test]
    fn fires_thursday_when_no_prior_summary() {
        // 周四：仍应补发上周（catch-up 路径，进程长时间没运行）
        let now = dt(2026, 5, 7, 14, 0);
        let res = should_trigger_weekly_summary(now, None, 20);
        let last_week = NaiveDate::from_ymd_opt(2026, 5, 3).unwrap().iso_week();
        assert_eq!(res, Some(last_week));
    }

    // --------- iso_week_bounds ---------

    #[test]
    fn iso_week_bounds_known_week() {
        // 2026-W18：周一是 2026-04-27，周日是 2026-05-03
        let week = NaiveDate::from_ymd_opt(2026, 5, 3).unwrap().iso_week();
        let (mon, sun) = iso_week_bounds(week);
        assert_eq!(mon, NaiveDate::from_ymd_opt(2026, 4, 27).unwrap());
        assert_eq!(sun, NaiveDate::from_ymd_opt(2026, 5, 3).unwrap());
    }

    // --------- aggregate_speech_count ---------

    #[test]
    fn speech_count_filters_by_date_range() {
        let mon = NaiveDate::from_ymd_opt(2026, 4, 27).unwrap();
        let sun = NaiveDate::from_ymd_opt(2026, 5, 3).unwrap();
        // 5 行：3 行在区间内，1 行在 4-26（前一周日，区间外），1 行在 5-04（区间外）
        let log = format!(
            "{} 早上好\n{} 午饭吃了吗\n{} 该睡了\n{} 上周日的话\n{} 下周一的话\n",
            iso(2026, 4, 27, 9, 0),
            iso(2026, 4, 30, 12, 0),
            iso(2026, 5, 3, 22, 0),
            iso(2026, 4, 26, 23, 59),
            iso(2026, 5, 4, 0, 1),
        );
        assert_eq!(aggregate_speech_count(&log, mon, sun), 3);
    }

    #[test]
    fn speech_count_handles_empty_and_malformed() {
        let mon = NaiveDate::from_ymd_opt(2026, 4, 27).unwrap();
        let sun = NaiveDate::from_ymd_opt(2026, 5, 3).unwrap();
        // 空 + 没有时间戳的脏行 + 一条合法的
        let log = format!(
            "\n   \nbad-line-no-timestamp\n{} 早上好\n",
            iso(2026, 4, 28, 9, 0),
        );
        assert_eq!(aggregate_speech_count(&log, mon, sun), 1);
    }

    // --------- aggregate_butler_events ---------

    #[test]
    fn butler_events_count_actions_and_pick_finished() {
        let mon = NaiveDate::from_ymd_opt(2026, 4, 27).unwrap();
        let sun = NaiveDate::from_ymd_opt(2026, 5, 3).unwrap();
        let log = [
            format!("{} create A :: [task pri=1] 整理", iso(2026, 4, 27, 9, 0)),
            format!(
                "{} update A :: [task pri=1] 整理 [done]",
                iso(2026, 4, 28, 10, 0)
            ),
            format!("{} create B :: [task pri=2] 跑步", iso(2026, 4, 29, 8, 0)),
            format!(
                "{} update B :: [task pri=2] 跑步 [error: 下雨]",
                iso(2026, 4, 30, 8, 0)
            ),
            format!("{} delete C :: 旧任务", iso(2026, 5, 1, 12, 0)),
            // 区间外（上周）—— 不应计数
            format!("{} create OLD :: x", iso(2026, 4, 25, 9, 0)),
        ]
        .join("\n");
        let s = aggregate_butler_events(&log, mon, sun);
        assert_eq!(s.create, 2); // A, B
        assert_eq!(s.update, 2); // A done, B error
        assert_eq!(s.delete, 1); // C
        // 完成 / 取消 / 删除：A([done]) + C(delete)；B 是 error 不算结束
        assert_eq!(s.completed_titles, vec!["A".to_string(), "C".to_string()]);
    }

    #[test]
    fn butler_events_dedup_completed_titles() {
        // 同一标题多次 done 只算一次进 completed_titles
        let mon = NaiveDate::from_ymd_opt(2026, 4, 27).unwrap();
        let sun = NaiveDate::from_ymd_opt(2026, 5, 3).unwrap();
        let log = [
            format!("{} update A :: x [done]", iso(2026, 4, 27, 9, 0)),
            format!("{} update A :: x [done]", iso(2026, 4, 28, 9, 0)),
        ]
        .join("\n");
        let s = aggregate_butler_events(&log, mon, sun);
        assert_eq!(s.completed_titles, vec!["A".to_string()]);
    }

    #[test]
    fn butler_events_picks_up_cancelled() {
        let mon = NaiveDate::from_ymd_opt(2026, 4, 27).unwrap();
        let sun = NaiveDate::from_ymd_opt(2026, 5, 3).unwrap();
        let log = format!(
            "{} update A :: x [cancelled: 不做了]",
            iso(2026, 4, 28, 9, 0)
        );
        let s = aggregate_butler_events(&log, mon, sun);
        assert_eq!(s.completed_titles, vec!["A".to_string()]);
    }

    #[test]
    fn butler_events_pairs_completed_with_result() {
        let mon = NaiveDate::from_ymd_opt(2026, 4, 27).unwrap();
        let sun = NaiveDate::from_ymd_opt(2026, 5, 3).unwrap();
        let log = [
            format!(
                "{} update A :: x [done] [result: 归档 38 个文件]",
                iso(2026, 4, 28, 9, 0)
            ),
            format!("{} update B :: y [done]", iso(2026, 4, 29, 9, 0)),
        ]
        .join("\n");
        let s = aggregate_butler_events(&log, mon, sun);
        assert_eq!(
            s.completed_with_results,
            vec![
                ("A".to_string(), Some("归档 38 个文件".to_string())),
                ("B".to_string(), None),
            ]
        );
    }

    #[test]
    fn butler_events_aggregates_tags_across_events() {
        let mon = NaiveDate::from_ymd_opt(2026, 4, 27).unwrap();
        let sun = NaiveDate::from_ymd_opt(2026, 5, 3).unwrap();
        // organize 出现 3 次（A 创建 + A 更新 + B 创建），weekly 1 次
        let log = [
            format!("{} create A :: 整理 #organize", iso(2026, 4, 27, 9, 0)),
            format!("{} update A :: 整理 #organize [done]", iso(2026, 4, 28, 9, 0)),
            format!("{} create B :: 跑步 #organize #weekly", iso(2026, 4, 29, 9, 0)),
            // 区间外不计
            format!("{} create OLD :: x #organize", iso(2026, 4, 25, 9, 0)),
        ]
        .join("\n");
        let s = aggregate_butler_events(&log, mon, sun);
        assert_eq!(
            s.tag_top,
            vec![("organize".to_string(), 3), ("weekly".to_string(), 1)]
        );
    }

    // --------- aggregate_mood_top ---------

    #[test]
    fn mood_top_orders_by_count_then_alpha() {
        let mon = NaiveDate::from_ymd_opt(2026, 4, 27).unwrap();
        let sun = NaiveDate::from_ymd_opt(2026, 5, 3).unwrap();
        // mood_history 行格式：`<ts> <motion> | <text>`
        let log = [
            format!("{} Tap | a", iso(2026, 4, 27, 9, 0)),
            format!("{} Tap | b", iso(2026, 4, 28, 9, 0)),
            format!("{} Idle | c", iso(2026, 4, 29, 9, 0)),
            format!("{} Flick | d", iso(2026, 4, 30, 9, 0)),
            format!("{} Idle | e", iso(2026, 5, 1, 9, 0)),
            // 区间外
            format!("{} Tap | x", iso(2026, 4, 26, 9, 0)),
        ]
        .join("\n");
        let top = aggregate_mood_top(&log, mon, sun, 3);
        // Tap × 2, Idle × 2 → count 同 → 字典序 Idle 在 Tap 前；Flick × 1
        assert_eq!(
            top,
            vec![
                ("Idle".to_string(), 2),
                ("Tap".to_string(), 2),
                ("Flick".to_string(), 1)
            ]
        );
    }

    #[test]
    fn mood_top_truncates_to_n() {
        let mon = NaiveDate::from_ymd_opt(2026, 4, 27).unwrap();
        let sun = NaiveDate::from_ymd_opt(2026, 5, 3).unwrap();
        let log = [
            format!("{} A | x", iso(2026, 4, 27, 9, 0)),
            format!("{} B | x", iso(2026, 4, 28, 9, 0)),
            format!("{} C | x", iso(2026, 4, 29, 9, 0)),
            format!("{} D | x", iso(2026, 4, 30, 9, 0)),
        ]
        .join("\n");
        let top = aggregate_mood_top(&log, mon, sun, 2);
        assert_eq!(top.len(), 2);
    }

    // --------- format detail / description ---------

    fn sample_stats() -> WeeklyStats {
        let week = NaiveDate::from_ymd_opt(2026, 5, 3).unwrap().iso_week();
        WeeklyStats {
            week,
            week_start: NaiveDate::from_ymd_opt(2026, 4, 27).unwrap(),
            week_end: NaiveDate::from_ymd_opt(2026, 5, 3).unwrap(),
            speech_count: 23,
            butler_create: 5,
            butler_update: 8,
            butler_delete: 1,
            completed_titles: vec!["整理 Downloads".to_string(), "跑步".to_string()],
            mood_top: vec![("Tap".to_string(), 12), ("Idle".to_string(), 8)],
            companionship_days: 42,
            completed_with_results: vec![
                (
                    "整理 Downloads".to_string(),
                    Some("归档 38 个文件到 ~/Archive/2026-04/".to_string()),
                ),
                ("跑步".to_string(), None),
            ],
            tag_top: vec![("organize".to_string(), 3), ("weekly".to_string(), 1)],
        }
    }

    #[test]
    fn detail_includes_all_sections_with_values() {
        let s = format_weekly_summary_detail(&sample_stats());
        assert!(s.contains("# 周报 — 2026-W18"));
        assert!(s.contains("4月27日"));
        assert!(s.contains("5月3日"));
        assert!(s.contains("## 任务"));
        assert!(s.contains("管家事件 14 条"));
        // 带 result 的任务行：title — result 形式
        assert!(s.contains("- 整理 Downloads — 归档 38 个文件到 ~/Archive/2026-04/"));
        // 无 result 的任务行：仅标题
        assert!(s.contains("- 跑步\n"));
        // tag 聚合段
        assert!(s.contains("主题 tag：#organize × 3、#weekly × 1"));
        assert!(s.contains("## 对话"));
        assert!(s.contains("主动开口 23 次"));
        assert!(s.contains("## 情绪"));
        assert!(s.contains("Tap × 12"));
        assert!(s.contains("Idle × 8"));
        assert!(s.contains("## 陪伴"));
        assert!(s.contains("累计陪伴 42 天"));
    }

    #[test]
    fn detail_marks_empty_sections_as_no_record() {
        let week = NaiveDate::from_ymd_opt(2026, 5, 3).unwrap().iso_week();
        let stats = WeeklyStats {
            week,
            week_start: NaiveDate::from_ymd_opt(2026, 4, 27).unwrap(),
            week_end: NaiveDate::from_ymd_opt(2026, 5, 3).unwrap(),
            speech_count: 0,
            butler_create: 0,
            butler_update: 0,
            butler_delete: 0,
            completed_titles: vec![],
            mood_top: vec![],
            companionship_days: 7,
            completed_with_results: vec![],
            tag_top: vec![],
        };
        let s = format_weekly_summary_detail(&stats);
        // 三段都应该写"本周无记录"
        let no_record_count = s.matches("（本周无记录）").count();
        assert_eq!(no_record_count, 3);
        // 陪伴段始终有数字
        assert!(s.contains("累计陪伴 7 天"));
    }

    #[test]
    fn description_one_liner_format() {
        let d = format_weekly_summary_description(&sample_stats());
        assert_eq!(d, "[weekly] 主动开口 23 次，管家事件 14 条，陪伴 42 天");
    }

    // --------- title format ---------

    #[test]
    fn title_uses_iso_week_with_zero_padded_week() {
        let w = NaiveDate::from_ymd_opt(2026, 1, 5).unwrap().iso_week(); // W02
        assert_eq!(weekly_summary_title(w), "weekly_summary_2026-W02");
        let w = NaiveDate::from_ymd_opt(2026, 5, 3).unwrap().iso_week(); // W18
        assert_eq!(weekly_summary_title(w), "weekly_summary_2026-W18");
    }
}
