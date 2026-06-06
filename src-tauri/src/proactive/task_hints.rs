//! Prompt-hint builders for butler_task / completion / persona / user_profile
//! context. All consumed by `run_proactive_turn` (and a couple by reactive
//! chat / panel) to give the LLM same-shape context across paths.

use super::butler_schedule::{
    format_butler_deadlines_hint, format_butler_tasks_block, parse_butler_deadline_prefix,
    BUTLER_TASKS_HINT_DESC_CHARS, BUTLER_TASKS_HINT_MAX_ITEMS,
};

/// Iter R77: read butler_tasks memory + extract `[deadline:]` prefixed items,
/// format the urgency-tier hint. Distinct from build_butler_tasks_hint
/// which surfaces the full task list — this one is laser-focused on
/// time-urgent deadline reminders. Empty when no deadlines / all Distant.
pub fn build_butler_deadlines_hint(now: chrono::NaiveDateTime) -> String {
    let items: Vec<(chrono::NaiveDateTime, String)> = crate::db::butler_tasks_as_memory_items()
        .iter()
        .filter_map(|i| parse_butler_deadline_prefix(&i.description))
        .collect();
    format_butler_deadlines_hint(&items, now)
}

/// 长任务心跳的 IO 层封装：读 `butler_tasks` → 过滤心跳候选 → 把命中
/// 的标题列表交给 `format_heartbeat_hint`。
///
/// 与 `build_butler_tasks_hint` 互补 —— butler_tasks_hint 是"队列里还有
/// 什么待办"的全景，task_heartbeat_hint 是"哪几条已经动过手却卡住了"的
/// 局部追踪。两个 hint 都注入到 prompt 时，LLM 既能看到完整队列又能看
/// 到必须本轮处理的"心跳点名"。
///
/// `threshold_minutes == 0` 时不做任何 IO，直接返回空串 — 让禁用路径几乎
/// 零成本。失败模式（memory_list 失败 / 类目缺失）静默退化为空串。
pub fn build_task_heartbeat_hint(
    now: chrono::NaiveDateTime,
    threshold_minutes: u32,
) -> String {
    if threshold_minutes == 0 {
        return String::new();
    }
    let titles: Vec<String> = crate::db::butler_tasks_as_memory_items()
        .iter()
        .filter(|i| {
            crate::task_heartbeat::is_heartbeat_candidate(
                &i.description,
                &i.created_at,
                &i.updated_at,
                now,
                threshold_minutes,
            )
        })
        .map(|i| i.title.clone())
        .collect();
    crate::task_heartbeat::format_heartbeat_hint(&titles, threshold_minutes)
}

/// Read butler_tasks memory entries and format the prompt-side digest. `now` is
/// injected so the call site (run_proactive_turn) shares one clock anchor with the
/// rest of the prompt build. Returns "" when the category is empty.
pub fn build_butler_tasks_hint(now: chrono::NaiveDateTime) -> String {
    let tuples: Vec<(String, String, String)> = crate::db::butler_tasks_as_memory_items()
        .into_iter()
        .map(|i| (i.title, i.description, i.updated_at))
        .collect();
    format_butler_tasks_block(
        &tuples,
        BUTLER_TASKS_HINT_MAX_ITEMS,
        BUTLER_TASKS_HINT_DESC_CHARS,
        now,
    )
}

/// 进程内已观察到的「butler_task 处于 done 状态」的标题集合。每次 proactive
/// tick 把当前 done 集合与本静态比对，新增的算「刚转 done」候选；之后用本
/// 次 done 集合**整体替换**静态值（pending → done 是新增；done → pending
/// 是 LLM 罕见地"复活"任务，差集对端不报）。
///
/// 进程重启后默认空 → 第一次 tick 把所有现存 done 都视作 new。这是有意设
/// 计：重启后用户已经看过这些 done（panel 一直可见），但这次的 prompt 提
/// 醒重述一遍并无显著噪音，且能让"启动后第一句"自然带出最近完成情况。
pub static LAST_SEEN_BUTLER_DONE_TITLES: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashSet<String>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashSet::new()));

/// 单条「刚完成」记录给纯格式化函数用。`result` 来自 description 里的
/// `[result: ...]`（None = LLM 没写产物）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletedTaskBrief {
    pub title: String,
    pub result: Option<String>,
}

/// 从 `(title, description)` 列表里挑出当前处于 done 但不在 `prev_seen`
/// 集合里的条目。返回 (新完成项, 当前所有 done 的标题集合)；caller 拿
/// 后者整体覆盖静态。Pure，单测覆盖各转换分支。
pub fn compute_recent_task_completions(
    items: &[(String, String)],
    prev_seen: &std::collections::HashSet<String>,
) -> (Vec<CompletedTaskBrief>, std::collections::HashSet<String>) {
    let mut new_completions: Vec<CompletedTaskBrief> = Vec::new();
    let mut current_done: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (title, desc) in items {
        let (status, _) = crate::task_queue::classify_status(desc);
        if !matches!(status, crate::task_queue::TaskStatus::Done) {
            continue;
        }
        current_done.insert(title.clone());
        if !prev_seen.contains(title) {
            new_completions.push(CompletedTaskBrief {
                title: title.clone(),
                result: crate::task_queue::parse_task_result(desc),
            });
        }
    }
    (new_completions, current_done)
}

/// 「刚完成」hint 单行 / 多行格式化。空列表 → 空串（push_if_nonempty 会
/// 跳过）。> N 条时截断到前 N + "…还有 K 条" 保护 prompt 长度不爆。
pub const TASK_COMPLETION_HINT_MAX_ITEMS: usize = 5;
pub const TASK_COMPLETION_RESULT_CHARS: usize = 80;

pub fn format_task_completion_hint(items: &[CompletedTaskBrief]) -> String {
    if items.is_empty() {
        return String::new();
    }
    let mut lines: Vec<String> = Vec::with_capacity(items.len() + 2);
    lines.push("[任务刚完成] 你之前接手的下面这些 butler_task 现在已标 done，可以挑一个挑一句话向用户简短报喜或确认产物（也可以不提，只是给你一个抓手）：".to_string());
    let take = items.len().min(TASK_COMPLETION_HINT_MAX_ITEMS);
    for brief in items.iter().take(take) {
        let line = match brief.result.as_deref() {
            Some(r) => {
                let r = r.trim();
                let truncated: String = if r.chars().count() <= TASK_COMPLETION_RESULT_CHARS {
                    r.to_string()
                } else {
                    let head: String =
                        r.chars().take(TASK_COMPLETION_RESULT_CHARS).collect();
                    format!("{}…", head)
                };
                format!("· {}（产物：{}）", brief.title.trim(), truncated)
            }
            None => format!("· {}（无产物记录）", brief.title.trim()),
        };
        lines.push(line);
    }
    if items.len() > take {
        lines.push(format!("· …还有 {} 条", items.len() - take));
    }
    lines.join("\n")
}

/// [最近 24h 完成] 全集：rolling window，与 task_completion_hint 单 tick 增
/// 量互补。让 LLM 在 proactive turn 看到 owner / pet 过去一天的 accomplishments
/// 整体景观，能用作"咱昨天搞定的 X 怎么样了 / 前面那个 Y 看起来挺顺手"等连
/// 贯关怀的抓手。
pub const RECENT_COMPLETION_HINT_MAX_ITEMS: usize = 8;
pub const RECENT_COMPLETION_HINT_HOURS: i64 = 24;

pub fn format_recent_completion_hint(items: &[CompletedTaskBrief]) -> String {
    if items.is_empty() {
        return String::new();
    }
    let mut lines: Vec<String> = Vec::with_capacity(items.len() + 2);
    lines.push("[最近 24h 完成] 你和用户在过去一天里完成了下面这些 butler_task —— 可以用作连贯关怀的抓手（如「咱昨天搞定的 X 怎么样了？」/「前面那个 Y 看起来挺顺手」等），但别每条都点名：".to_string());
    let take = items.len().min(RECENT_COMPLETION_HINT_MAX_ITEMS);
    for brief in items.iter().take(take) {
        let line = match brief.result.as_deref() {
            Some(r) => {
                let r = r.trim();
                let truncated: String = if r.chars().count() <= TASK_COMPLETION_RESULT_CHARS {
                    r.to_string()
                } else {
                    let head: String =
                        r.chars().take(TASK_COMPLETION_RESULT_CHARS).collect();
                    format!("{}…", head)
                };
                format!("· {}（产物：{}）", brief.title.trim(), truncated)
            }
            None => format!("· {}", brief.title.trim()),
        };
        lines.push(line);
    }
    if items.len() > take {
        lines.push(format!("· …还有 {} 条", items.len() - take));
    }
    lines.join("\n")
}

/// pure：从 `butler_tasks_as_memory_items` 类输入扫 done 且 updated_at 落在
/// `now - HOURS` 与 now 之间的条目，按 updated_at 倒序输出 CompletedTaskBrief
/// 列表。`updated_at` 解析失败 / 非 Done status 跳过。IO 由 caller 注入便于
/// 单测（不依赖 wall clock / butler_tasks 后端状态）。
pub fn compute_recent_completions(
    items: &[(String, String, String)],
    now: chrono::NaiveDateTime,
) -> Vec<CompletedTaskBrief> {
    let cutoff = now - chrono::Duration::hours(RECENT_COMPLETION_HINT_HOURS);
    let mut tuples: Vec<(chrono::NaiveDateTime, CompletedTaskBrief)> = Vec::new();
    for (title, desc, updated_at) in items {
        let (status, _) = crate::task_queue::classify_status(desc);
        if status != crate::task_queue::TaskStatus::Done {
            continue;
        }
        // Try parse two common formats: with milliseconds (chrono::Local 输出)
        // or without。Tauri save_session / memory_edit 都走 chrono Local 格式
        // 但兜底两形态防偶发数据形变。
        let updated = match chrono::NaiveDateTime::parse_from_str(
            updated_at,
            "%Y-%m-%dT%H:%M:%S%.f",
        ) {
            Ok(t) => t,
            Err(_) => match chrono::NaiveDateTime::parse_from_str(
                updated_at,
                "%Y-%m-%dT%H:%M:%S",
            ) {
                Ok(t) => t,
                Err(_) => continue,
            },
        };
        if updated < cutoff || updated > now {
            // 跳过：> 24h 前的（不在窗口） + 未来时间戳（数据 corrupt 防御）
            continue;
        }
        let result = crate::task_queue::parse_task_result(desc);
        tuples.push((
            updated,
            CompletedTaskBrief {
                title: title.clone(),
                result,
            },
        ));
    }
    // 最近完成在前（descending by updated_at）
    tuples.sort_by(|a, b| b.0.cmp(&a.0));
    tuples.into_iter().map(|(_, b)| b).collect()
}

/// IO 包装：读 butler_tasks → 走 compute_recent_completions → format。
pub fn build_recent_completion_hint(now: chrono::NaiveDateTime) -> String {
    let tuples: Vec<(String, String, String)> = crate::db::butler_tasks_as_memory_items()
        .into_iter()
        .map(|i| (i.title, i.description, i.updated_at))
        .collect();
    let recent = compute_recent_completions(&tuples, now);
    format_recent_completion_hint(&recent)
}

/// IO 包装：读 `butler_tasks` → 找 done 转换 → 更新静态 → 走纯 formatter。
/// 失败模式（memory_list 失败 / 类目缺失 / 无 done）静默退化为空串，与其
/// 它 hint 一致。
pub fn build_task_completion_hint() -> String {
    let pairs: Vec<(String, String)> = crate::db::butler_tasks_as_memory_items()
        .into_iter()
        .map(|i| (i.title, i.description))
        .collect();
    let prev_seen = match LAST_SEEN_BUTLER_DONE_TITLES.lock() {
        Ok(g) => g.clone(),
        Err(_) => std::collections::HashSet::new(),
    };
    let (new_completions, current_done) = compute_recent_task_completions(&pairs, &prev_seen);
    if let Ok(mut g) = LAST_SEEN_BUTLER_DONE_TITLES.lock() {
        *g = current_done;
    }
    format_task_completion_hint(&new_completions)
}

/// Iter D5: serializable shape for `get_persona_summary` — text + last-updated
/// timestamp so the Persona panel can show "X days ago" freshness. `text` is
/// empty when no summary exists yet; `updated_at` is the ISO-8601 from the
/// memory entry, also empty in that case.
#[derive(serde::Serialize)]
pub struct PersonaSummary {
    pub text: String,
    pub updated_at: String,
}

/// Tauri command returning the raw persona-summary description (Iter 105) — without
/// the "你最近一次自我反思的画像（来自 consolidate）：" header `build_persona_hint`
/// adds. The Persona panel surfaces this directly so users can read what the pet
/// has written about itself. Iter D5: now returns text + updated_at so the panel
/// can display freshness ("X 天前更新").
#[tauri::command]
pub fn get_persona_summary() -> PersonaSummary {
    crate::commands::memory::read_ai_insights_item("persona_summary")
        .map(|i| PersonaSummary {
            text: i.description.trim().to_string(),
            updated_at: i.updated_at,
        })
        .unwrap_or_else(|| PersonaSummary {
            text: String::new(),
            updated_at: String::new(),
        })
}

/// Read the pet's self-authored persona summary from `ai_insights/persona_summary`.
/// Iter 102: this is what the consolidate loop generates by reflecting on recent
/// speech_history + user_profile. Returns the description verbatim with a header line,
/// or empty when no summary has been written yet (fresh installs / not enough signal).
///
/// `pub` since Iter 104 — reactive chat reuses this to inject the same persona layer
/// into its system prompt, so the long-term identity isn't proactive-only.
pub fn build_persona_hint() -> String {
    let Some(item) = crate::commands::memory::read_ai_insights_item("persona_summary") else {
        return String::new();
    };
    if item.description.trim().is_empty() {
        return String::new();
    }
    // Iter Cw: redact the persona summary before re-injecting into the
    // proactive prompt. The LLM-authored description may have echoed private
    // terms (active_window app names / user_profile entries it didn't know
    // were sensitive when it wrote them); redacting here ensures the same
    // user-configured patterns cover this self-loop input too. The on-disk
    // memory file stays pristine — the panel's `get_persona_summary` command
    // intentionally returns the unredacted text since that view is local.
    format!(
        "你最近一次自我反思的画像（来自 consolidate）：\n{}",
        item.description.trim()
    )
}

/// Cap on how many `user_profile` entries to surface in the proactive prompt. Above
/// this the digest gets long enough to dominate the prompt and dilute its other
/// signals; the LLM can still call `memory_search` for the older ones if a topic
/// asks for them.
pub const USER_PROFILE_HINT_MAX_ITEMS: usize = 6;
/// Per-entry description char cap. Long bios become noisy when stacked 6 deep, so
/// the prompt sees a one-liner per habit; the full body is one tool call away.
pub const USER_PROFILE_HINT_DESC_CHARS: usize = 80;

/// Pure helper — formats a list of `(title, description, updated_at)` tuples into
/// the user-profile prompt block. Sorted by `updated_at` descending so the most
/// recently-touched habits surface first. Returns "" when items is empty so the
/// prompt builder's `push_if_nonempty` skips the line cleanly.
///
/// Extracted from `build_user_profile_hint` so the truncation / sort / header logic
/// is unit-testable without going through `memory_list`'s on-disk index.
pub fn format_user_profile_block(
    items: &[(String, String, String)],
    max_items: usize,
    max_desc_chars: usize,
) -> String {
    if items.is_empty() || max_items == 0 {
        return String::new();
    }
    let mut sorted: Vec<&(String, String, String)> = items.iter().collect();
    // updated_at is ISO-8601 with offset → string compare matches chronological
    // order; descending = most recent first.
    sorted.sort_by(|a, b| b.2.cmp(&a.2));
    let n = sorted.len().min(max_items);
    let mut lines: Vec<String> = Vec::with_capacity(n + 1);
    lines.push(format!(
        "你了解的用户习惯（来自 user_profile 记忆，最新 {} 条）：",
        n
    ));
    for (title, desc, _) in sorted.iter().take(n) {
        let trimmed = desc.trim();
        let truncated: String = if trimmed.chars().count() <= max_desc_chars {
            trimmed.to_string()
        } else {
            let head: String = trimmed.chars().take(max_desc_chars).collect();
            format!("{}…", head)
        };
        lines.push(format!("- {}：{}", title.trim(), truncated));
    }
    lines.join("\n")
}

/// Read `user_profile` memory entries and format a compact digest block for the
/// proactive prompt (Iter Cα). Returns empty when the category has no entries —
/// `push_if_nonempty` then skips it cleanly.
pub fn build_user_profile_hint() -> String {
    let Ok(index) = crate::commands::memory::memory_list(Some("user_profile".to_string())) else {
        return String::new();
    };
    let Some(cat) = index.categories.get("user_profile") else {
        return String::new();
    };
    let tuples: Vec<(String, String, String)> = cat
        .items
        .iter()
        .map(|i| (i.title.clone(), i.description.clone(), i.updated_at.clone()))
        .collect();
    format_user_profile_block(
        &tuples,
        USER_PROFILE_HINT_MAX_ITEMS,
        USER_PROFILE_HINT_DESC_CHARS,
    )
}
