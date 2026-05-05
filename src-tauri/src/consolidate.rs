//! Periodic memory consolidation.
//!
//! Spawns a long-period background loop that occasionally asks the LLM to look over the
//! pet's memory index and clean it up: merge duplicates, summarize stale entries, drop
//! trivial ones. The model uses the existing `memory_edit` tool to perform changes — Rust
//! never modifies memories directly. Default disabled and gated behind a minimum item count
//! so we don't spend tokens consolidating an empty index.

use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};

use crate::commands::chat::{run_chat_pipeline, ChatDonePayload, ChatMessage, CollectingSink};
use crate::commands::debug::{write_log, LogStore};
use crate::commands::memory;
use crate::commands::settings::get_settings;
use crate::commands::shell::ShellStore;
use crate::config::AiConfig;
use crate::mcp::McpManagerStore;
use crate::mood::{read_current_mood, read_mood_for_event};
use crate::proactive::{is_stale_reminder, parse_reminder_prefix};
use crate::tools::ToolContext;
use crate::weekly_summary::{
    aggregate_butler_events, aggregate_mood_top, aggregate_speech_count,
    format_weekly_summary_description, format_weekly_summary_detail, iso_week_bounds,
    should_trigger_weekly_summary, weekly_summary_title, WeeklyStats,
    LAST_WEEKLY_SUMMARY_WEEK,
};

/// Spawn the memory consolidation loop. Reads settings each tick so the user can toggle
/// at runtime. Sleeps for `interval_hours` between attempts.
pub fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        // Delay first run a bit so app startup doesn't include LLM calls.
        tokio::time::sleep(Duration::from_secs(120)).await;

        loop {
            let settings = match get_settings() {
                Ok(s) => s,
                Err(_) => {
                    tokio::time::sleep(Duration::from_secs(3600)).await;
                    continue;
                }
            };

            let cfg = &settings.memory_consolidate;
            let interval_secs = cfg.interval_hours.max(1) * 3600;

            // 周报合成：与 LLM consolidate 解耦 —— 即便 cfg.enabled 为 false
            // 也要按时跑（用户可能完全不需要 LLM 整理但想要每周报告）。门控
            // 内部检查"周日 20:00 后 + 该周还没合成"，确定性逻辑，无 token 成本。
            maybe_run_weekly_summary(&app, chrono::Local::now(), cfg.weekly_summary_closing_hour).await;

            if !cfg.enabled {
                tokio::time::sleep(Duration::from_secs(interval_secs)).await;
                continue;
            }

            let log_store = app.state::<LogStore>().inner().clone();
            let total = total_memory_items();

            if total < cfg.min_total_items {
                write_log(
                    &log_store.0,
                    &format!(
                        "Consolidate: skip — only {} items (min {})",
                        total, cfg.min_total_items
                    ),
                );
            } else if let Err(e) = run_consolidation(&app, total).await {
                eprintln!("Consolidate turn failed: {}", e);
            }

            tokio::time::sleep(Duration::from_secs(interval_secs)).await;
        }
    });
}

/// Manually trigger a consolidation pass right now, regardless of the timer interval or
/// the min-items gate. Returns a short status string for the panel to display ("done in
/// N ms / swept K stale" or "skip — only N items"). The user typically uses this after
/// adding many memories or to verify a prompt tweak without waiting hours.
#[tauri::command]
pub async fn trigger_consolidate(app: tauri::AppHandle) -> Result<String, String> {
    let total = total_memory_items();
    let started = std::time::Instant::now();
    let summary = run_consolidation(&app, total).await?;
    let elapsed_ms = started.elapsed().as_millis();
    // Iter D7: include the LLM's own short summary so the panel banner reflects
    // what actually changed ("合并了 2 条 / 删了 1 条 todo / persona_summary 已
    // update") instead of just "Consolidation finished in N ms". Strip / truncate
    // before display: long-form summaries get noisy in a banner.
    let summary_snippet: String = summary.trim().chars().take(160).collect();
    let prefix = format!(
        "Consolidation finished in {} ms ({} items at start)",
        elapsed_ms, total
    );
    if summary_snippet.is_empty() {
        Ok(prefix)
    } else {
        Ok(format!("{} · {}", prefix, summary_snippet))
    }
}

/// Build the consolidation prompt, run it through the chat pipeline so the LLM can call
/// `memory_edit`, and log a before/after item count. Iter D7: returns the LLM's short
/// summary so the panel banner can show it; empty string when the LLM produced no
/// summary text (rare — the prompt asks for one explicitly).
async fn run_consolidation(app: &AppHandle, total_before: usize) -> Result<String, String> {
    let config = AiConfig::from_settings()?;
    let mcp_store = app.state::<McpManagerStore>().inner().clone();
    let log_store = app.state::<LogStore>().inner().clone();
    let shell_store = app.state::<ShellStore>().inner().clone();
    let process_counters = app
        .state::<crate::commands::debug::ProcessCountersStore>()
        .inner()
        .clone();
    let ctx = ToolContext::new(log_store.clone(), shell_store, process_counters);

    // Deterministic sweep first — drop reminders whose Absolute target is past their
    // configured stale cutoff. The LLM later sees a cleaner index and won't waste a
    // call deciding whether to delete each one. TodayHour reminders are intentionally
    // left alone (recurring).
    let cfg_settings = get_settings();
    let now_naive = chrono::Local::now().naive_local();

    let stale_cutoff = cfg_settings
        .as_ref()
        .map(|s| s.memory_consolidate.stale_reminder_hours)
        .unwrap_or(24);
    let swept = sweep_stale_reminders(now_naive, stale_cutoff);
    if swept > 0 {
        write_log(
            &log_store.0,
            &format!(
                "Consolidate: swept {} stale reminder(s) before LLM run",
                swept
            ),
        );
    }

    let plan_cutoff = cfg_settings
        .as_ref()
        .map(|s| s.memory_consolidate.stale_plan_hours)
        .unwrap_or(24);
    if sweep_stale_plan(now_naive, plan_cutoff) {
        write_log(
            &log_store.0,
            "Consolidate: swept stale daily_plan before LLM run",
        );
    }

    // Iter Cλ: also sweep completed [once: ...] butler tasks past their grace period.
    // Mirrors the reminder/plan sweeps — keeps the queue from accumulating finished
    // one-shots that the LLM would otherwise have to triage every consolidate.
    let once_butler_cutoff = cfg_settings
        .as_ref()
        .map(|s| s.memory_consolidate.stale_once_butler_hours)
        .unwrap_or(48);
    let butler_swept = sweep_completed_once_butler_tasks(now_naive, once_butler_cutoff).await;
    if butler_swept > 0 {
        write_log(
            &log_store.0,
            &format!(
                "Consolidate: swept {} completed [once] butler task(s) past {}h grace",
                butler_swept, once_butler_cutoff
            ),
        );
    }

    // Iter R17: prune old daily_review entries past their retention window.
    // Mirrors the reminder/plan/butler sweeps. Quiet success — only logs when
    // something was actually pruned, to avoid every consolidate spamming
    // "swept 0 reviews".
    let review_retention = cfg_settings
        .as_ref()
        .map(|s| s.memory_consolidate.stale_daily_review_days)
        .unwrap_or(30);
    let today_for_sweep = chrono::Local::now().date_naive();
    let reviews_swept = sweep_stale_daily_reviews(today_for_sweep, review_retention);
    if reviews_swept > 0 {
        write_log(
            &log_store.0,
            &format!(
                "Consolidate: pruned {} daily_review(s) older than {} days",
                reviews_swept, review_retention
            ),
        );
    }

    // Iter Cη: derive today's butler-task summary from butler_history.log and
    // upsert into butler_daily.log so the user has a per-day "今天我帮你 ..."
    // trail surfaced in the panel. Pre-LLM and deterministic — survives even if
    // the LLM phase fails. Quiet days (no butler events) leave the file alone.
    let today = chrono::Local::now().date_naive();
    let events =
        crate::butler_history::recent_events(crate::butler_history::BUTLER_HISTORY_CAP).await;
    if let Some(summary) = crate::butler_history::summarize_events_for_date(&events, today) {
        crate::butler_history::record_daily_summary(today, &summary).await;
        write_log(
            &log_store.0,
            &format!("Consolidate: butler_daily updated — {}", summary),
        );
    }

    let index = memory::memory_list(None).map_err(|e| format!("memory_list failed: {e}"))?;
    let index_json =
        serde_json::to_string_pretty(&index).map_err(|e| format!("serialize index: {e}"))?;

    // Only nudge the LLM toward the focus_history.log file when it actually exists — no
    // point asking it to read a path that's empty on a fresh install or non-macOS host.
    let focus_log_hint = focus_history_hint();

    // Iter 102: surface the pet's recent self-utterances so the consolidate run can
    // write/refresh a persona_summary. We strip timestamps for readability; the LLM
    // doesn't need exact times to spot voice patterns. Empty list is fine — the prompt
    // tells the model to skip the persona step when signal is too thin.
    let recent_speeches = crate::speech_history::recent_speeches(30).await;
    let recent_speech_block = if recent_speeches.is_empty() {
        "（你最近还没有主动开过口；本次跳过 persona_summary 维护。）".to_string()
    } else {
        let body: Vec<String> = recent_speeches
            .iter()
            .map(|line| crate::speech_history::strip_timestamp(line).to_string())
            .collect();
        format!(
            "你最近 {} 句主动开口（按时间正序，最新在底部）：\n{}",
            body.len(),
            body.iter()
                .map(|t| format!("- {}", t))
                .collect::<Vec<_>>()
                .join("\n"),
        )
    };

    let prompt = format!(
        "[系统提示·记忆整理]\n\n\
作为 AI 桌面宠物，你正在做后台记忆维护——这次没有用户互动，只是回顾一下你存的记忆。\n\n\
当前记忆索引（共 {total} 条）：\n\n```yaml\n{index}\n```\n\n\
{recent_speeches}\n\n\
请扫一遍这些条目，判断：\n\
1. **重复/同主题**：把内容相近的合并成一条更精炼的——保留信息量大的，用 `memory_edit update` 更新；用 `memory_edit delete` 删掉冗余的。\n\
2. **过期/失效**：明显过时（已完成的 todo、不再相关的临时上下文），用 `memory_edit delete`。`butler_tasks` 类别下如果某条任务用户已经撤回 / 已经完成且不再 recurring，也归这一类。\n\
3. **太琐碎**：完全没有保留价值的（例如随口一句话被记下），删除。\n\
4. **可以补充细节**：如果某条记忆 description 太短、可以扩展但需要查更多上下文，可以用 `memory_edit update` 加入更完整的 detail_content。\n\
5. **维护 `ai_insights/persona_summary`**：基于上面「你最近主动开口」的句子 + `user_profile` 类下的条目，简要总结你观察到的自己的语气特点和与用户的互动模式。description 控制在 ~100 字以内、写第一人称（如「我倾向...」、「我注意到...」）。如果该条目不存在，用 `memory_edit create` 创建到 `ai_insights/persona_summary`；如果已存在并且这次有新观察，用 `update` 更新。如果最近开口少于 5 句、信号不足，跳过这一项。\n\n\
**特殊保护**：`ai_insights/current_mood` 是宠物当前的心情状态，绝对不要删除——可以适当 update 让 description 更准确，但务必保留这条记录、且 description 必须以 `[motion: Tap|Flick|Flick3|Idle] 心情文字` 开头格式。`ai_insights/persona_summary` 同样保护——是你长期人格画像，可 update 不要 delete。\n\n\
{focus_log_hint}\
原则：**保守**。如果不确定一条记忆是否还有价值，就保留。**不要为了整理而整理**——如果索引看起来已经清爽，就什么都不做并输出 `<noop>`。\n\n\
工作完成后，简短总结你做了什么（合并了几条 / 删了几条 / persona_summary 是创建/更新/跳过 / 没改动）。不需要客气，只要事实。",
        total = total_before,
        index = index_json,
        recent_speeches = recent_speech_block,
        focus_log_hint = focus_log_hint,
    );

    let messages: Vec<ChatMessage> = vec![
        serde_json::from_value(serde_json::json!({
            "role": "system",
            "content": "你是一个记忆整理助理。可以并应当使用 memory_edit 工具直接修改记忆。",
        }))
        .unwrap(),
        serde_json::from_value(serde_json::json!({
            "role": "user",
            "content": prompt,
        }))
        .unwrap(),
    ];

    let mood_before = read_current_mood();

    let sink = CollectingSink::new();
    let summary = run_chat_pipeline(messages, &sink, &config, &mcp_store, &ctx).await?;

    let total_after = total_memory_items();
    write_log(
        &log_store.0,
        &format!(
            "Consolidate: done — {} -> {} items. Summary: {}",
            total_before,
            total_after,
            summary.trim().chars().take(200).collect::<String>()
        ),
    );

    // Re-read mood for the post-consolidation snapshot. If consolidation merged or refined
    // the mood entry, we want the desktop pet's Live2D motion to reflect it.
    let (mood, motion) = read_mood_for_event(&ctx, "Consolidate");
    if mood_before.is_some() && mood.is_none() {
        // Despite the explicit prompt protection, the LLM removed the mood entry. Worth
        // surfacing — repeated occurrences mean the protection text needs hardening.
        write_log(
            &log_store.0,
            "Consolidate: WARNING — current_mood entry was removed despite protection rule",
        );
    }
    let payload = ChatDonePayload {
        mood,
        motion,
        timestamp: chrono::Local::now()
            .format("%Y-%m-%dT%H:%M:%S%.3f")
            .to_string(),
    };
    let _ = app.emit("chat-done", payload);

    Ok(summary)
}

/// Sweep the pet's `ai_insights/daily_plan` entry when its `updated_at` is older than
/// `cutoff_hours`. Returns true if the entry was deleted, false otherwise (no plan, or
/// plan still fresh, or any IO/parse failure).
pub fn sweep_stale_plan(now: chrono::NaiveDateTime, cutoff_hours: u64) -> bool {
    let Some(plan) = memory::read_ai_insights_item("daily_plan") else {
        return false;
    };
    // updated_at is written as "%Y-%m-%dT%H:%M:%S%:z" — RFC3339 compatible.
    let Ok(updated) = chrono::DateTime::parse_from_rfc3339(&plan.updated_at) else {
        return false;
    };
    let age = now - updated.naive_local();
    if age <= chrono::Duration::hours(cutoff_hours as i64) {
        return false;
    }
    memory::memory_edit(
        "delete".to_string(),
        "ai_insights".to_string(),
        plan.title,
        None,
        None,
    )
    .is_ok()
}

/// Walk the `todo` memory category, identify reminder entries whose Absolute target is
/// more than `cutoff_hours` past `now`, and delete them via `memory_edit`. Returns the
/// number deleted. Non-reminder todos and TodayHour reminders are left alone — only
/// stale one-shot Absolute reminders are swept.
pub fn sweep_stale_reminders(now: chrono::NaiveDateTime, cutoff_hours: u64) -> usize {
    let Ok(index) = memory::memory_list(Some("todo".to_string())) else {
        return 0;
    };
    let Some(cat) = index.categories.get("todo") else {
        return 0;
    };
    // Collect titles to delete first (don't mutate while iterating the index snapshot).
    let mut to_delete = Vec::new();
    for item in &cat.items {
        if let Some((target, _)) = parse_reminder_prefix(&item.description) {
            if is_stale_reminder(&target, now, cutoff_hours) {
                to_delete.push(item.title.clone());
            }
        }
    }
    let mut count = 0;
    for title in to_delete {
        if memory::memory_edit("delete".to_string(), "todo".to_string(), title, None, None).is_ok()
        {
            count += 1;
        }
    }
    count
}

/// Iter Cλ: sweep completed `[once: ...]` butler_tasks past the configured grace
/// period. Each deleted task is also recorded into `butler_history.log` as a delete
/// event so the panel timeline / daily summary still reflects the cleanup.
/// Returns the number of tasks deleted.
///
/// Async because butler_history::record_event is async; consolidate's outer
/// `run_consolidation` is already in a tokio context.
pub async fn sweep_completed_once_butler_tasks(
    now: chrono::NaiveDateTime,
    grace_hours: u64,
) -> usize {
    let Ok(index) = memory::memory_list(Some("butler_tasks".to_string())) else {
        return 0;
    };
    let Some(cat) = index.categories.get("butler_tasks") else {
        return 0;
    };
    // Snapshot first so iter / mutation don't race.
    let to_delete: Vec<(String, String)> = cat
        .items
        .iter()
        .filter(|it| {
            crate::proactive::is_completed_once(&it.description, &it.updated_at, now, grace_hours)
        })
        .map(|it| (it.title.clone(), it.description.clone()))
        .collect();
    let mut count = 0;
    for (title, desc) in to_delete {
        if memory::memory_edit(
            "delete".to_string(),
            "butler_tasks".to_string(),
            title.clone(),
            None,
            None,
        )
        .is_ok()
        {
            // Tools-layer memory_edit_impl normally writes butler_history; we go
            // through commands::memory directly so we log manually here, marking
            // the action source so it's clear in the log.
            crate::butler_history::record_event("delete", &title, &desc).await;
            count += 1;
        }
    }
    count
}

/// Iter R17: prune `daily_review_YYYY-MM-DD` entries from `ai_insights`
/// older than `retention_days`. R12 writes one per day; without this they
/// accumulate forever. The pure staleness gate lives in
/// `proactive::daily_review::is_stale_daily_review` and rejects non-review
/// titles (mood / plan / persona_summary stay untouched). `retention_days
/// == 0` disables pruning. Returns the number of entries deleted.
pub fn sweep_stale_daily_reviews(today: chrono::NaiveDate, retention_days: u32) -> usize {
    if retention_days == 0 {
        return 0;
    }
    let Ok(index) = memory::memory_list(Some("ai_insights".to_string())) else {
        return 0;
    };
    let Some(cat) = index.categories.get("ai_insights") else {
        return 0;
    };
    let to_delete: Vec<String> = cat
        .items
        .iter()
        .filter(|it| crate::proactive::is_stale_daily_review(&it.title, today, retention_days))
        .map(|it| it.title.clone())
        .collect();
    let mut count = 0;
    for title in to_delete {
        if memory::memory_edit(
            "delete".to_string(),
            "ai_insights".to_string(),
            title,
            None,
            None,
        )
        .is_ok()
        {
            count += 1;
        }
    }
    count
}

/// Sum item counts across all memory categories.
fn total_memory_items() -> usize {
    match memory::memory_list(None) {
        Ok(idx) => idx.categories.values().map(|c| c.items.len()).sum(),
        Err(_) => 0,
    }
}

/// Build the optional consolidation prompt fragment that points the LLM at the focus
/// history log. Returns an empty string when the file doesn't exist (fresh install / no
/// transitions logged yet / non-macOS) so the prompt stays clean. Otherwise returns a
/// short paragraph with the absolute path, the format, and what to do with it.
fn focus_history_hint() -> String {
    let Some(path) = dirs::config_dir().map(|d| d.join("pet").join("focus_history.log")) else {
        return String::new();
    };
    if !path.exists() {
        return String::new();
    }
    let path_str = path.to_string_lossy();
    format!(
        "**长周期信号**：磁盘上有一份 macOS Focus 模式切换历史 `{path}`，每行一条事件，格式如：\n\
```\n2026-05-02T11:55:00+08:00 on:work\n2026-05-02T12:30:00+08:00 off\n2026-05-02T13:00:00+08:00 switch:personal\n```\n\
建议你用 `read_file` 工具读一下（或用 bash 跑 `tail -n 200`）；如果数据足以总结出长期模式（如\"用户每天工作 focus 平均 N 小时\"、\"周末几乎不开 focus\"），把结论用 `memory_edit create` 或 `update` 写到 `user_profile` 类别下。一条结论性 memory 比一千行原始日志更有用。如果数据太少（< 一周），就先放着。\n\n\
",
        path = path_str
    )
}

/// 周报合成的 IO 包装。门控内部决定要不要跑；命中后读三个 .log + companionship
/// 天数 + 调 aggregator → 写入 `ai_insights/weekly_summary_YYYY-Www`。
///
/// 跨进程幂等：先看进程内 `LAST_WEEKLY_SUMMARY_WEEK`，再读 ai_insights 里
/// 标题是否已存在；只有两道关都"未写过"才会真正落盘。任何 IO 失败均
/// 静默退化 —— best-effort，不影响 consolidate 主流程。
async fn maybe_run_weekly_summary(
    _app: &AppHandle,
    now_local: chrono::DateTime<chrono::Local>,
    closing_hour: u8,
) {
    let now_naive = now_local.naive_local();
    let last = LAST_WEEKLY_SUMMARY_WEEK.lock().ok().and_then(|g| *g);
    let Some(week) = should_trigger_weekly_summary(now_naive, last, closing_hour) else {
        return;
    };
    let title = weekly_summary_title(week);
    if memory::read_ai_insights_item(&title).is_some() {
        // 已写过（前一进程合成的）—— 标记进程内缓存避免接下来的 tick 反复读盘
        if let Ok(mut g) = LAST_WEEKLY_SUMMARY_WEEK.lock() {
            *g = Some(week);
        }
        return;
    }

    let (week_start, week_end) = iso_week_bounds(week);
    let speech_log = crate::speech_history::read_history_content().await;
    let butler_log = crate::butler_history::read_history_content().await;
    let mood_log = crate::mood_history::read_history_content().await;
    let speech_count = aggregate_speech_count(&speech_log, week_start, week_end);
    let butler = aggregate_butler_events(&butler_log, week_start, week_end);
    let mood_top = aggregate_mood_top(&mood_log, week_start, week_end, 3);
    let companionship_days = crate::companionship::companionship_days().await;

    let stats = WeeklyStats {
        week,
        week_start,
        week_end,
        speech_count,
        butler_create: butler.create,
        butler_update: butler.update,
        butler_delete: butler.delete,
        completed_titles: butler.completed_titles,
        mood_top,
        companionship_days,
        completed_with_results: butler.completed_with_results,
        tag_top: butler.tag_top,
    };
    let detail = format_weekly_summary_detail(&stats);
    let description = format_weekly_summary_description(&stats);
    let _ = memory::memory_edit(
        "create".to_string(),
        "ai_insights".to_string(),
        title,
        Some(description),
        Some(detail),
    );
    if let Ok(mut g) = LAST_WEEKLY_SUMMARY_WEEK.lock() {
        *g = Some(week);
    }
}
