//! Morning briefing — 每日固定时间触发一次"主动发言"，由 LLM 组合
//! 天气 / 日程 / 用户提醒 / 昨日 daily_review 摘要，生成晨间播报。
//!
//! 与 daily_review 同构：本模块只暴露 **纯** 函数（门控 + intent 文本
//! 模板），所有 IO（chat 调用、写入 speech_history、读 settings）由
//! `proactive.rs` 中的 async wrapper 在更上层处理。这样门控与文本
//! 拼装均可单测，不依赖 tokio / fs / time-of-day。
//!
//! M1 阶段产物（见 docs/index-20260504-1150-morning-briefing.md）。

use chrono::{NaiveDate, NaiveDateTime};

/// 默认触发小时。为什么是 8 而非 9：上班族普遍 8:30 出门前后，简报最
/// 有用；用户嫌早可在设置里改成 9 或更晚。
pub const MORNING_BRIEFING_DEFAULT_HOUR: u8 = 8;
pub const MORNING_BRIEFING_DEFAULT_MINUTE: u8 = 30;

/// 错过整点后的"宽限窗口"。proactive 循环可能因为 mute / 专注模式 / 进
/// 程刚启动等原因没在 8:30 整点 tick — 8:30 ~ 9:30 内任意一次合法 tick
/// 都补发一次，超过则今天放弃，避免下午突然弹出"早安"。
pub const MORNING_BRIEFING_DEFAULT_GRACE_MINUTES: u32 = 60;

/// 进程内缓存：今天是否已经发过早安简报。`None` 表示进程启动以来还没发
/// 过；async wrapper 在写完 speech_history 后置为 `Some(today)`。跨重
/// 启的幂等性靠在触发前再读 speech_history 校验当天是否存在 kind =
/// `morning_briefing` 的条目，本静态只是会话内快路径。
pub static LAST_MORNING_BRIEFING_DATE: std::sync::Mutex<Option<NaiveDate>> =
    std::sync::Mutex::new(None);

/// 纯门控。返回 true 当且仅当：
/// 1. 当前时刻 ≥ 目标 (hour:minute)；
/// 2. 当前时刻距离目标不超过 `grace_minutes`；
/// 3. 今天还没触发过（`last_briefing_date != Some(today)`）。
///
/// 由调用方决定 `now` 的时区（一般是 chrono::Local::now().naive_local()），
/// 所以本函数本身与时区 / 系统时钟解耦，便于单测。
pub fn should_trigger_morning_briefing(
    now: NaiveDateTime,
    target_hour: u8,
    target_minute: u8,
    grace_minutes: u32,
    last_briefing_date: Option<NaiveDate>,
) -> bool {
    let today = now.date();
    if matches!(last_briefing_date, Some(d) if d == today) {
        return false;
    }
    let target_time = match chrono::NaiveTime::from_hms_opt(
        target_hour as u32,
        target_minute as u32,
        0,
    ) {
        Some(t) => t,
        None => return false, // 无效配置（如 hour=25），主动放弃，不 panic
    };
    let target_dt = today.and_time(target_time);
    if now < target_dt {
        return false;
    }
    let elapsed = now.signed_duration_since(target_dt);
    elapsed.num_minutes() <= grace_minutes as i64
}

/// 上限：intent 中拼入的"昨日回顾摘要"最多多少字符。太长会挤掉主任务
/// 指令，让 LLM 把简报写成纯回顾。
pub const YESTERDAY_EXCERPT_CHAR_CAP: usize = 200;

/// 纯文本组装器：把 LLM 这一轮要被告知的"早安简报"指令拼好。返回值会
/// 由 wrapper 注入为一次特殊的 proactive turn 的 user message（或类似
/// 通道；具体接入方式见 M2）。
///
/// - `user_name` 为空时省略称呼行；
/// - `yesterday_excerpt` 给 None 时不附"昨日回顾"段（首次使用 / 昨天
///   没记录都属此情况）；
/// - `mood_hint` 给 None 时省略情绪行；
/// - 工具白名单不在文案里出现（由 chat pipeline 的 system prompt 控
///   制），避免"硬编码"的 brittle 协议。
pub fn format_morning_briefing_intent(
    user_name: &str,
    yesterday_excerpt: Option<&str>,
    mood_hint: Option<&str>,
    today: NaiveDate,
) -> String {
    let mut out = String::new();
    out.push_str(&format!("【早安简报 · {}】\n", today));
    if !user_name.trim().is_empty() {
        out.push_str(&format!("主人：{}\n", user_name.trim()));
    }
    if let Some(m) = mood_hint.map(str::trim).filter(|s| !s.is_empty()) {
        out.push_str(&format!("当前心情线索：{}\n", m));
    }
    if let Some(y) = yesterday_excerpt
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let truncated: String = if y.chars().count() <= YESTERDAY_EXCERPT_CHAR_CAP {
            y.to_string()
        } else {
            let mut s: String = y.chars().take(YESTERDAY_EXCERPT_CHAR_CAP).collect();
            s.push('…');
            s
        };
        out.push_str("昨日回顾摘录：\n");
        out.push_str(&truncated);
        out.push('\n');
    }
    out.push_str(
        "请你扮演宠物角色，用一段自然口语对主人说「早安」。可以调用 \
         get_weather / get_upcoming_events / memory_list 工具核实今天的天气、日程与待办，\
         并把要点融进早安里——不是机械罗列，而是像朋友顺嘴提醒。控制在 80 字内。",
    );
    out
}

/// 早安简报跨节奏控制层的复合门控。把以下三类"先于时间窗口"的拒绝信号
/// 收拢成一个纯函数：
/// - **disabled**：用户在设置面板关闭了早安；
/// - **muted**：用户按下 transient mute（"接下来 1 小时安静一会儿"）；
/// - **focus**：macOS Focus / 勿扰开启 *且* 用户勾选了"尊重 Focus"。
///
/// 与 `should_trigger_morning_briefing`（时间 + 日期幂等）是两层关系：
/// 调用方先过本函数，再过下层时间门，缺一不可。返回 `None` = 通过；返回
/// `Some(原因短语)` = 拒绝，便于 log / 决策日志显示具体原因。
///
/// 故意不在这里处理 cooldown — 早安自带"每日 1 次"语义，应**绕过**普通
/// proactive cooldown（参见 docs/20260504-1150-morning-briefing.md）。
pub fn morning_briefing_block_reason(
    enabled: bool,
    muted: bool,
    focus_active: bool,
    respect_focus: bool,
) -> Option<&'static str> {
    if !enabled {
        return Some("disabled");
    }
    if muted {
        return Some("muted");
    }
    if respect_focus && focus_active {
        return Some("focus");
    }
    None
}

/// 拼装写入 `ai_insights/morning_briefing_YYYY-MM-DD` 的索引描述。和
/// daily_review 的 `[review] ...` 同形：保留 `[briefing]` 机器标签便于
/// 未来 LLM 整理或前端筛选；正文截到 80 字符（中文按 char 计），避免
/// 把整段早安挤进索引摘要。
pub const BRIEFING_DESCRIPTION_CHAR_CAP: usize = 80;

pub fn format_morning_briefing_description(spoken: &str) -> String {
    let trimmed = spoken.trim();
    if trimmed.is_empty() {
        return "[briefing]".to_string();
    }
    let truncated: String = if trimmed.chars().count() <= BRIEFING_DESCRIPTION_CHAR_CAP {
        trimmed.to_string()
    } else {
        let mut s: String = trimmed.chars().take(BRIEFING_DESCRIPTION_CHAR_CAP).collect();
        s.push('…');
        s
    };
    format!("[briefing] {}", truncated)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dt(y: i32, m: u32, d: u32, hh: u32, mm: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(hh, mm, 0)
            .unwrap()
    }

    #[test]
    fn before_target_time_does_not_trigger() {
        let now = dt(2026, 5, 4, 8, 0);
        assert!(!should_trigger_morning_briefing(now, 8, 30, 60, None));
    }

    #[test]
    fn at_exact_target_time_triggers() {
        let now = dt(2026, 5, 4, 8, 30);
        assert!(should_trigger_morning_briefing(now, 8, 30, 60, None));
    }

    #[test]
    fn within_grace_window_triggers() {
        let now = dt(2026, 5, 4, 9, 15);
        assert!(should_trigger_morning_briefing(now, 8, 30, 60, None));
    }

    #[test]
    fn at_grace_boundary_triggers() {
        // 8:30 + 60min = 9:30，含端点
        let now = dt(2026, 5, 4, 9, 30);
        assert!(should_trigger_morning_briefing(now, 8, 30, 60, None));
    }

    #[test]
    fn past_grace_window_does_not_trigger() {
        let now = dt(2026, 5, 4, 9, 31);
        assert!(!should_trigger_morning_briefing(now, 8, 30, 60, None));
    }

    #[test]
    fn already_triggered_today_skips() {
        let now = dt(2026, 5, 4, 8, 45);
        let today = NaiveDate::from_ymd_opt(2026, 5, 4).unwrap();
        assert!(!should_trigger_morning_briefing(now, 8, 30, 60, Some(today)));
    }

    #[test]
    fn yesterday_briefing_does_not_block_today() {
        let now = dt(2026, 5, 4, 8, 45);
        let yesterday = NaiveDate::from_ymd_opt(2026, 5, 3).unwrap();
        assert!(should_trigger_morning_briefing(
            now,
            8,
            30,
            60,
            Some(yesterday)
        ));
    }

    #[test]
    fn invalid_hour_returns_false_not_panic() {
        let now = dt(2026, 5, 4, 8, 30);
        // hour=25 — 防御性输入：不应 panic，且永远不应触发
        assert!(!should_trigger_morning_briefing(now, 25, 0, 60, None));
    }

    #[test]
    fn intent_includes_date_and_user_name() {
        let today = NaiveDate::from_ymd_opt(2026, 5, 4).unwrap();
        let s = format_morning_briefing_intent("moon", None, None, today);
        assert!(s.contains("2026-05-04"));
        assert!(s.contains("主人：moon"));
    }

    #[test]
    fn intent_omits_user_name_when_blank_or_whitespace() {
        let today = NaiveDate::from_ymd_opt(2026, 5, 4).unwrap();
        let s = format_morning_briefing_intent("   ", None, None, today);
        assert!(!s.contains("主人："));
    }

    #[test]
    fn intent_omits_yesterday_block_when_none_or_empty() {
        let today = NaiveDate::from_ymd_opt(2026, 5, 4).unwrap();
        let s = format_morning_briefing_intent("moon", None, None, today);
        assert!(!s.contains("昨日回顾"));
        let s2 = format_morning_briefing_intent("moon", Some("   "), None, today);
        assert!(!s2.contains("昨日回顾"));
    }

    #[test]
    fn intent_truncates_long_yesterday_excerpt() {
        let today = NaiveDate::from_ymd_opt(2026, 5, 4).unwrap();
        let long: String = "记".repeat(YESTERDAY_EXCERPT_CHAR_CAP + 50);
        let s = format_morning_briefing_intent("moon", Some(&long), None, today);
        // 截断后应附省略号且字符数不超过上限+1
        assert!(s.contains("…"));
        let after_marker = s
            .split("昨日回顾摘录：\n")
            .nth(1)
            .expect("must contain yesterday block");
        let line = after_marker.lines().next().unwrap_or_default();
        assert!(line.chars().count() <= YESTERDAY_EXCERPT_CHAR_CAP + 1);
    }

    #[test]
    fn intent_includes_mood_when_provided() {
        let today = NaiveDate::from_ymd_opt(2026, 5, 4).unwrap();
        let s = format_morning_briefing_intent("moon", None, Some("有点犯困"), today);
        assert!(s.contains("当前心情线索：有点犯困"));
    }

    #[test]
    fn intent_mentions_required_tools() {
        // 不绑死具体协议，但要让 LLM 知道有这些工具可用
        let today = NaiveDate::from_ymd_opt(2026, 5, 4).unwrap();
        let s = format_morning_briefing_intent("moon", None, None, today);
        assert!(s.contains("get_weather"));
        assert!(s.contains("get_upcoming_events"));
        assert!(s.contains("memory_list"));
    }

    #[test]
    fn description_handles_empty_spoken() {
        assert_eq!(format_morning_briefing_description("   "), "[briefing]");
        assert_eq!(format_morning_briefing_description(""), "[briefing]");
    }

    #[test]
    fn description_passes_short_spoken_through() {
        assert_eq!(
            format_morning_briefing_description("早上好，今天 25 度，记得带伞"),
            "[briefing] 早上好，今天 25 度，记得带伞"
        );
    }

    #[test]
    fn block_reason_passes_when_all_clear() {
        assert_eq!(morning_briefing_block_reason(true, false, false, true), None);
        // respect_focus=false 时 focus_active 应被忽略
        assert_eq!(morning_briefing_block_reason(true, false, true, false), None);
    }

    #[test]
    fn block_reason_disabled_takes_precedence() {
        // 即便 mute / focus 都成立，disabled 仍然是它给出的拒绝原因 —
        // 顺序保证 disabled 一旦设置，下游不会拿到混淆的 "muted" 告警。
        assert_eq!(
            morning_briefing_block_reason(false, true, true, true),
            Some("disabled")
        );
        assert_eq!(
            morning_briefing_block_reason(false, false, false, true),
            Some("disabled")
        );
    }

    #[test]
    fn block_reason_muted_takes_precedence_over_focus() {
        // 用户按了 mute，焦点状态就不需要再追究。
        assert_eq!(
            morning_briefing_block_reason(true, true, true, true),
            Some("muted")
        );
        assert_eq!(
            morning_briefing_block_reason(true, true, false, true),
            Some("muted")
        );
    }

    #[test]
    fn block_reason_focus_only_blocks_when_respected() {
        assert_eq!(
            morning_briefing_block_reason(true, false, true, true),
            Some("focus")
        );
        // 用户在设置里关掉「尊重 Focus」时，focus_active 不再阻塞早安。
        assert_eq!(
            morning_briefing_block_reason(true, false, true, false),
            None
        );
    }

    #[test]
    fn description_truncates_long_spoken() {
        let long: String = "记".repeat(BRIEFING_DESCRIPTION_CHAR_CAP + 30);
        let out = format_morning_briefing_description(&long);
        assert!(out.starts_with("[briefing] "));
        assert!(out.ends_with('…'));
        // [briefing]<space> + cap chars + …  →  cap+1 chars in body
        let body = out.trim_start_matches("[briefing] ");
        assert_eq!(body.chars().count(), BRIEFING_DESCRIPTION_CHAR_CAP + 1);
    }
}
