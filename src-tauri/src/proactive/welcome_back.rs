//! Welcome-back — proactive 触发源：用户从长时 idle 回到桌前的瞬间
//! 主动打招呼（GOAL 008）。
//!
//! 与 [`morning_briefing`] / [`memory_follow_up`] 同结构：本模块只暴露
//! **纯** 函数（判定 + intent 文本），所有 IO（读 idle / 调 LLM / 落
//! mood / emit 事件）由 `proactive.rs` 的 async wrapper 在上层处理。
//!
//! 触发条件（详见 GOAL 008）：
//! 1. 上一观测周期 idle ≥ [`IDLE_THRESHOLD_SECS`]（用户已"离开"）；
//! 2. 当前观测周期 idle ≤ [`RETURN_DETECTION_SECS`]（用户已"回来"）；
//! 3. 本次「离开-回来」周期尚未触发过 welcome-back（per-session dedup）；
//! 4. 距上次成功 welcome-back ≥ [`REPEAT_COOLDOWN_HOURS`] 小时（全局节流，
//!    防"短暂离桌-回来"反复打扰）。

use chrono::{DateTime, Local};
use tauri::{AppHandle, Emitter, Manager};

use crate::commands::chat::{run_chat_pipeline, ChatMessage, CollectingSink};
use crate::commands::debug::{write_log, LogStore};
use crate::commands::settings::get_soul;
use crate::commands::shell::ShellStore;
use crate::config::AiConfig;
use crate::mcp::McpManagerStore;
use crate::mood::{read_current_mood_parsed, read_mood_for_event};
use crate::tools::ToolContext;

use super::clock::InteractionClockStore;
use super::gate::{self, mute_remaining_seconds};
use super::prompt_assembler::is_silent_reply;
use super::session_helpers::{load_active_session, persist_assistant_message};
use super::ProactiveMessage;

/// idle 累积超过此秒数视作「用户已离开」。GOAL 默认 30min。再短容易把
/// 「读长文档不动鼠标」误判成离开；再长又错过短暂离开-回来的陪伴瞬间。
pub const IDLE_THRESHOLD_SECS: u64 = 1800;

/// idle 跌到此秒数以下视作「用户已回来」。15s 给真人鼠标飘过一两次的
/// 容错空间 —— 不需要等用户敲键盘那一刻才触发，鼠标恢复就够。
pub const RETURN_DETECTION_SECS: u64 = 15;

/// 两次成功 welcome-back 之间的全局冷却（小时）。2h = 工作日里午餐 / 下
/// 午茶常见离桌节奏，过密会变唐僧；过疏又让"回桌瞬间"信号失效。
pub const REPEAT_COOLDOWN_HOURS: i64 = 2;

/// Pure：组合所有「该不该 fire」的条件成单一 bool。把跨 tick 的「上一
/// 次 idle 状态 / 本周期是否已 fire / 全局上次 fire 时间」都作为入参显
/// 式传，便于纯单测覆盖所有状态机分支。
///
/// 返回 true 当且仅当：
/// - `prev_idle_secs` ≥ [`IDLE_THRESHOLD_SECS`]（之前确实离开过），且
/// - `cur_idle_secs` ≤ [`RETURN_DETECTION_SECS`]（现在已经回来），且
/// - `!fired_this_session`（本周期还没触发），且
/// - 距 `last_welcome_back_at` ≥ [`REPEAT_COOLDOWN_HOURS`]（或为 None）。
pub fn should_fire_welcome_back(
    prev_idle_secs: Option<u64>,
    cur_idle_secs: Option<u64>,
    fired_this_session: bool,
    last_welcome_back_at: Option<DateTime<Local>>,
    now: DateTime<Local>,
) -> bool {
    if fired_this_session {
        return false;
    }
    let prev = match prev_idle_secs {
        Some(p) => p,
        None => return false, // 第一 tick / 非 macOS — 无法判断"离开"
    };
    let cur = match cur_idle_secs {
        Some(c) => c,
        None => return false,
    };
    if prev < IDLE_THRESHOLD_SECS {
        return false; // 之前根本没离开
    }
    if cur > RETURN_DETECTION_SECS {
        return false; // 现在还没真正回来
    }
    if let Some(last) = last_welcome_back_at {
        let elapsed = now.signed_duration_since(last);
        if elapsed.num_hours() < REPEAT_COOLDOWN_HOURS {
            return false;
        }
    }
    true
}

/// 是否「离开 session 已开启」—— 用来在 cur_idle 重新越过阈值时把
/// per-session 的 `fired_this_session` 标志位 reset 回 false，让下一次
/// 回桌可以再触发。Pure / testable。
pub fn is_new_idle_session(prev_idle_secs: Option<u64>, cur_idle_secs: Option<u64>) -> bool {
    match (prev_idle_secs, cur_idle_secs) {
        // 上次 < 阈值，本次 ≥ 阈值 = 刚刚迈入"离开"
        (Some(prev), Some(cur)) => prev < IDLE_THRESHOLD_SECS && cur >= IDLE_THRESHOLD_SECS,
        _ => false,
    }
}

/// 拼好给 LLM 的 intent 文本。把离开时长 + 当下 mood + 当前 transient
/// note 都丢进 prompt，让模型围绕它们自然组合一句问候。明确反模板化
/// 要求（与 GOAL 「不走固定模板」对齐）；保留 `[SILENT]` 退出口，让
/// 模型判断"现在打招呼不合时宜"时安静跳过。
pub fn format_welcome_back_intent(
    idle_minutes: u64,
    mood_hint: Option<&str>,
    transient_note: Option<&str>,
) -> String {
    let mut out = String::new();
    out.push_str("【欢迎回来】用户刚刚结束一段离桌时间回到电脑前：\n");
    out.push_str(&format!("- 离开时长：约 {} 分钟\n", idle_minutes));
    if let Some(m) = mood_hint.map(str::trim).filter(|s| !s.is_empty()) {
        out.push_str(&format!("- 当前心情：{}\n", m));
    }
    if let Some(n) = transient_note.map(str::trim).filter(|s| !s.is_empty()) {
        out.push_str(&format!("- 当前 transient_note：{}\n", n));
    }
    out.push_str(
        "\n请你扮演宠物角色，用一两句自然口语跟主人打个招呼 —— \
         不要照搬「欢迎回来！想我了吗」式固定模板；可以提一下离开时长 / \
         心情 / transient_note 里任何一个素材，也可以什么都不提，只问一句\
         「回来啦」。控制在 40 字内。如果你判断现在打扰主人不合适\
         （刚回桌就有 transient_note 写「专心工作中」之类），直接回复 \
         `[SILENT]` 跳过这次。",
    );
    out
}

/// GOAL 008：跨 tick 跟踪 input idle 状态机的两个静态。
///
/// - `LAST_OBSERVED_INPUT_IDLE_SECS`：上一 tick 的 idle 秒数，用于本 tick
///   判断「prev ≥ THRESHOLD && cur ≤ RETURN」回桌跳变。
/// - `WELCOME_BACK_FIRED_THIS_SESSION`：当前「离开-回来」周期是否已 fire
///   过；用户重新越过阈值再次离开时，由 `is_new_idle_session` 检测后
///   reset 回 false。
/// - `LAST_WELCOME_BACK_TIME`：上次成功 emit 的 wall-clock 时间，给全局
///   2h 冷却查询。
///
/// 三个都是进程内 Mutex；重启清零（与 LAST_MORNING_BRIEFING_DATE 等同款
/// 「重启即重算」风格）。
static LAST_OBSERVED_INPUT_IDLE_SECS: std::sync::Mutex<Option<u64>> =
    std::sync::Mutex::new(None);
static WELCOME_BACK_FIRED_THIS_SESSION: std::sync::Mutex<bool> = std::sync::Mutex::new(false);
static LAST_WELCOME_BACK_TIME: std::sync::Mutex<Option<chrono::DateTime<chrono::Local>>> =
    std::sync::Mutex::new(None);

/// Welcome-back async wrapper（GOAL 008）。读 input_idle、跟踪跨 tick 状
/// 态、命中条件后跑 LLM、emit ProactiveMessage。与其它 `maybe_run_*` 同
/// 设计：任何失败 (IO / API / LLM) 都吞 None，不阻塞主循环。
///
/// 让位 morning_briefing 的实现：若 morning_briefing 本 tick 刚 emit
/// （`mark_proactive_spoken` 已被打），通过 InteractionClock 看到
/// `since_last_proactive_seconds` 很小 → 主动跳过本 tick；下一 tick 再
/// 评估（用户回桌的窗口是分钟级，不差这 15s）。
pub(super) async fn maybe_run(
    app: &AppHandle,
    now_local: chrono::DateTime<chrono::Local>,
) -> Option<String> {
    // Gate 0: mute（与 morning_briefing / memory_follow_up 同款短路）。
    if mute_remaining_seconds().is_some() {
        return None;
    }

    // Read 当前 input idle；非 macOS 或 ioreg 失败时 None，跨 tick 状态
    // 机会维持 prev=None → should_fire 自动返回 false（与首 tick 同分支）。
    let cur_idle = crate::input_idle::user_input_idle_seconds().await;

    // 上一 tick 的 idle 值；同时把本 tick 的写回去给下一 tick 用。
    let prev_idle = {
        let mut g = LAST_OBSERVED_INPUT_IDLE_SECS.lock().ok()?;
        let prev = *g;
        *g = cur_idle;
        prev
    };

    // 检测「刚迈入离开」：cur 第一次越过阈值时 reset per-session 标志，
    // 让下一次"回桌"可以再次触发。放在 fire 判定之前 —— 同一 tick 里既
    // 越过阈值又恢复在物理上不可能。
    if is_new_idle_session(prev_idle, cur_idle) {
        if let Ok(mut g) = WELCOME_BACK_FIRED_THIS_SESSION.lock() {
            *g = false;
        }
    }

    let fired_this_session = WELCOME_BACK_FIRED_THIS_SESSION
        .lock()
        .ok()
        .map(|g| *g)
        .unwrap_or(true); // lock fail → 视作已触发以免重复 emit
    let last_welcome_back_at = LAST_WELCOME_BACK_TIME.lock().ok().and_then(|g| *g);

    if !should_fire_welcome_back(
        prev_idle,
        cur_idle,
        fired_this_session,
        last_welcome_back_at,
        now_local,
    ) {
        return None;
    }

    // 让位 morning_briefing / memory_follow_up：若 60s 内有 proactive
    // 刚 emit，本触发跳过；下 tick 再说。
    let clock = app.state::<InteractionClockStore>().inner().clone();
    let snap = clock.snapshot().await;
    if let Some(since) = snap.since_last_proactive_seconds {
        if since < 60 {
            return None;
        }
    }

    // 离开时长：用 prev_idle（"刚才一直没动"的累计秒数）转分钟。
    let idle_minutes = prev_idle.unwrap_or(0) / 60;

    // 上下文素材：mood + transient_note。两者都允许为空，format 函数会
    // 跳过空段，让 LLM 在没素材时也能输出"回来啦"式短问候。
    let log_store = app.state::<LogStore>().inner().clone();
    let mood_hint = read_current_mood_parsed()
        .map(|(t, _)| t)
        .filter(|t| !t.trim().is_empty());
    let transient = gate::transient_note_active();

    let intent = format_welcome_back_intent(
        idle_minutes,
        mood_hint.as_deref(),
        transient.as_deref(),
    );

    let config = match AiConfig::from_settings() {
        Ok(c) => c,
        Err(e) => {
            write_log(&log_store.0, &format!("WelcomeBack: AiConfig: {}", e));
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
    let sink = CollectingSink::new();
    let reply = match run_chat_pipeline(messages, &sink, &config, &mcp_store, &ctx).await {
        Ok(r) => r,
        Err(e) => {
            write_log(
                &log_store.0,
                &format!("WelcomeBack: chat pipeline error: {}", e),
            );
            return None;
        }
    };
    let reply_trimmed = reply.trim();
    if is_silent_reply(reply_trimmed) {
        // 模型主动选择沉默时也 mark fired —— 用户已经回桌，re-trying
        // 同样的素材没意义，等下一次离开-回来周期。
        if let Ok(mut g) = WELCOME_BACK_FIRED_THIS_SESSION.lock() {
            *g = true;
        }
        return None;
    }

    // 持久化 + emit：与其它 proactive 触发同末段。
    if let Some(id) = load_active_session().0 {
        let _ = persist_assistant_message(&id, reply_trimmed);
    }
    clock.mark_proactive_spoken().await;
    super::record_speech_with_current_meta(reply_trimmed).await;

    let (mood_after, motion_after) = read_mood_for_event(&ctx, "WelcomeBack");
    if let Some(text) = &mood_after {
        crate::mood_history::record_mood(text, &motion_after).await;
    }
    let payload = ProactiveMessage {
        text: reply_trimmed.to_string(),
        timestamp: now_local.format("%Y-%m-%dT%H:%M:%S%.3f").to_string(),
        mood: mood_after,
        motion: motion_after,
        image_url: None,
    };
    let _ = app.emit("proactive-message", payload);
    super::unread_tray::record_emitted(app);

    if let Ok(mut g) = WELCOME_BACK_FIRED_THIS_SESSION.lock() {
        *g = true;
    }
    if let Ok(mut g) = LAST_WELCOME_BACK_TIME.lock() {
        *g = Some(now_local);
    }
    decisions.push("WelcomeBack", format!("idle_back {}min", idle_minutes));

    Some(reply_trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<Local> {
        Local::now()
    }

    #[test]
    fn fires_when_idle_drops_from_above_threshold_to_active() {
        // 30min+ idle → 鼠标恢复（idle ~ 0）→ 应触发
        assert!(should_fire_welcome_back(
            Some(IDLE_THRESHOLD_SECS + 60),
            Some(0),
            false,
            None,
            now(),
        ));
    }

    #[test]
    fn does_not_fire_when_prev_never_idle() {
        // 一直在用电脑：prev 5s, cur 5s → 不该触发
        assert!(!should_fire_welcome_back(
            Some(5),
            Some(2),
            false,
            None,
            now(),
        ));
    }

    #[test]
    fn does_not_fire_when_still_idle() {
        // 用户离开很久，还没回来：prev huge, cur still huge → 不触发
        assert!(!should_fire_welcome_back(
            Some(IDLE_THRESHOLD_SECS + 600),
            Some(IDLE_THRESHOLD_SECS + 605),
            false,
            None,
            now(),
        ));
    }

    #[test]
    fn does_not_fire_when_already_fired_this_session() {
        assert!(!should_fire_welcome_back(
            Some(IDLE_THRESHOLD_SECS + 60),
            Some(0),
            true, // already fired
            None,
            now(),
        ));
    }

    #[test]
    fn does_not_fire_when_within_global_cooldown() {
        // 上次 welcome-back 30min 前 → 仍在 2h 冷却内 → 不触发
        let last = now() - chrono::Duration::minutes(30);
        assert!(!should_fire_welcome_back(
            Some(IDLE_THRESHOLD_SECS + 60),
            Some(0),
            false,
            Some(last),
            now(),
        ));
    }

    #[test]
    fn fires_after_global_cooldown_expires() {
        // 上次 welcome-back 3h 前 → 超过 2h 冷却 → 触发
        let last = now() - chrono::Duration::hours(3);
        assert!(should_fire_welcome_back(
            Some(IDLE_THRESHOLD_SECS + 60),
            Some(0),
            false,
            Some(last),
            now(),
        ));
    }

    #[test]
    fn first_tick_with_none_prev_does_not_fire() {
        // 重启后第一 tick: prev=None。即使 cur=0 也不该 fire（无法判断之前是否离开）
        assert!(!should_fire_welcome_back(None, Some(0), false, None, now()));
    }

    #[test]
    fn cur_idle_just_above_return_threshold_does_not_fire() {
        // 边界：return threshold 15s，cur 20s 还不算"回来"
        assert!(!should_fire_welcome_back(
            Some(IDLE_THRESHOLD_SECS + 60),
            Some(20),
            false,
            None,
            now(),
        ));
    }

    #[test]
    fn is_new_idle_session_detects_crossing_threshold() {
        // 100s → 1900s = 跨越 1800s 阈值 → true
        assert!(is_new_idle_session(Some(100), Some(IDLE_THRESHOLD_SECS + 100)));
        // 100s → 200s = 同在阈值下 → false
        assert!(!is_new_idle_session(Some(100), Some(200)));
        // 1900s → 2000s = 都已在阈值上 → false（已经在 idle 中，不算"新"）
        assert!(!is_new_idle_session(
            Some(IDLE_THRESHOLD_SECS + 100),
            Some(IDLE_THRESHOLD_SECS + 200),
        ));
        // None → 任何 → false
        assert!(!is_new_idle_session(None, Some(IDLE_THRESHOLD_SECS + 100)));
    }

    #[test]
    fn format_intent_includes_idle_minutes() {
        let s = format_welcome_back_intent(45, None, None);
        assert!(s.contains("45 分钟"));
        assert!(s.contains("[SILENT]"));
    }

    #[test]
    fn format_intent_skips_empty_optional_blocks() {
        // 断言只针对 *bullet 行* (`- 当前心情：X` / `- 当前 transient_note：X`)；
        // 后面 prompt copy 提到「心情 / transient_note 里任何一个素材」是
        // 不变的指导语，不应被字符串裸 contains 误触发。
        let s = format_welcome_back_intent(30, Some("  "), Some(""));
        assert!(!s.contains("- 当前心情"));
        assert!(!s.contains("- 当前 transient_note"));
    }

}
