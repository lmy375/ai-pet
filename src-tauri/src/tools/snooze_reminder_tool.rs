//! `snooze_reminder` LLM tool（GOAL 030）：015 cancel / 022 ambiguity confirm
//! 的对偶——「再等 10 分」「挪到明天 9 点」「晚 30 分」直接走一条 tool 调
//! 用而不需要 user 先 cancel 再重建。
//!
//! 落库语义：原 todo memory item 的 `[remind: ...]` 前缀替换为 canonical 新
//! 时间，topic 保留；同一 entry 物理 in-place 更新，audit 走 butler_history
//! `snooze` / `reschedule` event。无新增 panel / TG 命令——纯 LLM 工具层。
//!
//! 短期 snooze 上限：同一 reminder 24h 内最多 3 次。超过则 tool 返结构化
//! 拒绝，让 LLM 反问 user「这条要不要直接改日子或删了？」。判定语义来自
//! butler_history（action="snooze" + title 精确匹配 + 时间窗），不在
//! description 里维护计数 marker——避免 marker 累积污染 prompt。
//!
//! 022 时间表达 ambiguity：由既有 `inject_time_ambiguity_layer` 在 prompt
//! 层把关，本 tool 只接受精确 `YYYY-MM-DD HH:MM` 或 `HH:MM`；模糊词应在
//! LLM 调用本 tool *之前* 已被反问澄清，tool 调用阶段不再做歧义处理。

use crate::tools::context::ToolContext;
use crate::tools::tool::Tool;

pub struct SnoozeReminderTool;

impl Tool for SnoozeReminderTool {
    fn name(&self) -> &str {
        "snooze_reminder"
    }

    fn definition(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "snooze_reminder",
                "description": "Push back / move a reminder when the owner says things like \"再等 10 分钟\" / \"晚 30 分\" / \"挪到明天 9 点\" / \"改到下午 3 点\" / \"推迟一会儿\". Same entry stays in place — just the `[remind: …]` time updates; the topic, title, and any chain links are preserved. Audited via butler_history.\n\n**When to call**:\n- The owner clearly points at a specific reminder (by title or by recent context like \"刚才那个吃药提醒\") AND gives a concrete new time.\n- The new time is either exact (`HH:MM` or `YYYY-MM-DD HH:MM`) or you have already resolved an ambiguous expression via a clarifying turn.\n- Do NOT call when the target reminder is ambiguous (multiple candidates) — list up to 3 recent matching titles and ask first, same pattern as `cancel_task`.\n- Do NOT call for `[recur-daily: HH:MM]` items (you can't \"snooze\" a recurring schedule sensibly) — refuse and suggest cancel + new create.\n\n**Short vs long**:\n- If `new_ts` is HH:MM only (same-day form), this counts as a *short snooze*. Same reminder is capped at 3 short snoozes per 24 hours — past that the tool refuses and asks you to reframe (\"要不要直接挪到明天 / 删了？\").\n- If `new_ts` carries a date (`YYYY-MM-DD HH:MM`), this is a *reschedule* and isn't subject to the 3×/24h cap.\n\nWhen successful, your reply should briefly confirm (\"好，挪到 <new_ts>\") without re-listing details.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "title": {
                            "type": "string",
                            "description": "Exact title of the todo memory item (the reminder). Find via memory_list(todo) if unsure."
                        },
                        "new_ts": {
                            "type": "string",
                            "description": "New target time. Use `HH:MM` for same-day push-back; use `YYYY-MM-DD HH:MM` for any cross-day move. 24-hour clock. Do NOT pass fuzzy expressions like \"傍晚\" — resolve those in a prior clarifying turn first."
                        },
                        "reason": {
                            "type": "string",
                            "description": "Optional one-line reason from the owner's wording (\"还在忙\" / \"提前到家\" / etc). Captured in butler_history snippet; defaults to empty when the owner didn't say."
                        }
                    },
                    "required": ["title", "new_ts"]
                }
            }
        })
    }

    fn execute<'a>(
        &'a self,
        arguments: &'a str,
        ctx: &'a ToolContext,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = String> + Send + 'a>> {
        Box::pin(execute_impl(arguments, ctx))
    }
}

const SHORT_SNOOZE_CAP_PER_24H: usize = 3;

async fn execute_impl(arguments: &str, ctx: &ToolContext) -> String {
    let args: serde_json::Value = serde_json::from_str(arguments).unwrap_or_default();
    let title = args["title"].as_str().unwrap_or("").trim().to_string();
    let new_ts = args["new_ts"].as_str().unwrap_or("").trim().to_string();
    let reason = args["reason"].as_str().unwrap_or("").trim().to_string();

    if title.is_empty() {
        return err("`title` 不能为空");
    }
    if new_ts.is_empty() {
        return err("`new_ts` 不能为空");
    }

    // 找原 item
    let index = match crate::commands::memory::memory_list(Some("todo".to_string())) {
        Ok(i) => i,
        Err(e) => return err(&format!("memory_list 失败：{}", e)),
    };
    let cat = match index.categories.get("todo") {
        Some(c) => c,
        None => return err("category todo 不存在"),
    };
    let item = match cat.items.iter().find(|i| i.title == title) {
        Some(i) => i.clone(),
        None => return err(&format!("没找到 todo.「{}」", title)),
    };

    // 已 cancelled / recur-daily 拒绝
    if item.description.contains("[cancelled:") {
        return err(&format!("「{}」已经是 cancelled 状态，无法 snooze", title));
    }
    let trimmed_desc = item.description.trim_start();
    if trimmed_desc.starts_with("[recur-daily:") {
        return serde_json::json!({
            "error": "recur-daily 周期提醒不能 snooze；如要改时间请 cancel_task + 新建一个 recur-daily entry"
        })
        .to_string();
    }
    if !trimmed_desc.starts_with("[remind:") {
        return err(&format!("「{}」description 不是 reminder 前缀", title));
    }

    // 解析 old / new target
    let old_parsed =
        match crate::proactive::parse_reminder_prefix(&item.description) {
            Some(p) => p,
            None => return err(&format!("「{}」的 [remind:] 前缀解析失败", title)),
        };
    let (old_target, topic) = old_parsed;
    let new_target = match parse_new_ts(&new_ts) {
        Some(t) => t,
        None => {
            return err(
                "`new_ts` 格式必须是 `HH:MM` 或 `YYYY-MM-DD HH:MM`（24h 制）",
            )
        }
    };

    let action = classify_action(&new_target);

    // 短期 snooze 24h 上限
    if action == "snooze" {
        let history = crate::butler_history::read_history_content().await;
        let now = chrono::Local::now().naive_local();
        let recent = count_recent_snoozes(&history, &title, now, 24);
        if recent >= SHORT_SNOOZE_CAP_PER_24H {
            return serde_json::json!({
                "status": "refused",
                "reason": "snooze_cap_24h",
                "message": format!(
                    "「{}」在过去 24 小时内已经 snooze 了 {} 次（上限 {}）。请反问 owner：这条要不要直接挪到明天 / 删了 / 改个固定时间？",
                    title, recent, SHORT_SNOOZE_CAP_PER_24H
                ),
                "title": title,
            })
            .to_string();
        }
    }

    // 重写 description
    let new_prefix = format_new_prefix(&new_target);
    let new_desc = format!("{} {}", new_prefix, topic);

    if let Err(e) = crate::commands::memory::memory_edit(
        "update".to_string(),
        "todo".to_string(),
        title.clone(),
        Some(new_desc),
        None,
    ) {
        return err(&format!("memory_edit 失败：{}", e));
    }

    let old_fmt = crate::proactive::format_target(&old_target);
    let new_fmt = format_new_target_display(&new_target);
    let snippet = format!(
        "{} -> {} :: {}",
        old_fmt,
        new_fmt,
        if reason.is_empty() {
            "(no reason)".to_string()
        } else {
            reason.clone()
        }
    );
    crate::butler_history::record_event(action, &title, &snippet).await;

    ctx.log(&format!(
        "snooze_reminder: {} '{}' {} -> {}",
        action, title, old_fmt, new_fmt
    ));

    serde_json::json!({
        "status": "ok",
        "action": action,
        "title": title,
        "old_ts": old_fmt,
        "new_ts": new_fmt,
    })
    .to_string()
}

/// 解析 tool 入参 `new_ts`。两种 canonical 形式：
/// - `HH:MM` → TodayHour（同日 push-back / snooze）
/// - `YYYY-MM-DD HH:MM` → Absolute（跨日 reschedule）
fn parse_new_ts(s: &str) -> Option<crate::proactive::ReminderTarget> {
    let s = s.trim();
    if let Some((d, t)) = s.split_once(' ') {
        let date = chrono::NaiveDate::parse_from_str(d.trim(), "%Y-%m-%d").ok()?;
        let time = chrono::NaiveTime::parse_from_str(t.trim(), "%H:%M").ok()?;
        return Some(crate::proactive::ReminderTarget::Absolute(
            date.and_time(time),
        ));
    }
    let (hh, mm) = s.split_once(':')?;
    let hour: u8 = hh.trim().parse().ok()?;
    let minute: u8 = mm.trim().parse().ok()?;
    if hour > 23 || minute > 59 {
        return None;
    }
    Some(crate::proactive::ReminderTarget::TodayHour(
        hour, minute,
    ))
}

/// 跨日（Absolute）→ reschedule；同日（TodayHour）→ snooze。需求文档 GOAL
/// 030 明确「短期 snooze（≤ 24h）」/「跨日 reschedule」两档对应 butler_
/// history 不同 action 标签。
fn classify_action(target: &crate::proactive::ReminderTarget) -> &'static str {
    match target {
        crate::proactive::ReminderTarget::TodayHour(_, _) => "snooze",
        crate::proactive::ReminderTarget::Absolute(_) => "reschedule",
    }
}

fn format_new_prefix(target: &crate::proactive::ReminderTarget) -> String {
    match target {
        crate::proactive::ReminderTarget::TodayHour(h, m) => {
            format!("[remind: {:02}:{:02}]", h, m)
        }
        crate::proactive::ReminderTarget::Absolute(dt) => {
            format!("[remind: {}]", dt.format("%Y-%m-%d %H:%M"))
        }
    }
}

/// 展示串与 prefix 一致——audit 与 LLM 看到的是同一个时间表达。
fn format_new_target_display(target: &crate::proactive::ReminderTarget) -> String {
    crate::proactive::format_target(target)
}

/// 扫 butler_history 找过去 `window_h` 小时内、`action == "snooze"` 且 title
/// 精确匹配的事件数。reschedule 不计入（GOAL 030 上限仅约束短期 snooze）。
///
/// Pure 函数 — 输入 history 全文 + now，便于单测。
pub fn count_recent_snoozes(
    content: &str,
    target_title: &str,
    now: chrono::NaiveDateTime,
    window_h: i64,
) -> usize {
    let target = target_title.trim();
    let cutoff = now - chrono::Duration::hours(window_h);
    content
        .lines()
        .filter(|l| !l.is_empty())
        .filter_map(crate::butler_history::parse_butler_history_line)
        .filter(|(ts_str, action, title, _)| {
            if *action != "snooze" || title.trim() != target {
                return false;
            }
            // ts 格式：`2026-05-23T14:05:00+08:00` 或 `2026-05-23T14:05:00`
            // butler_history record_event 写 `%Y-%m-%dT%H:%M:%S%:z`；strip 掉
            // 时区后按 naive 比对。
            let naive = if let Some(idx) = ts_str.rfind('+').or_else(|| ts_str.rfind('-').filter(|&i| i > 10)) {
                &ts_str[..idx]
            } else {
                ts_str
            };
            chrono::NaiveDateTime::parse_from_str(naive, "%Y-%m-%dT%H:%M:%S")
                .map(|t| t >= cutoff && t <= now)
                .unwrap_or(false)
        })
        .count()
}

fn err(msg: &str) -> String {
    serde_json::json!({ "error": msg }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proactive::ReminderTarget;
    use chrono::{NaiveDate, NaiveDateTime};

    fn ndt(y: i32, m: u32, d: u32, hh: u32, mm: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(hh, mm, 0)
            .unwrap()
    }

    #[test]
    fn parse_new_ts_today_form() {
        let t = parse_new_ts("14:30").unwrap();
        assert!(matches!(t, ReminderTarget::TodayHour(14, 30)));
    }

    #[test]
    fn parse_new_ts_absolute_form() {
        let t = parse_new_ts("2026-05-24 09:00").unwrap();
        assert!(matches!(t, ReminderTarget::Absolute(_)));
    }

    #[test]
    fn parse_new_ts_rejects_invalid() {
        assert!(parse_new_ts("傍晚").is_none());
        assert!(parse_new_ts("25:00").is_none());
        assert!(parse_new_ts("2026-13-01 09:00").is_none());
    }

    #[test]
    fn classify_today_is_snooze_absolute_is_reschedule() {
        assert_eq!(
            classify_action(&ReminderTarget::TodayHour(14, 30)),
            "snooze"
        );
        assert_eq!(
            classify_action(&ReminderTarget::Absolute(ndt(2026, 5, 24, 9, 0))),
            "reschedule"
        );
    }

    #[test]
    fn format_new_prefix_canonical() {
        assert_eq!(
            format_new_prefix(&ReminderTarget::TodayHour(9, 5)),
            "[remind: 09:05]"
        );
        assert_eq!(
            format_new_prefix(&ReminderTarget::Absolute(ndt(2026, 5, 24, 9, 0))),
            "[remind: 2026-05-24 09:00]"
        );
    }

    #[test]
    fn count_recent_snoozes_filters_by_action_and_title() {
        // butler_history line: "<ts> <action> <title> :: <snippet>"
        // 与 record_event 写盘格式一致。
        let content = "\
2026-05-23T10:00:00+08:00 snooze 吃药 :: 10:00 -> 10:15 :: 还在忙
2026-05-23T10:15:00+08:00 snooze 吃药 :: 10:15 -> 10:30 :: (no reason)
2026-05-23T10:30:00+08:00 snooze 其他事 :: 10:30 -> 10:45 :: 不算
2026-05-23T11:00:00+08:00 reschedule 吃药 :: 10:30 -> 2026-05-24 09:00 :: 改到明天
2026-05-22T09:00:00+08:00 snooze 吃药 :: 9:00 -> 9:15 :: 窗外
";
        let now = ndt(2026, 5, 23, 12, 0);
        // 24h 窗：22日9点是 27h 前 → 跳；reschedule 不算；「其他事」title 不匹配。
        // 命中两条。
        assert_eq!(count_recent_snoozes(content, "吃药", now, 24), 2);
    }

    #[test]
    fn count_recent_snoozes_ignores_outside_window() {
        let content = "\
2026-05-20T09:00:00+08:00 snooze 吃药 :: a :: b
";
        let now = ndt(2026, 5, 23, 9, 0);
        assert_eq!(count_recent_snoozes(content, "吃药", now, 24), 0);
    }

    #[test]
    fn count_recent_snoozes_handles_negative_tz_offset() {
        // record_event 在负时区机器上写 `-08:00` 后缀；rfind 必须只 strip 时区
        // 部分，不能把日期里的 `-` 误判成分隔点。
        let content = "\
2026-05-23T10:00:00-08:00 snooze 吃药 :: a :: b
";
        let now = ndt(2026, 5, 23, 11, 0);
        assert_eq!(count_recent_snoozes(content, "吃药", now, 24), 1);
    }
}
