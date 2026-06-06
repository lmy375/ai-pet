//! Daily plan / review reading + morning-briefing flag-file + session load /
//! persist helpers. All used by the proactive turn / morning-briefing /
//! daily-review paths in proactive.rs root.

use crate::commands::session;

use super::prompt_assembler::format_plan_hint;

/// Iter R12: bare description read of `ai_insights/daily_plan`. Used by the
/// daily-review writer which needs the plan body without the prompt-hint
/// header. Empty when nothing's been written. Stays unredacted because the
/// review caller redacts at the bullet-line level.
pub(super) fn read_daily_plan_description() -> String {
    crate::commands::memory::read_ai_insights_item("daily_plan")
        .map(|i| i.description)
        .unwrap_or_default()
}

/// Iter R16: read the description of `ai_insights/daily_review_YYYY-MM-DD`
/// for the given date. Returns the raw description (e.g.
/// "[review] 今天主动开口 7 次，计划 3/5") or None if the entry doesn't
/// exist. Caller (proactive turn) hands it to `format_yesterday_recap_hint`
/// to reframe past-tense for the prompt.
pub(super) fn read_daily_review_description(date: chrono::NaiveDate) -> Option<String> {
    let title = format!("daily_review_{}", date);
    crate::commands::memory::read_ai_insights_item(&title).map(|i| i.description)
}

/// Iter R12: index-existence check for cross-process-restart idempotency.
/// LAST_DAILY_REVIEW_DATE only covers the current process; if the user
/// restarts the app at 23:00 after the 22:00 review already wrote, the
/// in-memory date is None and we'd otherwise re-fire. This catches that.
pub(super) fn daily_review_exists(title: &str) -> bool {
    crate::commands::memory::read_ai_insights_item(title).is_some()
}

/// 早安简报跨进程幂等：把"最后一次成功播报的本地日期"写到 config_dir 的
/// `morning_briefing_last.txt`（单行 `YYYY-MM-DD`）。重启后 LAST_MORNING_
/// BRIEFING_DATE 静态值为 None，但本文件仍在 → exists 检查命中，不会重发。
///
/// 之前用 ai_insights/morning_briefing_YYYY-MM-DD memory 条目做幂等。但
/// 用户反馈记忆系统应当只存技能 / 偏好，不存事件日志，所以剥离到独立的
/// 状态文件，让 memory 视图不再被一堆每日早安条目灌污染。
pub(super) fn briefing_flag_path() -> Option<std::path::PathBuf> {
    Some(dirs::config_dir()?.join("pet").join("morning_briefing_last.txt"))
}

pub(super) fn morning_briefing_exists(today_iso: &str) -> bool {
    let Some(p) = briefing_flag_path() else {
        return false;
    };
    match std::fs::read_to_string(&p) {
        Ok(content) => content.trim() == today_iso,
        Err(_) => false,
    }
}

pub(super) fn record_morning_briefing_done(today_iso: &str) {
    let Some(p) = briefing_flag_path() else {
        return;
    };
    if let Some(parent) = p.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&p, today_iso);
}

pub(super) fn build_plan_hint() -> String {
    let description = crate::commands::memory::read_ai_insights_item("daily_plan")
        .map(|i| i.description)
        .unwrap_or_default();
    format_plan_hint(&description, &|s| s.to_string())
}

/// Load the most recent session's messages (without the proactive prompt). Returns
/// `(session_id, messages)` or `(None, [])` if none exists yet.
pub(super) fn load_active_session() -> (Option<String>, Vec<serde_json::Value>) {
    let index = session::list_sessions();
    let Some(meta) = index.sessions.last().cloned() else {
        return (None, vec![]);
    };
    match session::load_session(meta.id.clone()) {
        Ok(s) => (Some(s.id), s.messages),
        Err(_) => (None, vec![]),
    }
}

/// Append an assistant turn to the active session file so the bubble + history reflect it.
pub(super) fn persist_assistant_message(session_id: &str, text: &str) -> Result<(), String> {
    let mut sess = session::load_session(session_id.to_string())?;
    sess.messages
        .push(serde_json::json!({ "role": "assistant", "content": text }));
    sess.items
        .push(serde_json::json!({ "type": "assistant", "content": text }));
    sess.updated_at = chrono::Local::now()
        .format("%Y-%m-%dT%H:%M:%S%.3f")
        .to_string();
    session::save_session(sess)
}
