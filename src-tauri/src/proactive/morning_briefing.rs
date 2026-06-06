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
use tauri::{AppHandle, Emitter, Manager};

use crate::commands::chat::{run_chat_pipeline, ChatMessage};
use crate::commands::debug::{write_log, LogStore};
use crate::commands::settings::get_soul;
use crate::commands::shell::ShellStore;
use crate::config::AiConfig;
use crate::mcp::McpManagerStore;
use crate::mood::{read_current_mood_parsed, read_mood_for_event};
use crate::tools::ToolContext;

use super::clock::InteractionClockStore;
use super::gate::mute_remaining_seconds;
use super::prompt_assembler::is_silent_reply;
use super::session_helpers::{
    load_active_session, morning_briefing_exists, persist_assistant_message,
    read_daily_review_description, record_morning_briefing_done,
};
use super::telemetry;
use super::ProactiveMessage;

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
        "请你扮演宠物角色对主人说「早安」，把这早安做成「管家式问候」而非寒暄。\n\n\
         **请按顺序主动调用以下工具**（GOAL 016 enrich）：\n\
         1. `get_weather` —— 拿今天天气，提炼一句话（温度 / 雨 / 风的关键信号）；\n\
         2. `get_upcoming_events` —— 取今天前 3 条日程（时间 + 标题）；\n\
         3. `memory_list` —— 翻 ai_insights/transient_note 和最近 reminder（含 \
         `[remind: today]` / `[recur-daily: ...]`）作背景上下文。\n\n\
         然后按这个**结构**输出（每行 ≤ 30 字、总共 ≤ 6 行）：\n\
         · 行 1：早安 + 称呼\n\
         · 行 2：天气一句（只要 get_weather 拿到了数据）\n\
         · 行 3..N：今日日程（每条一行，最多 3 行；get_upcoming_events 拿到的取前 3）\n\
         · 倒数行：当下心情 / 一句小关怀\n\n\
         **失败处理**：weather / calendar 工具调用任何一个失败（返回 \
         `{\"error\": ...}`）—— 静默跳过对应行，不在早安里抱怨工具炸了。\n\
         **不要罗列**：日程行写「14:00 客户视频」即可，不要写「日程 1: 14:00 客户视频」。",
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
pub fn morning_briefing_block_reason(enabled: bool, muted: bool) -> Option<&'static str> {
    if !enabled {
        return Some("disabled");
    }
    if muted {
        return Some("muted");
    }
    None
}

/// 早安简报触发器。门控通过后调用 LLM 生成一段早安播报，写入 speech_history、
/// 当前 session、ai_insights 标记，并通过 `proactive-message` 事件推送到前端
/// 气泡。与 daily_review 的等价：本函数承担所有 IO，纯门控 + 文本拼装在
/// 上面的 pure helpers。
///
/// 与现有节奏控制层的关系（与文档 docs/20260504-1150-morning-briefing.md
/// 的「与 mute / 主动发言冷却」节对齐）：
/// - 尊重 mute（用户主动按下"安静一会儿"时早安也跟着安静，免得打破期待）；
/// - **绕过**主动发言冷却 — 早安自带"每日 1 次"语义，不参与一般 cooldown 节流；
///   触发后通过 `mark_proactive_spoken` 让常规 proactive 循环之后照常 cooldown。
///
/// 任何 IO 失败都不冒泡 — 早安是 best-effort 信号，失败时静默返回，下一 tick
/// 仍可重试（如未越过 grace 窗口）。
/// GOAL 016：自定 sink 在 LLM tool_result 事件里嗅探 `"error":` 字符串，
/// 命中 get_weather / get_upcoming_events 时 bump 对应 telemetry 计数器。
/// 行为透明（不改 reply），副作用只在静态 atomic 累加 —— 与 CollectingSink
/// 同构（无 reply 收集需求；run_chat_pipeline 已直接返回 reply）。
struct BriefingFailCountingSink;

impl crate::commands::chat::ChatEventSink for BriefingFailCountingSink {
    fn send_chunk(&self, _text: &str) {}
    fn send_tool_start(&self, _name: &str, _arguments: &str) {}
    fn send_tool_result(&self, name: &str, result: &str) {
        use std::sync::atomic::Ordering;
        if !result.contains("\"error\":") {
            return;
        }
        match name {
            "get_weather" => {
                telemetry::BRIEFING_WEATHER_FAIL_COUNT.fetch_add(1, Ordering::Relaxed);
            }
            "get_upcoming_events" => {
                telemetry::BRIEFING_CALENDAR_FAIL_COUNT.fetch_add(1, Ordering::Relaxed);
            }
            _ => {}
        }
    }
    fn send_done(&self) {}
    fn send_error(&self, _message: &str) {}
}

pub(super) async fn maybe_run(
    app: &AppHandle,
    settings: &crate::commands::settings::AppSettings,
    now_local: chrono::DateTime<chrono::Local>,
) -> Option<String> {
    let cfg = &settings.morning_briefing;
    let muted = mute_remaining_seconds().is_some();
    if morning_briefing_block_reason(cfg.enabled, muted).is_some() {
        return None;
    }
    let now_naive = now_local.naive_local();
    let today = now_local.date_naive();
    let last = LAST_MORNING_BRIEFING_DATE.lock().ok().and_then(|g| *g);
    if !should_trigger_morning_briefing(
        now_naive,
        cfg.hour,
        cfg.minute,
        MORNING_BRIEFING_DEFAULT_GRACE_MINUTES,
        last,
    ) {
        return None;
    }
    let today_iso = today.format("%Y-%m-%d").to_string();
    if morning_briefing_exists(&today_iso) {
        if let Ok(mut g) = LAST_MORNING_BRIEFING_DATE.lock() {
            *g = Some(today);
        }
        return None;
    }

    let log_store = app.state::<LogStore>().inner().clone();

    // 拼装 intent
    let yesterday = today.pred_opt().unwrap_or(today);
    let yesterday_excerpt = read_daily_review_description(yesterday);
    let mood_hint = read_current_mood_parsed()
        .map(|(t, _)| t)
        .filter(|t| !t.trim().is_empty());
    let intent = format_morning_briefing_intent(
        settings.user_name.trim(),
        yesterday_excerpt.as_deref(),
        mood_hint.as_deref(),
        today,
    );
    let config = match AiConfig::from_settings() {
        Ok(c) => c,
        Err(e) => {
            write_log(
                &log_store.0,
                &format!("MorningBriefing: AiConfig error: {}", e),
            );
            return None;
        }
    };
    let mcp_store = app.state::<McpManagerStore>().inner().clone();
    let shell_store = app.state::<ShellStore>().inner().clone();
    let process_counters = app
        .state::<crate::commands::debug::ProcessCountersStore>()
        .inner()
        .clone();
    let tool_review = app
        .state::<crate::tool_review::ToolReviewRegistryStore>()
        .inner()
        .clone();
    let decisions = app
        .state::<crate::decision_log::DecisionLogStore>()
        .inner()
        .clone();
    let ctx = ToolContext::new(log_store.clone(), shell_store, process_counters)
        .with_tool_review(tool_review)
        .with_decision_log(decisions.clone());

    let soul = get_soul().unwrap_or_default();
    let messages = vec![
        ChatMessage {
            role: "system".to_string(),
            content: serde_json::Value::String(soul),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        },
        ChatMessage {
            role: "user".to_string(),
            content: serde_json::Value::String(intent),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        },
    ];
    // GOAL 016：用自定 sink 取代默认 CollectingSink — 让 weather / calendar
    // tool 调用失败时 bump telemetry::BRIEFING_*_FAIL_COUNT。reply 仍走
    // run_chat_pipeline 的 Result<String> return path。
    let sink = BriefingFailCountingSink;
    let reply = match run_chat_pipeline(messages, &sink, &config, &mcp_store, &ctx).await {
        Ok(r) => r,
        Err(e) => {
            write_log(
                &log_store.0,
                &format!("MorningBriefing: chat pipeline error: {}", e),
            );
            return None;
        }
    };
    let reply_trimmed = reply.trim();
    if is_silent_reply(reply_trimmed) {
        // 模型主动选择沉默时仍占用今天的"额度"，否则会在 grace 窗口内反复重试。
        write_log(&log_store.0, "MorningBriefing: pet returned empty / silent");
        if let Ok(mut g) = LAST_MORNING_BRIEFING_DATE.lock() {
            *g = Some(today);
        }
        return None;
    }

    // 先读 mood —— briefing LLM 可能在跑工具时更新过情绪，要用最新值给
    // 早安图当 prompt 素材。把原来 L2466 的 mood read 提到这里，下游
    // record_mood / payload / image prompt 共用一份快照。
    let (mood_after, motion_after) = read_mood_for_event(&ctx, "MorningBriefing");
    if let Some(text) = &mood_after {
        crate::mood_history::record_mood(text, &motion_after).await;
    }

    // GOAL 003：按 mood 生成一张可爱风格早安问候图，与 briefing 文字一起
    // 推。失败 / 未配置 image_model 时 None，下游退回「仅文字」原行为。
    // 一天一次硬上限由 LAST_MORNING_BRIEFING_DATE 与 morning_briefing_last
    // .txt 已经守好 —— briefing 触发一次 = 图也只尝试一次。
    let image_url = generate_morning_image(mood_after.as_deref()).await;

    // 拼"持久化文本"：成图时尾巴附 `[早安图]` marker，让磁盘 history /
    // 未来 LLM context / 面板列表都能稳定看到"这条带图"信号。没图就纯
    // 文字（与历史行为一致），不挂空 marker 误导未来 turn。
    let persisted_text = if image_url.is_some() {
        format!("{} [早安图]", reply_trimmed)
    } else {
        reply_trimmed.to_string()
    };

    // 持久化 + 推送给前端：与 run_proactive_turn 末段保持平行。session 落盘失败
    // 不致命（气泡仍会显示）— 仅对它做静默忽略。
    if let Some(id) = load_active_session().0 {
        let _ = persist_assistant_message(&id, &persisted_text);
    }
    let clock = app.state::<InteractionClockStore>().inner().clone();
    clock.mark_proactive_spoken().await;
    // Iter #389: same meta-recording wrapper as run_proactive_turn — let
    // morning briefing speeches 也带触发上下文进 sidecar。
    super::record_speech_with_current_meta(&persisted_text).await;

    // 把"今天的早安已发"写入 morning_briefing_last.txt 而不是 memory ——
    // 早安是事件，不是技能 / 偏好；剥出 memory 让记忆视图保持纯净。
    record_morning_briefing_done(&today_iso);

    let payload = ProactiveMessage {
        text: persisted_text.clone(),
        timestamp: now_local.format("%Y-%m-%dT%H:%M:%S%.3f").to_string(),
        mood: mood_after,
        motion: motion_after,
        image_url,
    };
    let _ = app.emit("proactive-message", payload);
    super::unread_tray::record_emitted(app);

    if let Ok(mut g) = LAST_MORNING_BRIEFING_DATE.lock() {
        *g = Some(today);
    }
    decisions.push(
        "MorningBriefing",
        format!("{} chars", persisted_text.chars().count()),
    );

    Some(persisted_text)
}

/// 早安图 prompt 拼装 + 调用 image_generate 的 best-effort wrapper。
/// mood 为 None 时 fallback 到「平静温暖」让 prompt 仍可生成；失败返回
/// None 让 caller 退回纯文字早安。
///
/// 风格约束（与 GOAL「UI 要美观可爱」对齐）：
/// - 水彩 / 治愈 / 暖色调 —— 不容易和宠物气泡的卡通风格打架；
/// - 「不要文字」—— DALL·E 系列模型默认爱在图里塞字，明确禁掉避免乱码；
/// - 正方形 —— 桌面气泡缩略图 / TG 预览都按 1:1 看着最稳。
///
/// 不传 size override：尊重 settings.image_size，让用户对画幅有控制权。
async fn generate_morning_image(mood: Option<&str>) -> Option<String> {
    let mood_phrase = mood
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .unwrap_or("平静温暖");
    let prompt = format!(
        "为桌面宠物生成一张早安问候小图。情绪基调：{}。\
         风格：温暖明亮的水彩、可爱治愈、宫崎骏式色彩。\
         画面：晨光、植物或宠物本身；简洁正方形构图；\
         画面中不要出现任何文字。",
        mood_phrase
    );
    match crate::commands::image::run_image_generate(&prompt, 1, None).await {
        Ok(result) if !result.urls.is_empty() => result.urls.into_iter().next(),
        Ok(result) => {
            log::warn!(
                "MorningBriefing image: empty urls (errors: {:?})",
                result.errors
            );
            None
        }
        Err(e) => {
            log::warn!("MorningBriefing image: setup error: {}", e);
            None
        }
    }
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
    fn block_reason_passes_when_all_clear() {
        assert_eq!(morning_briefing_block_reason(true, false), None);
    }

    #[test]
    fn block_reason_disabled_takes_precedence() {
        // 即便 mute 也成立，disabled 仍然是它给出的拒绝原因 —
        // 顺序保证 disabled 一旦设置，下游不会拿到混淆的 "muted" 告警。
        assert_eq!(
            morning_briefing_block_reason(false, true),
            Some("disabled")
        );
        assert_eq!(
            morning_briefing_block_reason(false, false),
            Some("disabled")
        );
    }

    #[test]
    fn block_reason_muted_blocks() {
        assert_eq!(
            morning_briefing_block_reason(true, true),
            Some("muted")
        );
    }

}
