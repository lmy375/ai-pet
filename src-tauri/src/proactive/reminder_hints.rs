//! Reminder-system prompt hint builders + `/pending_reminders` Tauri command.
//! Builds the "due now" reminder block injected into proactive prompts, with
//! dedup-then-log into butler_history (each `(title, date)` pair only logged
//! once per day) so the cluster detector sees a clean event stream.

use super::reminder_cluster;
use super::reminder_context;
use super::reminders::{
    format_reminders_hint, format_target, is_reminder_due, parse_reminder_prefix,
    ReminderTarget,
};

#[derive(serde::Serialize)]
pub struct PendingReminder {
    pub time: String,
    pub topic: String,
    pub title: String,
    pub due_now: bool,
}

/// List every parseable reminder currently in the `todo` memory category, regardless of
/// whether it's due. Lets the panel show both "set for later" entries (helpful to verify
/// the chat actually wrote them) and "due now" entries (helpful to confirm the
/// proactive loop will surface them next tick).
#[tauri::command]
pub fn get_pending_reminders() -> Vec<PendingReminder> {
    let now = chrono::Local::now().naive_local();
    let items = crate::db::todos_as_memory_items();
    let mut out = Vec::new();
    for item in &items {
        if let Some((target, topic)) = parse_reminder_prefix(&item.description) {
            out.push(PendingReminder {
                time: format_target(&target),
                topic,
                title: item.title.clone(),
                due_now: is_reminder_due(&target, now, 30),
            });
        }
    }
    out
}

/// 每日 reminder dedup 表：进程内追踪「今天已写过 butler_history 的 (title, date)」
/// 日期。同一 reminder 在同一天会被 due-now 检测命中多次（30min 窗口里
/// 每 tick 都命中一次），但聚类需求是「每天 1 次」。这张表让 record_event
/// 调用每个 (title, date) 只触发一次。
///
/// 进程内 Mutex；重启后清表 → 第一次 tick 会重日志一次。可接受 —— 重
/// 启不每天发生，cluster 算法又按 dates 去重，单次冗余写入不影响命中数。
static REMINDER_LOG_DEDUP: std::sync::Mutex<
    Option<std::collections::HashMap<String, chrono::NaiveDate>>,
> = std::sync::Mutex::new(None);

/// Scan the `todo` memory category for items whose description starts with a reminder
/// prefix and are due now (within the 30-minute window). Returns a multi-line bullet
/// hint listing the due reminders, or empty when no due items are found.
///
/// Iter QG5: 命中 due 时 fire-and-forget 调 record_event 写 butler_history.log，
/// 给 [`reminder_cluster::detect_clusters`] 提供历史数据源。
pub(super) fn build_reminders_hint(now: chrono::NaiveDateTime) -> String {
    let todos = crate::db::todos_as_memory_items();
    let mut items: Vec<(String, String, String)> = Vec::new();
    let today = now.date();
    for item in &todos {
        if let Some((target, topic)) = parse_reminder_prefix(&item.description) {
            if is_reminder_due(&target, now, 30) {
                // 取 HH:MM 用于 cluster snippet，复用 reminders.rs 的格式器。
                let (hh, mm) = match &target {
                    ReminderTarget::TodayHour(h, m) => (*h, *m),
                    ReminderTarget::Absolute(dt) => {
                        use chrono::Timelike;
                        (dt.hour() as u8, dt.minute() as u8)
                    }
                };
                // dedup-then-log。锁短粒度；spawn 异步写盘不阻塞 hint 构建。
                let should_log = {
                    let mut g = REMINDER_LOG_DEDUP.lock().ok();
                    if let Some(ref mut opt) = g {
                        let map = opt.get_or_insert_with(std::collections::HashMap::new);
                        match map.get(&item.title) {
                            Some(d) if *d == today => false,
                            _ => {
                                map.insert(item.title.clone(), today);
                                true
                            }
                        }
                    } else {
                        false
                    }
                };
                if should_log {
                    let title_for_log = item.title.clone();
                    let topic_for_log = topic.clone();
                    tauri::async_runtime::spawn(async move {
                        let body = reminder_cluster::format_reminder_log_body(
                            hh,
                            mm,
                            &topic_for_log,
                        );
                        crate::butler_history::record_event(
                            "reminder",
                            &title_for_log,
                            &body,
                        )
                        .await;
                    });
                }
                items.push((format_target(&target), topic, item.title.clone()));
            }
        }
    }
    format_reminders_hint(&items, &|s| s.to_string())
}

/// GOAL 004：在 [`build_reminders_hint`] 输出之后追加「周期性观察」段。
/// 读 butler_history.log → 解析 `reminder` 事件流 → 聚类 → 命中
/// `MIN_HITS_FOR_PROPOSAL` 时把提议文案拼到尾巴。无 cluster / 无 hint
/// 都返回不变的原文，行为对回 noop。async 仅因 butler_history 读盘 IO。
pub(super) async fn build_reminders_hint_with_proposals(
    now_local: chrono::DateTime<chrono::Local>,
) -> String {
    let base = build_reminders_hint(now_local.naive_local());
    if base.is_empty() {
        // 没 due reminder 就不挂提议 —— 让 LLM 不必在「主人没设过提醒」
        // 时还看到「周期性观察」噪音。
        return base;
    }
    // GOAL 033：reminder fire 时附"近期相关 memory"上下文。先抓 due topic
    // list（无副作用扫，不重写 REMINDER_LOG_DEDUP）→ 检索 → 拼接 context
    // block。disabled / 全空命中 → 跳过 enrich，base 原样返回。
    let due_topics = collect_due_reminder_topics(now_local.naive_local());
    let context_block = if !due_topics.is_empty() {
        let per = reminder_context::retrieve_for_due_reminders(
            &due_topics,
            now_local.date_naive(),
        )
        .await;
        let per_refs: Vec<(String, Vec<&crate::commands::memory::MemoryItem>)> = per
            .iter()
            .map(|(topic, items)| (topic.clone(), items.iter().collect()))
            .collect();
        reminder_context::format_context_block(&per_refs)
    } else {
        String::new()
    };

    let history = crate::butler_history::read_history_content().await;
    let events = reminder_cluster::parse_reminder_events_from_history(&history);
    let clusters = reminder_cluster::detect_clusters(&events, now_local);
    let proposal = reminder_cluster::format_proposal_hint(&clusters);

    let mut out = base;
    if !context_block.is_empty() {
        out.push_str(&context_block);
    }
    if !proposal.is_empty() {
        out.push_str("\n\n");
        out.push_str(&proposal);
    }
    out
}

/// GOAL 033 helper：扫 todo cat 拿 due 的 (topic) 列表。**无副作用**——
/// 与 [`build_reminders_hint`] 不同，不写 REMINDER_LOG_DEDUP / 不发
/// butler_history reminder event。专给 context-enrich 路径用，避免与
/// hint 构建路径重复 log 同一条 reminder。
pub(super) fn collect_due_reminder_topics(now: chrono::NaiveDateTime) -> Vec<String> {
    let todos = crate::db::todos_as_memory_items();
    let mut topics: Vec<String> = Vec::new();
    for item in &todos {
        if let Some((target, topic)) = parse_reminder_prefix(&item.description) {
            if is_reminder_due(&target, now, 30) {
                if !topic.trim().is_empty() && !topics.iter().any(|t| t == &topic) {
                    topics.push(topic);
                }
            }
        }
    }
    topics
}
