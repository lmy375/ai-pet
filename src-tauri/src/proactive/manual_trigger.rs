//! Manual fire Tauri commands: 3 trigger entry points used by PanelDebug
//! "▶️ 立即开口" + per-task variants + prompt-override variant. Bypass the
//! gates and record outcome into the same channels the loop uses (counters /
//! decision log / manual fire ring buffer).

use super::{
    chatty_mode_tag, push_manual_fire_history, record_proactive_outcome, run_proactive_turn,
    InteractionClockStore, ManualFireRecord, FORCED_PROMPT_OVERRIDE, FORCED_TASK_FOCUS,
    LAST_MANUAL_FIRE, LAST_MANUAL_FIRE_HISTORY,
};
use crate::commands::settings::get_settings;
use tauri::Manager;

/// Force a proactive turn right now, bypassing the gates (awaiting / cooldown / idle /
/// quiet hours / focus / input-idle). Real values are still passed through into the
/// prompt so the LLM sees the actual idle stats. Used by panel "fire now" / demo flows
/// and for prompt iteration without waiting for natural conditions.
///
/// Iter QG3: routes the same outcome through `record_proactive_outcome` so manual
/// triggers update llm_outcome counters, env_tool stats and the decision-log just
/// like the loop. `source="manual"` keeps the panel able to tell them apart.
/// Per-task variant of `trigger_proactive_turn`. Panel "▶️ 现在跑一次"
/// passes a specific butler_task title — we stash it in FORCED_TASK_FOCUS
/// (consumed by the next `run_proactive_turn`) and call the regular path.
///
/// `RAII` defer-clear pattern: even if `trigger_proactive_turn` itself
/// panics before `run_proactive_turn` is reached, the static gets cleared
/// so a later natural tick doesn't inherit the stale focus.
#[tauri::command]
pub async fn trigger_proactive_turn_for_task(
    app: tauri::AppHandle,
    title: String,
) -> Result<String, String> {
    let title_trim = title.trim();
    if title_trim.is_empty() {
        return Err("title is required".to_string());
    }
    // Stash before delegating; defer clear via guard so panic safety holds.
    if let Ok(mut g) = FORCED_TASK_FOCUS.lock() {
        *g = Some(title_trim.to_string());
    }
    struct ClearOnDrop;
    impl Drop for ClearOnDrop {
        fn drop(&mut self) {
            if let Ok(mut g) = FORCED_TASK_FOCUS.lock() {
                // 兜底：run_proactive_turn 路径正常时已经 take 走，此处是 noop；
                // panic / 早 return 路径下确保 static 不会泄漏 stale title。
                *g = None;
            }
        }
    }
    let _guard = ClearOnDrop;
    let result = trigger_proactive_turn(app).await;
    // trigger_proactive_turn 写了一条 title=None 的 LAST_MANUAL_FIRE +
    // history ring 末条。我们把两处的 title 都改成 Some(本次目标)，让 panel
    // 能区分"全局 manual fire" vs "per-item fire"。Mutex 拿不到（panic 中
    // 或被毒化）时静默忽略。
    if let Ok(mut g) = LAST_MANUAL_FIRE.lock() {
        if let Some(rec) = g.as_mut() {
            rec.title = Some(title_trim.to_string());
        }
    }
    if let Ok(mut g) = LAST_MANUAL_FIRE_HISTORY.lock() {
        if let Some(rec) = g.back_mut() {
            rec.title = Some(title_trim.to_string());
        }
    }
    result
}

/// PanelDebug "立即开口 with prompt" 路径：用户在 modal 内改了 SOUL，
/// 想用临时 prompt 跑一次而不写盘。塞 FORCED_PROMPT_OVERRIDE 后 delegate
/// 给 `trigger_proactive_turn`；run_proactive_turn 起跑时 take 一次。
/// RAII defer-clear 兜底防 panic 漏。
#[tauri::command]
pub async fn trigger_proactive_turn_with_prompt(
    app: tauri::AppHandle,
    soul_override: String,
) -> Result<String, String> {
    let trimmed = soul_override.trim();
    if trimmed.is_empty() {
        return Err("soul_override is required".to_string());
    }
    if let Ok(mut g) = FORCED_PROMPT_OVERRIDE.lock() {
        *g = Some(trimmed.to_string());
    }
    struct ClearOnDrop;
    impl Drop for ClearOnDrop {
        fn drop(&mut self) {
            if let Ok(mut g) = FORCED_PROMPT_OVERRIDE.lock() {
                *g = None;
            }
        }
    }
    let _guard = ClearOnDrop;
    trigger_proactive_turn(app).await
}

#[tauri::command]
pub async fn trigger_proactive_turn(app: tauri::AppHandle) -> Result<String, String> {
    let clock = app.state::<InteractionClockStore>().inner().clone();
    let snap = clock.snapshot().await;
    let input_idle = crate::input_idle::user_input_idle_seconds().await;
    let started = std::time::Instant::now();
    let result = run_proactive_turn(&app, snap.idle_seconds, input_idle).await;

    // Sample chatty_tag fresh so the manual trigger gets the same annotation
    // shape as the loop. rules_tag is None for manual: gates were bypassed so
    // there's no "this rule fired" set to record (see helper doc).
    let chatty_today = crate::speech_history::today_speech_count().await;
    let chatty_threshold = get_settings()
        .ok()
        .map(|s| s.proactive.effective_chatty_threshold())
        .unwrap_or(5);
    let chatty_part =
        chatty_mode_tag(chatty_today, chatty_threshold).unwrap_or_else(|| "-".to_string());
    let counters = app
        .state::<crate::commands::debug::ProcessCountersStore>()
        .inner()
        .clone();
    let decisions = app
        .state::<crate::decision_log::DecisionLogStore>()
        .inner()
        .clone();
    record_proactive_outcome(&counters, &decisions, "manual", &chatty_part, None, &result);

    // Compute the user-facing response string once: same shape for success / silent /
    // 触发失败 paths. Stashes into LAST_MANUAL_FIRE for PanelDebug audit before
    // unwrapping the Result so error fires also show up in the panel.
    let elapsed_ms = started.elapsed().as_millis();
    let response_string = match &result {
        Ok(outcome) => match &outcome.reply {
            Some(text) => format!(
                "开口完成 ({} ms, idle={}s): {}",
                elapsed_ms, snap.idle_seconds, text
            ),
            None => format!(
                "宠物选择沉默 ({} ms, idle={}s)",
                elapsed_ms, snap.idle_seconds
            ),
        },
        Err(e) => format!("触发失败：{}", e),
    };
    let record = ManualFireRecord {
        timestamp: chrono::Local::now()
            .format("%Y-%m-%d %H:%M:%S")
            .to_string(),
        // Default to None (global manual fire); the per-task wrapper
        // overwrites .title to Some(...) after this returns.
        title: None,
        result: response_string.clone(),
    };
    if let Ok(mut g) = LAST_MANUAL_FIRE.lock() {
        *g = Some(record.clone());
    }
    // 同步推入历史 ring（cap 5）。per-task 路径在本函数 await 后会回过
    // 头改 LAST_MANUAL_FIRE.title — 但 ring 已固化此条 title=None。改
    // 进：让 per-task wrapper 同时改 ring 最后一条。下方 wrapper 处。
    push_manual_fire_history(record);
    result.map(|_| response_string)
}
