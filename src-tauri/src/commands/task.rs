//! 任务队列的 Tauri 命令层。把 `task_queue` 模块的纯函数包成 IO：
//! - `task_create`：拼装 `[task pri=N due=ISO]` header → `memory_edit("create",
//!   "butler_tasks", ...)`，返回新建条目的 detail_path。
//! - `task_list`：读 `butler_tasks` → 解析每条 → 排序后返回 TaskView 列表。
//!
//! 设计选择记录：
//! - **不暴露独立的"任务存储"**：butler_tasks 内存类目已经是真相源，旁挂
//!   一份 SQLite / JSON 只会带来"两边数据不一致"的运维负担。
//! - **task_create 直接调 memory_edit**：保留 `butler_history` 的事件流（创建
//!   会被记到 butler_history.log），与 LLM 自己 memory_edit 的路径同源。
//! - **task_list 不做缓存**：butler_tasks 本身就是个小列表（典型 < 30 条），
//!   YAML 解析快且总在内存里走；为这个加 LRU 是过早优化。

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

use crate::commands::memory;
use crate::decision_log::DecisionLogStore;
use crate::task_queue::{
    append_cancelled_marker, classify_status, compare_for_queue,
    format_task_description, parse_task_header, parse_task_result, parse_task_tags,
    strip_error_markers, strip_origin_marker, strip_result_marker, TaskHeader, TaskStatus,
    TaskView, TASK_PRIORITY_MAX,
};

/// `task_create` 的入参集合。Tauri 要求顶级字段为可序列化的简单类型，
/// 把它打包成结构体便于前端 invoke 时传一个对象，少几个 camelCase 烦扰。
#[derive(Debug, Deserialize)]
pub struct TaskCreateArgs {
    /// 标题。必填、非空。会被原样写到 `butler_tasks.title`，所以重名会触发
    /// `memory_edit` 内的 unique-filename 兜底（自动加 `_1` 等后缀）。
    pub title: String,
    /// 任务正文。可选；面板上的多行描述。
    #[serde(default)]
    pub body: String,
    /// 优先级 0..=9。越界返回 Err，UI 应在表单层先做 clamp。
    pub priority: u8,
    /// 可选 ISO 时间，形如 `2026-05-05T18:00`（datetime-local 默认输出）。
    #[serde(default)]
    pub due: Option<String>,
}

/// 仅用于序列化返回值。前端拿到的就是 `TaskView` 的列表 + 一个轻封装。
#[derive(Debug, Serialize)]
pub struct TaskListResponse {
    pub tasks: Vec<TaskView>,
}

#[tauri::command]
pub fn task_create(args: TaskCreateArgs) -> Result<String, String> {
    let title = args.title.trim();
    if title.is_empty() {
        return Err("title is required".to_string());
    }
    if args.priority > TASK_PRIORITY_MAX {
        return Err(format!(
            "priority must be 0..={} (got {})",
            TASK_PRIORITY_MAX, args.priority
        ));
    }
    let due_parsed = match args.due.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => Some(
            NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M")
                .map_err(|e| format!("invalid due (expect YYYY-MM-DDThh:mm): {}", e))?,
        ),
        None => None,
    };

    let header = TaskHeader {
        priority: args.priority,
        due: due_parsed,
        body: args.body.trim().to_string(),
    };
    let description = format_task_description(&header);

    memory::memory_edit(
        "create".to_string(),
        "butler_tasks".to_string(),
        title.to_string(),
        Some(description),
        // detail_content：默认空字符串。LLM 执行任务时会用 memory_edit
        // update 写最新进度 / 产物路径。这里不预填内容，避免和 LLM 的自由
        // 写作冲突。
        Some(String::new()),
    )
}

#[tauri::command]
pub fn task_list() -> Result<TaskListResponse, String> {
    // v5a：read path 切到 SQLite。v3 startup_backfill 在 setup hook 同步
    // 跑过，调用此命令时 db 一定已与 yaml 对齐。description 文本完全一致，
    // build_task_view 派生的 TaskView（status / priority / due / tags
    // 等）与之前 yaml 路径产物相同。yaml 仍由 memory_edit / mirror 双写
    // 保 source-of-truth 一致（v6 才删 yaml）。
    let rows = crate::db::with_db(crate::db::butler_tasks_list)?;
    let mut views: Vec<TaskView> = rows
        .iter()
        .map(|r| {
            let item = r.to_memory_item();
            build_task_view(&item)
        })
        .collect();

    let now = chrono::Local::now().naive_local();
    views.sort_by(|a, b| compare_for_queue(a, b, now));
    Ok(TaskListResponse { tasks: views })
}

/// `task_retry`：把 status==Error 的任务恢复为 Pending。剥掉描述里
/// 所有 `[error: ...]` / `[done...]` 标记；调 `memory_edit("update", ...)`
/// 写回，`updated_at` 自动前进 → 心跳计数也重置；推决策日志。
///
/// 校验：找不到任务 / 任务非 Error 状态 → 返回 Err，让前端给出明确反馈而
/// 非静默 noop。
#[tauri::command]
pub fn task_retry(
    title: String,
    decisions: tauri::State<'_, DecisionLogStore>,
) -> Result<(), String> {
    task_retry_inner(title, decisions.inner().clone())
}

/// 实际执行 retry。Tauri 命令是它的薄包装；需要从非-Tauri-invoke 路径
/// 触发的调用方（如 Telegram bot 的 /retry 命令）直接调本函数即可，
/// 把 DecisionLogStore 通过 `app.state::<...>().inner().clone()` 拿到
/// 后传入。
pub fn task_retry_inner(title: String, decisions: DecisionLogStore) -> Result<(), String> {
    let title_trimmed = title.trim();
    if title_trimmed.is_empty() {
        return Err("title is required".to_string());
    }
    let item = find_butler_task(title_trimmed)
        .ok_or_else(|| format!("task not found: {}", title_trimmed))?;
    let (status, _) = classify_status(&item.description);
    if status != TaskStatus::Error {
        return Err(format!(
            "only error tasks can be retried (current status: {:?})",
            status
        ));
    }
    let cleaned = strip_error_markers(&item.description);
    memory::memory_edit(
        "update".to_string(),
        "butler_tasks".to_string(),
        item.title.clone(),
        Some(cleaned),
        None, // 不重写 detail.md — 保留 LLM 之前写过的进度笔记
    )?;
    decisions.push("TaskRetry", item.title.clone());
    Ok(())
}

/// `task_cancel`：在 description 末尾追加 `[cancelled: <reason>]`。
/// 终态操作 —— 已 Done / Cancelled 的任务不能再取消（用 Err 拒绝，避免
/// 把 cancelled 标记重复追加污染 description）。Error / Pending 都可以
/// 取消，cancelled 优先级覆盖此前状态。
#[tauri::command]
pub fn task_cancel(
    title: String,
    reason: String,
    decisions: tauri::State<'_, DecisionLogStore>,
) -> Result<(), String> {
    task_cancel_inner(title, reason, decisions.inner().clone())
}

/// 实际执行 cancel。Tauri 命令是它的薄包装；TG 的 /cancel 命令直接调
/// 本函数，原因留空（TG 单行命令不收原因；如果想给原因让用户去面板）。
pub fn task_cancel_inner(
    title: String,
    reason: String,
    decisions: DecisionLogStore,
) -> Result<(), String> {
    let title_trimmed = title.trim();
    if title_trimmed.is_empty() {
        return Err("title is required".to_string());
    }
    let item = find_butler_task(title_trimmed)
        .ok_or_else(|| format!("task not found: {}", title_trimmed))?;
    let (status, _) = classify_status(&item.description);
    if matches!(status, TaskStatus::Done | TaskStatus::Cancelled) {
        return Err(format!(
            "task already finished (status: {:?})",
            status
        ));
    }
    let new_desc = append_cancelled_marker(&item.description, &reason);
    memory::memory_edit(
        "update".to_string(),
        "butler_tasks".to_string(),
        item.title.clone(),
        Some(new_desc),
        None,
    )?;
    let reason_trim = reason.trim();
    let log_msg = if reason_trim.is_empty() {
        item.title.clone()
    } else {
        format!("{} — {}", item.title, reason_trim)
    };
    decisions.push("TaskCancel", log_msg);
    Ok(())
}

/// `task_unarchive`：把 task_archive 里某条恢复到 butler_tasks。剥 [archived:]
/// / [done] / [cancelled:] / [error:] / [result:] 标记让任务回到 pending；脱
/// 掉 `YYYY-MM-DD_` 前缀让 title 回原始形态；通过 memory_edit create 加到
/// butler_tasks（SQLite mirror 自动同步），随后 memory_edit delete 清归档条
/// 目。原 detail.md 在 task_archive/ 下保留不动（避免回滚 + 老笔记仍能翻
/// 出来），新 butler_tasks 起一份空 detail.md。
///
/// 失败模式：归档条目不存在 → Err；新 title 与现有 butler_tasks 冲突 →
/// memory_edit 不会拒（yaml 允许重名），SQLite mirror 的 UNIQUE 索引会拒
/// 第二条但属 best-effort 容忍。
#[tauri::command]
pub fn task_unarchive(title: String) -> Result<String, String> {
    let archive_title = title.trim().to_string();
    if archive_title.is_empty() {
        return Err("title is required".to_string());
    }
    // 1. 从 SQLite task_archive 取条目
    let archived = crate::db::with_db(|c| crate::db::task_archive_get(c, &archive_title))
        .map_err(|e| format!("db read failed: {e}"))?
        .ok_or_else(|| format!("archive item not found: {archive_title}"))?;

    // 2. 剥 archive / 终态 marker，拿到 pending 形态 description
    let new_description = crate::task_queue::strip_archive_markers(&archived.description);

    // 3. 脱 `YYYY-MM-DD_` 前缀；regex 不引入，直接 split_once
    //    形如 "2026-04-01_整理 downloads" → "整理 downloads"
    let new_title = archive_title
        .splitn(2, '_')
        .nth(1)
        .filter(|rest| {
            let prefix_len = archive_title.len() - rest.len() - 1;
            archive_title.get(..prefix_len).is_some_and(|p| {
                // 严格检查前缀是 YYYY-MM-DD 格式（10 字符）
                p.len() == 10 && p.chars().filter(|&c| c == '-').count() == 2
            })
        })
        .map(String::from)
        .unwrap_or(archive_title.clone());

    // 4. 创建到 butler_tasks
    memory::memory_edit(
        "create".to_string(),
        "butler_tasks".to_string(),
        new_title.clone(),
        Some(new_description),
        None,
    )?;

    // 5. 删除原 archive 条目
    memory::memory_edit(
        "delete".to_string(),
        "task_archive".to_string(),
        archive_title,
        None,
        None,
    )?;

    Ok(format!("Restored as butler_task: {new_title}"))
}

/// `task_mark_done`：在 description 末尾追加 `[done]`（可选 `[result: ...]`）
/// 标记。`result` 为 `None` / 空 / 仅空白 → 不附 result，与按 d 键盘快捷键
/// 路径等价；非空 → 附 `[result: <trim>]`，与 LLM 自动 mark done 时形态
/// 一致。已是 done / cancelled 的任务拒绝，避免重复追加 marker 污染 description。
#[tauri::command]
pub fn task_mark_done(
    title: String,
    result: Option<String>,
    decisions: tauri::State<'_, DecisionLogStore>,
) -> Result<(), String> {
    task_mark_done_inner(title, result, decisions.inner().clone())
}

pub fn task_mark_done_inner(
    title: String,
    result: Option<String>,
    decisions: DecisionLogStore,
) -> Result<(), String> {
    let title_trimmed = title.trim();
    if title_trimmed.is_empty() {
        return Err("title is required".to_string());
    }
    let item = find_butler_task(title_trimmed)
        .ok_or_else(|| format!("task not found: {}", title_trimmed))?;
    let (status, _) = classify_status(&item.description);
    if matches!(status, TaskStatus::Done | TaskStatus::Cancelled) {
        return Err(format!(
            "task already finished (status: {:?})",
            status
        ));
    }
    let new_desc = crate::task_queue::append_done_marker_with_result(
        &item.description,
        result.as_deref(),
    );
    memory::memory_edit(
        "update".to_string(),
        "butler_tasks".to_string(),
        item.title.clone(),
        Some(new_desc),
        None,
    )?;
    decisions.push("TaskMarkDone", item.title.clone());
    Ok(())
}

/// `task_clone`：一键克隆 butler_task。新 task 标题 = `${源} (副本)`（若
/// 已占用 → `(副本 2)` / `(副本 3)` ... 至 9 都占用时 Err 让 owner 先重命
/// 名旧 clones）。新 description 走 strip_for_clone 剥所有终态 / 一次性
/// marker（done / result / error / cancelled / archived / snooze），保留
/// task header / schedule / tag / pinned / silent / blockedBy / reminderMin
/// — owner 想克的是 task spec 不是状态历史。detail.md 内容一并拷贝。
/// decision_log push "TaskClone" 给 audit。返新 title 让前端 toast 显。
#[tauri::command]
pub fn task_clone(
    title: String,
    decisions: tauri::State<'_, DecisionLogStore>,
) -> Result<String, String> {
    task_clone_inner(title, decisions.inner().clone())
}

pub fn task_clone_inner(
    title: String,
    decisions: DecisionLogStore,
) -> Result<String, String> {
    let title_trimmed = title.trim();
    if title_trimmed.is_empty() {
        return Err("title is required".to_string());
    }
    let item = find_butler_task(title_trimmed)
        .ok_or_else(|| format!("task not found: {}", title_trimmed))?;
    // 找 unique 新标题：(副本) → (副本 2) → ... → (副本 9)。9 个仍占用时
    // Err 让 owner 先 rename 旧的，避免 (副本 100) 这种丑陋积累。
    let new_title = {
        let base = format!("{} (副本)", item.title);
        if find_butler_task(&base).is_none() {
            base
        } else {
            let mut found: Option<String> = None;
            for n in 2..=9 {
                let candidate = format!("{} (副本 {})", item.title, n);
                if find_butler_task(&candidate).is_none() {
                    found = Some(candidate);
                    break;
                }
            }
            found.ok_or_else(|| {
                format!(
                    "task '{}' 已有 9 个克隆副本；先重命名 / 删掉旧的再克隆",
                    item.title
                )
            })?
        }
    };
    // 读源 detail.md 内容（IO 失败兜空字符串：仍能创建新 task，只是 detail
    // 是空白让 owner 重写）
    let detail_md = crate::commands::memory::memory_read_detail_full(
        item.detail_path.clone(),
    )
    .unwrap_or_default();
    // strip 终态 + snooze marker 后写入新 task
    let cleaned_desc = crate::task_queue::strip_for_clone(&item.description);
    memory::memory_edit(
        "create".to_string(),
        "butler_tasks".to_string(),
        new_title.clone(),
        Some(cleaned_desc),
        Some(detail_md),
    )?;
    decisions.push("TaskClone", new_title.clone());
    Ok(new_title)
}

/// `task_skip_once`：让 owner 跳过本轮 due 的 butler_task — 把 description
/// 原样 write 回 memory_edit，触发 updated_at 自动刷到 now。对 `every: HH:MM`
/// 任务效果是：mostRecentFire 仍是今日 HH:MM，但 last_updated 现在 > fire →
/// isButlerDue 返 false → 本轮跳过；下一轮（明日 HH:MM）仍按 schedule 触发。
///
/// 不动 description / schedule / tags / markers — 仅刷时间戳。也不重写
/// detail.md。decision_log push "TaskSkipOnce" 给 audit。
///
/// 校验：title 空 / 找不到 / 终态（done / cancelled）拒绝；error 状态不拒，
/// 让 owner 在 LLM 报错后也能"跳本轮回头再说"。
#[tauri::command]
pub fn task_skip_once(
    title: String,
    decisions: tauri::State<'_, DecisionLogStore>,
) -> Result<(), String> {
    task_skip_once_inner(title, decisions.inner().clone())
}

pub fn task_skip_once_inner(
    title: String,
    decisions: DecisionLogStore,
) -> Result<(), String> {
    rewrite_description_to_bump_updated(title, decisions, "TaskSkipOnce")
}

/// `task_touch_inner`：与 task_skip_once 同机制（重 write 同 description
/// → memory_edit 自动 stamp updated_at），但语义是「让老 task 重新冒
/// 头 proactive 选单」（非 schedule-bounded task 也可用 — owner 想让
/// pet 重新关注一条挂了很久的 active task）。decision_log 标
/// "TaskTouch" 做 audit 区分。done / cancelled 拒绝（终态任务 touch
/// 无意义 — 不会回到 proactive 选单）。
pub fn task_touch_inner(
    title: String,
    decisions: DecisionLogStore,
) -> Result<(), String> {
    rewrite_description_to_bump_updated(title, decisions, "TaskTouch")
}

/// 内部 helper：核心机制是 rewrite 同 description 让 memory_edit 自
/// 动 bump updated_at。caller 注入 decision_log label 区分意图。
fn rewrite_description_to_bump_updated(
    title: String,
    decisions: DecisionLogStore,
    decision_label: &str,
) -> Result<(), String> {
    let title_trimmed = title.trim();
    if title_trimmed.is_empty() {
        return Err("title is required".to_string());
    }
    let item = find_butler_task(title_trimmed)
        .ok_or_else(|| format!("task not found: {}", title_trimmed))?;
    let (status, _) = classify_status(&item.description);
    if matches!(status, TaskStatus::Done | TaskStatus::Cancelled) {
        return Err(format!(
            "cannot {} a finished task (current status: {:?})",
            decision_label.to_lowercase(),
            status
        ));
    }
    memory::memory_edit(
        "update".to_string(),
        "butler_tasks".to_string(),
        item.title.clone(),
        Some(item.description.clone()),
        None,
    )?;
    decisions.push(decision_label, item.title.clone());
    Ok(())
}

/// `task_undo_done`：把 status == Done 的 task 还原为 Pending — 剥 description
/// 里的 `[done]` / `[result: ...]` marker，写回 butler_tasks。owner 误标 done
/// 撤销用：与 `task_mark_done` 对偶（同 marker，反向操作）。
///
/// 校验：找不到任务 / 任务非 Done 状态 → 返回 Err（让前端给明确反馈而非
/// 静默 noop）。被剥的 marker 仅限本 task 的"完成" — 保留 schedule / tag /
/// pinned / silent / snooze / blockedBy / detail.md 等 owner-intent 状态。
/// 不重写 detail.md — 保留 done 时 LLM 可能写的 [result: ...] 在 detail 段
/// 的扩展笔记（与 `task_retry` 同保留语义）。
#[tauri::command]
pub fn task_undo_done(
    title: String,
    decisions: tauri::State<'_, DecisionLogStore>,
) -> Result<(), String> {
    task_undo_done_inner(title, decisions.inner().clone())
}

pub fn task_undo_done_inner(
    title: String,
    decisions: DecisionLogStore,
) -> Result<(), String> {
    let title_trimmed = title.trim();
    if title_trimmed.is_empty() {
        return Err("title is required".to_string());
    }
    let item = find_butler_task(title_trimmed)
        .ok_or_else(|| format!("task not found: {}", title_trimmed))?;
    let (status, _) = classify_status(&item.description);
    if status != TaskStatus::Done {
        return Err(format!(
            "only done tasks can be undone (current status: {:?})",
            status
        ));
    }
    let cleaned = crate::task_queue::strip_done_markers(&item.description);
    memory::memory_edit(
        "update".to_string(),
        "butler_tasks".to_string(),
        item.title.clone(),
        Some(cleaned),
        None,
    )?;
    decisions.push("TaskUndoDone", item.title.clone());
    Ok(())
}

/// 给「任务」标签头部红点徽章用：返回当前已过期未完成（pending / error 且
/// `due < now`）的任务数。
///
/// 不计 `done` / `cancelled` —— 终态任务不需要"催促"信号；不计无 due 任务
/// （没截止就谈不上"过期"）。状态优先级与面板渲染一致：cancelled / done
/// 即便有 due 已过，也不入计数。
///
/// 实现走 `count_overdue` 纯函数，IO 仅一次 memory_list。
#[tauri::command]
pub fn task_overdue_count() -> u64 {
    let items = crate::db::butler_tasks_as_memory_items();
    let now = chrono::Local::now().naive_local();
    count_overdue(&items, now)
}

/// Pure：从 butler_tasks `MemoryItem` 列表中数 pending / error 且 due 已到
/// 或超过的条数。`now` 由调用方注入（生产传 `chrono::Local::now()
/// .naive_local()`，测试传固定时刻），便于单测无依赖。
///
/// 边界：
/// - 无 task header → 跳过（无 due 概念）
/// - 解析 due 失败 / 缺失 → 跳过
/// - status ∉ {Pending, Error} → 跳过
/// - `due <= now` → 计入（与 `compare_for_queue` 的 overdue 判断一致：
///   边界即时刻准时也算过期，避免抖动）
pub(crate) fn count_overdue(items: &[memory::MemoryItem], now: chrono::NaiveDateTime) -> u64 {
    use crate::task_queue::{classify_status, parse_task_header, TaskStatus};
    items
        .iter()
        .filter(|item| {
            let Some(h) = parse_task_header(&item.description) else {
                return false;
            };
            let Some(due) = h.due else {
                return false;
            };
            if due > now {
                return false;
            }
            let (status, _) = classify_status(&item.description);
            matches!(status, TaskStatus::Pending | TaskStatus::Error)
        })
        .count() as u64
}

/// `task_save_detail`：把任务的 detail.md 文件内容覆盖为 `content`。给「任务
/// 详情页」的编辑入口用 —— 用户在面板里写 / 改进度笔记不必去 memories 目录。
///
/// 走 `memory_edit("update", ...)`：description=None 保留 yaml 原描述（priority
/// / due / 其它 markers 不动），`detail_content=Some(content)` 覆盖 detail.md
/// 文件内容。空 content 视作"清空进度笔记"，与 LLM 写空文件等价。
///
/// 不做长度上限 —— detail.md 是用户自管笔记，硬限不利。memory_edit 内部已
/// 处理路径解析与 IO 错误。
#[tauri::command]
pub fn task_save_detail(title: String, content: String) -> Result<(), String> {
    let title_trim = title.trim();
    if title_trim.is_empty() {
        return Err("title is required".to_string());
    }
    let item = find_butler_task(title_trim)
        .ok_or_else(|| format!("task not found: {}", title_trim))?;
    memory::memory_edit(
        "update".to_string(),
        "butler_tasks".to_string(),
        item.title.clone(),
        None,
        Some(content),
    )?;
    Ok(())
}

/// `task_history_sparklines`：给 PanelTasks 行内「📊 sparkline」chip 批量
/// 算每个 task 近 30 天的 update 频率桶分布（10 bar，oldest → newest）。
/// 一次性读 butler_history.log + 扫一遍按 title 聚合，避免行内 N 个 task
/// 各发一次 IO 拖慢面板。结果 = Map<title, [u32; 10]>。
///
/// titles 入参 = 当前面板正在显示的 task title 列表 — 让 backend 仅算
/// owner 看得见的（archive 段不显时不算 archive task 节省 CPU）。
/// 空 titles → 空 map。读不到 history.log（NotFound / IO 错）→ 空 map
/// （前端 chip 自然不渲；与既有 task_get_detail history IO error 兜底
/// 同语义 — 历史信号 best-effort 不阻塞主流程）。
#[tauri::command]
pub async fn task_history_sparklines(
    titles: Vec<String>,
) -> Result<std::collections::HashMap<String, Vec<u32>>, String> {
    if titles.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    let content = crate::butler_history::read_history_content_strict()
        .await
        .unwrap_or_default();
    let now = chrono::Local::now().fixed_offset();
    Ok(crate::butler_history::compute_sparkline_buckets(
        &content, &titles, now,
    ))
}

/// `task_history_24h_hourly`：PanelTasks 顶部「📈 24h 事件 sparkline」
/// chip 用。扫 butler_history.log 按近 24 小时 hourly bucket 聚合所有
/// title 的事件计数（global view，与 per-title `task_history_sparklines`
/// 互补 — 那个看「单 task 30 天节奏」，本命令看「今天 24h 全局活跃曲
/// 线」）。返 24 个 u32，oldest → newest。
///
/// 读不到 history.log（NotFound / IO 错）→ 全零 vec（前端 chip 自然
/// 不渲；与 sparklines 同 best-effort 兜底）。
#[tauri::command]
pub async fn task_history_24h_hourly() -> Result<Vec<u32>, String> {
    let content = crate::butler_history::read_history_content_strict()
        .await
        .unwrap_or_default();
    let now = chrono::Local::now().fixed_offset();
    Ok(crate::butler_history::compute_hourly_buckets_24h(
        &content, now,
    ))
}

/// `task_detail_history`：列出指定任务 detail.md 的最近 N 个版本快照（最新在
/// 前）。给「任务详情页」的「📜 历史」chip 用 —— owner 想拿回上一版时一键
/// 列出 + 选 ts 复制内容。任务不存在 → Err；history 目录不存在 / 空 → Ok([])。
#[tauri::command]
pub fn task_detail_history(
    title: String,
) -> Result<Vec<crate::detail_history::DetailHistoryEntry>, String> {
    let title_trim = title.trim();
    if title_trim.is_empty() {
        return Err("title is required".to_string());
    }
    let item = find_butler_task(title_trim)
        .ok_or_else(|| format!("task not found: {}", title_trim))?;
    let mem_dir = memory::memories_dir()?;
    let full_path = mem_dir.join(&item.detail_path);
    Ok(crate::detail_history::list_history(&full_path))
}

/// `task_reveal_history_dir`：在 Finder / Explorer 打开指定任务 detail.md
/// 对应的 `.history` 目录。给「📜」popover「📁 Finder 打开 .history dir」
/// 按钮用 —— owner cherry-pick 历史 / 备份导出 / 自己 diff 时不必复制
/// 路径再去开。任务不存在 → Err；history 目录不存在（任务从未 save 过
/// 或 cap 被清光）→ Err 含友好文案。
#[tauri::command]
pub fn task_reveal_history_dir(title: String) -> Result<(), String> {
    let title_trim = title.trim();
    if title_trim.is_empty() {
        return Err("title is required".to_string());
    }
    let item = find_butler_task(title_trim)
        .ok_or_else(|| format!("task not found: {}", title_trim))?;
    let mem_dir = memory::memories_dir()?;
    let full_path = mem_dir.join(&item.detail_path);
    let history_dir = crate::detail_history::history_dir_for(&full_path);
    if !history_dir.exists() {
        return Err("尚无历史快照（save 过 detail.md 后才会有 .history dir）".to_string());
    }
    let canon = std::fs::canonicalize(&history_dir).map_err(|e| {
        format!("Failed to resolve history dir: {}", e)
    })?;
    let mem_canon = std::fs::canonicalize(&mem_dir).map_err(|e| {
        format!("Failed to resolve memories_dir: {}", e)
    })?;
    if !canon.starts_with(&mem_canon) {
        return Err("history dir escaped memories_dir".to_string());
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&canon)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("Failed to open via `open`: {}", e))
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&canon)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("Failed to open via `explorer`: {}", e))
    }
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        std::process::Command::new("xdg-open")
            .arg(&canon)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("Failed to open via `xdg-open`: {}", e))
    }
}

/// `task_set_due`：把任务的截止时间改成新值（或清空）。与 `task_set_priority`
/// 完全对偶 —— 保留 priority / body / 其它 markers 不动，只动 due 字段。
///
/// `due == None` 或 trim 后为空 → 写出无 `due=` 的 header（任务变成"无截止"）。
/// 否则按 `YYYY-MM-DDThh:mm`（datetime-local 协议）严格解析；失败返回 Err。
///
/// 兼容 legacy 无 header 任务：parse 失败时构造 `{ priority: 0, due, body:
/// trim(desc) }`（与 `task_set_priority` 兼容路径一致，把 description 提升到带
/// header 的标准形式）。
///
/// 不推 decision_log —— 与改优先级同：日常 UX 调整，非状态转移。
#[tauri::command]
pub fn task_set_due(title: String, due: Option<String>) -> Result<(), String> {
    let title_trim = title.trim();
    if title_trim.is_empty() {
        return Err("title is required".to_string());
    }
    let due_parsed = match due.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => Some(
            NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M")
                .map_err(|e| format!("invalid due (expect YYYY-MM-DDThh:mm): {}", e))?,
        ),
        None => None,
    };
    let item = find_butler_task(title_trim)
        .ok_or_else(|| format!("task not found: {}", title_trim))?;
    let new_header = match parse_task_header(&item.description) {
        Some(h) => TaskHeader {
            priority: h.priority,
            due: due_parsed,
            body: h.body,
        },
        None => TaskHeader {
            priority: 0,
            due: due_parsed,
            body: item.description.trim().to_string(),
        },
    };
    let new_desc = format_task_description(&new_header);
    memory::memory_edit(
        "update".to_string(),
        "butler_tasks".to_string(),
        item.title.clone(),
        Some(new_desc),
        None,
    )?;
    Ok(())
}

/// `task_set_pinned`：写 / 撤销任务 description 的 `[pinned]` marker。owner 在
/// 面板点 📌 切换；pinned=true 时 append `[pinned]`，false 时 strip 所有 marker。
/// 与 `task_set_snooze` 同模式 —— 单 bool 字段原子修改、保留其它 markers 不动。
///
/// 不推 decision_log —— 与 due / snooze 同：owner 标注偏好，非状态转移。
#[tauri::command]
pub fn task_set_pinned(title: String, pinned: bool) -> Result<(), String> {
    let title_trim = title.trim();
    if title_trim.is_empty() {
        return Err("title is required".to_string());
    }
    let item = find_butler_task(title_trim)
        .ok_or_else(|| format!("task not found: {}", title_trim))?;
    let stripped = crate::task_queue::strip_pinned_markers(&item.description);
    let new_desc = if pinned {
        let base = stripped.trim_end();
        if base.is_empty() {
            "[pinned]".to_string()
        } else {
            format!("{} [pinned]", base)
        }
    } else {
        stripped
    };
    memory::memory_edit(
        "update".to_string(),
        "butler_tasks".to_string(),
        item.title.clone(),
        Some(new_desc),
        None,
    )?;
    Ok(())
}

/// `task_set_silent`：写 / 撤销任务 description 的 `[silent]` marker。owner 在
/// 面板点 🔇 切换；silent=true 时 append `[silent]`，false 时 strip 所有
/// marker。与 `task_set_pinned` 同模式 —— 单 bool 字段原子修改、保留其它
/// markers 不动。`[silent]` 在 `format_butler_tasks_block` 被过滤，使该
/// task 不进 LLM proactive 主动 pick 队列。
///
/// 不推 decision_log —— 与 pinned / due / snooze 同：owner 标注偏好，非状态转移。
#[tauri::command]
pub fn task_set_silent(title: String, silent: bool) -> Result<(), String> {
    let title_trim = title.trim();
    if title_trim.is_empty() {
        return Err("title is required".to_string());
    }
    let item = find_butler_task(title_trim)
        .ok_or_else(|| format!("task not found: {}", title_trim))?;
    let stripped = crate::task_queue::strip_silent_markers(&item.description);
    let new_desc = if silent {
        let base = stripped.trim_end();
        if base.is_empty() {
            "[silent]".to_string()
        } else {
            format!("{} [silent]", base)
        }
    } else {
        stripped
    };
    memory::memory_edit(
        "update".to_string(),
        "butler_tasks".to_string(),
        item.title.clone(),
        Some(new_desc),
        None,
    )?;
    Ok(())
}

/// `task_set_snooze`：写 / 撤销任务 description 的 `[snooze: ...]` marker。
/// 与 `task_set_due` 同模式 —— 单字段原子修改、保留其它 markers 不动。
///
/// `until == None` 或 trim 后为空 → 调 `strip_snooze_markers` 清掉所有
/// 既有 `[snooze:]` marker（"撤销暂停"语义）。
/// 否则两步解析（先预设后严格）：
///   1. 先尝试 `parse_snooze_token` 预设短串（EN: tonight / tomorrow /
///      monday / Nm / Nh，CJK: 今晚 / 明早 / 明天 / 下周一 / 周一 / N分 /
///      N小时）；命中 → 用 `compute_snooze_until(now)` 解析到绝对时刻
///   2. 再 fall back 到严格 `YYYY-MM-DD HH:MM`
/// 两路径都失败 → Err（提示同时列预设 + 绝对格式让用户知道两种都行）。
/// 写入时先 strip 旧 marker 再 append 新 marker，保证 description 整洁。
///
/// 不推 decision_log —— 与 due / priority 同：日常 UX 调整，非状态转移。
#[tauri::command]
pub fn task_set_snooze(title: String, until: Option<String>) -> Result<(), String> {
    let title_trim = title.trim();
    if title_trim.is_empty() {
        return Err("title is required".to_string());
    }
    let parsed_until = match until.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => {
            // 先试预设短串：tonight / tomorrow / monday / 今晚 / 明早 / 30m / 2h / 30分 / 2小时
            if let Some(spec) = crate::telegram::commands::parse_snooze_token(s) {
                let now = chrono::Local::now().naive_local();
                let abs = crate::telegram::commands::compute_snooze_until(spec, now);
                Some(abs.format("%Y-%m-%d %H:%M").to_string())
            } else if NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").is_ok() {
                Some(s.to_string())
            } else {
                return Err(format!(
                    "invalid snooze input {:?} —— 预设支持：tonight / tomorrow / monday / 今晚 / 明早 / 明天 / 下周一 / 30m / 2h / 30分 / 2小时；或绝对格式 YYYY-MM-DD HH:MM",
                    s
                ));
            }
        }
        None => None,
    };
    let item = find_butler_task(title_trim)
        .ok_or_else(|| format!("task not found: {}", title_trim))?;
    let stripped = crate::task_queue::strip_snooze_markers(&item.description);
    let new_desc = match parsed_until {
        Some(s) => {
            let base = stripped.trim_end();
            if base.is_empty() {
                format!("[snooze: {}]", s)
            } else {
                format!("{} [snooze: {}]", base, s)
            }
        }
        None => stripped,
    };
    memory::memory_edit(
        "update".to_string(),
        "butler_tasks".to_string(),
        item.title.clone(),
        Some(new_desc),
        None,
    )?;
    Ok(())
}

/// `task_set_priority`：把任务的 priority 改成新值，保留 due / body / 其它
/// markers 不动。批量改优先级走每条 invoke 一次，对小规模选择（5-10）的 IO
/// 开销可忽略。
///
/// 兼容 legacy 无 header 任务：parse 失败时直接构造 header `{ priority,
/// due: None, body: description.trim() }` —— 即便用户在改优先级时也顺带把
/// description "提升"为带 header 的标准格式（无副作用，所有现有 marker 是
/// 写在 body 里的，trim 不动它们）。
///
/// 不推 decision log —— 改优先级是日常 UX 调整（不像 cancel/retry 是状态
/// 转移），进 log 是噪音。
#[tauri::command]
pub fn task_set_priority(title: String, priority: u8) -> Result<(), String> {
    let title_trim = title.trim();
    if title_trim.is_empty() {
        return Err("title is required".to_string());
    }
    if priority > TASK_PRIORITY_MAX {
        return Err(format!(
            "priority must be 0..={} (got {})",
            TASK_PRIORITY_MAX, priority
        ));
    }
    let item = find_butler_task(title_trim)
        .ok_or_else(|| format!("task not found: {}", title_trim))?;
    let new_header = match parse_task_header(&item.description) {
        Some(h) => TaskHeader {
            priority,
            due: h.due,
            body: h.body,
        },
        None => TaskHeader {
            priority,
            due: None,
            body: item.description.trim().to_string(),
        },
    };
    let new_desc = format_task_description(&new_header);
    memory::memory_edit(
        "update".to_string(),
        "butler_tasks".to_string(),
        item.title.clone(),
        Some(new_desc),
        None,
    )?;
    Ok(())
}

/// `task_set_tags`：批量改 tag。`ops_input` 是用户输入的 `+a -b +工作` 单
/// 行 —— 解析层做校验（互斥 / 缺前缀 / 非法字符），通过后对 description
/// 应用 add/remove。bulk 模式下每条选中任务都调一次本命令；解析在外层只
/// 跑一次的话每条都得参数化传 `Vec<TagOp>`，给前端 invoke 接口加复杂度
/// 而省 N-1 次 trivial 解析，权衡选 N 次。
///
/// 不推 decision log —— 改 tag 是日常组织调整（同 priority / due），进
/// log 是噪音。
#[tauri::command]
pub fn task_set_tags(title: String, ops_input: String) -> Result<(), String> {
    let title_trim = title.trim();
    if title_trim.is_empty() {
        return Err("title is required".to_string());
    }
    let ops = crate::task_queue::parse_tag_ops(&ops_input)?;
    let item = find_butler_task(title_trim)
        .ok_or_else(|| format!("task not found: {}", title_trim))?;
    let new_desc = crate::task_queue::apply_tag_ops(&item.description, &ops);
    if new_desc == item.description {
        return Ok(()); // 全部 noop（add 已存在 / remove 不存在），不写盘
    }
    memory::memory_edit(
        "update".to_string(),
        "butler_tasks".to_string(),
        item.title.clone(),
        Some(new_desc),
        None,
    )?;
    Ok(())
}

/// 在 butler_tasks 表里按 title 匹配返回一条。v5b：read 路径切到 SQLite —
/// title 在表里是 UNIQUE 索引，O(1) 查询比 yaml 全扫快。失败 / 不存在
/// 返 None。caller 拿 MemoryItem 形态（status / tags 由 caller 重新从
/// description 解析，与 yaml 路径同算法）。
///
/// 历史背景：yaml 路径下 memory_edit 对重名标题有 `_1` 后缀机制，
/// 所以 title 字段本身仍可重复 —— SQLite 切了 UNIQUE 索引后这种重复
/// 会被 backfill 时 INSERT 冲突拒绝。当前用户数据若已含重名，backfill
/// 会跳过第二条（v2 的 idempotent skip 逻辑），不会因此 panic。
fn find_butler_task(title: &str) -> Option<memory::MemoryItem> {
    crate::db::with_db(|conn| crate::db::butler_task_get(conn, title))
        .ok()
        .flatten()
        .map(|row| row.to_memory_item())
}

/// 任务详情页的 payload。`raw_description` 故意保留 `[task pri=...]` 等所有
/// markers —— 用户回溯单条任务全过程时希望看到的就是"宠物在 yaml 里到底
/// 看到了什么"，而不是被 strip 过的展示 body。
#[derive(Debug, Serialize)]
pub struct TaskDetail {
    pub title: String,
    pub raw_description: String,
    pub detail_path: String,
    pub detail_md: String,
    pub created_at: String,
    pub updated_at: String,
    pub history: Vec<TaskHistoryEvent>,
    /// detail.md 读失败标志（NotFound 不算，仅 permission denied / corrupt 等
    /// 真正 IO 错误置 true）。前端渲染红字提示让用户区分"真没数据"和"读失败"。
    pub detail_md_io_error: bool,
    /// butler_history.log 读失败标志（NotFound / 路径解析失败不算）。
    pub history_io_error: bool,
}

/// 时间线条目。timestamp 与 butler_history 行原始 ts 同形（RFC3339 + 时区）；
/// snippet 是 ` :: ` 之后的部分（已被 80-char 截断）。
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct TaskHistoryEvent {
    pub timestamp: String,
    pub action: String,
    pub snippet: String,
}

/// `task_get_detail`：给「任务详情」页一次性拿齐三段数据。
///
/// 三个数据源全部 best-effort：
/// - 找不到任务 → Err（前端回退到"已被删除？"提示）
/// - detail.md 读不到（文件不存在 / IO 失败）→ 空串
/// - butler_history.log 读不到 → 空 history
///
/// 这一原则保证：detail.md 缺失或日志被轮转切掉**不会**让整个详情页
/// 加载失败。"任务存在 + 描述可见"是详情页最低承诺。
#[tauri::command]
pub async fn task_get_detail(title: String) -> Result<TaskDetail, String> {
    let title_trim = title.trim();
    if title_trim.is_empty() {
        return Err("title is required".to_string());
    }
    let item = find_butler_task(title_trim)
        .ok_or_else(|| format!("task not found: {}", title_trim))?;

    // 读 detail.md（同步 fs，与 memory_edit 那边一致；文件很小）。NotFound
    // 视作"还没生成"非错误（detail.md 起始即可为空）；其它 IO 错误置 io_error
    // 让前端能渲染红字 hint 让用户去排查。
    let (detail_md, detail_md_io_error) = match memory::memories_dir() {
        Ok(dir) => {
            let full = dir.join(&item.detail_path);
            match std::fs::read_to_string(&full) {
                Ok(s) => (s, false),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => (String::new(), false),
                Err(_) => (String::new(), true),
            }
        }
        Err(_) => (String::new(), true),
    };

    // butler_history 全文（async）+ 过滤；strict 版本区分 NotFound（视作空）
    // 与其它 IO 错误（io_error=true）。
    let (history_content, history_io_error) =
        match crate::butler_history::read_history_content_strict().await {
            Ok(s) => (s, false),
            Err(_) => (String::new(), true),
        };
    let history: Vec<TaskHistoryEvent> =
        crate::butler_history::filter_history_for_task(&history_content, &item.title)
            .into_iter()
            .map(|(timestamp, action, snippet)| TaskHistoryEvent {
                timestamp,
                action,
                snippet,
            })
            .collect();

    Ok(TaskDetail {
        title: item.title,
        raw_description: item.description,
        detail_path: item.detail_path,
        detail_md,
        created_at: item.created_at,
        updated_at: item.updated_at,
        history,
        detail_md_io_error,
        history_io_error,
    })
}

/// 把一条 `MemoryItem` 转成 `TaskView`：解析 task header 拿 priority / due /
/// body；解析 status；保留时间戳。无 task header 的历史条目走"无 header
/// 兼容路径"——`priority = 0, due = None, body = description.trim()`。
pub(crate) fn build_task_view(item: &memory::MemoryItem) -> TaskView {
    let raw = item.description.as_str();
    let (priority, due_str, body_raw) = match parse_task_header(raw) {
        Some(h) => (
            h.priority,
            h.due.map(|d| d.format("%Y-%m-%dT%H:%M").to_string()),
            h.body,
        ),
        None => (0u8, None, raw.trim().to_string()),
    };
    // 面板里隐藏 [origin:...] / [result:...] / [pinned] 标记 — origin 是
    // routing 协议、result 已在 result 字段单独展示、pinned 由 TaskView.pinned
    // bool 暴露。tag 不剥（让用户在 body 里也看得到 #xxx 词）。
    let body = crate::task_queue::strip_pinned_markers(&strip_result_marker(
        &strip_origin_marker(&body_raw),
    ));
    let (status, error_message) = classify_status(raw);
    let tags = parse_task_tags(raw);
    let result = parse_task_result(raw);
    let blocked_by = crate::task_queue::parse_blocked_by(raw);
    let pinned = crate::task_queue::parse_pinned(raw);
    // snoozed_until：仅当 snooze 时刻仍在未来时填字符串；过点后视作 None
    // 让前端 chip 自动消失（marker 自然失效，不需要 cleanup）。
    let snoozed_until = crate::task_queue::parse_snooze(raw).and_then(|until| {
        let now = chrono::Local::now().naive_local();
        if until > now {
            Some(until.format("%Y-%m-%dT%H:%M").to_string())
        } else {
            None
        }
    });
    TaskView {
        title: item.title.clone(),
        body,
        raw_description: raw.to_string(),
        priority,
        due: due_str,
        status,
        error_message,
        tags,
        result,
        created_at: item.created_at.clone(),
        updated_at: item.updated_at.clone(),
        detail_path: item.detail_path.clone(),
        blocked_by,
        snoozed_until,
        pinned,
    }
}

/// `regenerate_task_title`：让 LLM 看任务 title + 描述 + detail.md 前 600 字，
/// 给出 ≤ 10 字的中文新标题，并 atomic 调 `memory_rename` 写回。与
/// `regenerate_session_title` 同 IO 模板（非流式 / 30s timeout / temperature
/// 0.3 / max_tokens 30 / 输出清洗）。
///
/// 设计：
/// - **不走 chat_pipeline**：本调用是"总结任务"工具，宠物自我画像 / 工具用法
///   等 layer 注入只会污染 prompt。bare-bones context + 一条指令。
/// - **detail.md best-effort 600 字**：长 detail 截断；读失败 / 空文件不阻塞
///   —— 仍能基于 title + description 给标题。
/// - **rename 失败但 LLM 成功时返 Err**：用户没看到新名也没改名，对称错误。
///   memory_rename 的"new == old" / "已存在重名" 等错误透传给前端。
/// - **不推 decision_log**：与 due / priority / pinned 等"UX 操作"同非状态转移。
#[tauri::command]
pub async fn regenerate_task_title(title: String) -> Result<String, String> {
    let settings = crate::commands::settings::get_settings()?;
    if settings.api_key.is_empty() {
        return Err("API Key 未配置。打开「设置」填好后再试。".to_string());
    }
    if settings.model.trim().is_empty() {
        return Err("model 未配置。".to_string());
    }
    let title_trim = title.trim().to_string();
    if title_trim.is_empty() {
        return Err("title is required".to_string());
    }
    let item = find_butler_task(&title_trim)
        .ok_or_else(|| format!("task not found: {}", title_trim))?;
    // 拼任务上下文：title + description + (可选) detail.md 前 600 字。
    let mut context = String::new();
    context.push_str("原标题：");
    context.push_str(&item.title);
    context.push_str("\n描述：");
    context.push_str(item.description.trim());
    if !item.detail_path.is_empty() {
        if let Ok(detail) =
            crate::commands::memory::memory_read_detail(item.detail_path.clone())
        {
            let trimmed: String = detail.chars().take(600).collect();
            if !trimmed.trim().is_empty() {
                context.push_str("\n详情（前 600 字）：");
                context.push_str(&trimmed);
            }
        }
    }
    let messages = vec![serde_json::json!({
        "role": "user",
        "content": format!(
            "{}\n\n请用 ≤ 10 字的中文给这条任务起一个更切题 / 更易识别的新标题，仅输出标题文本，不要带引号 / 句号 / 表情。",
            context
        ),
    })];
    let url = format!(
        "{}/chat/completions",
        settings.api_base.trim_end_matches('/')
    );
    let body = serde_json::json!({
        "model": settings.model,
        "messages": messages,
        "max_tokens": 30,
        "stream": false,
        "temperature": 0.3,
    });
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("HTTP client init failed: {e}"))?;
    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", settings.api_key))
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("请求 chat API 失败：{e}"))?;
    let status = resp.status();
    let raw = resp
        .text()
        .await
        .map_err(|e| format!("读取响应体失败：{e}"))?;
    if !status.is_success() {
        return Err(format!("chat API 返回 {}：{}", status, raw));
    }
    let parsed: serde_json::Value = serde_json::from_str(&raw).map_err(|e| {
        format!(
            "解析响应失败：{e}；原始 body 前 200 字：{}",
            &raw.chars().take(200).collect::<String>()
        )
    })?;
    let raw_content = parsed["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .trim();
    // 清洗：剥首尾引号 / 句号；夹 \n 用 " " 替换避免多行标题。
    let stripped: String = raw_content
        .trim_matches(|c: char| {
            matches!(c, '"' | '\'' | '“' | '”' | '‘' | '’' | '.' | '。')
        })
        .replace('\n', " ")
        .trim()
        .to_string();
    if stripped.is_empty() {
        return Err("LLM 返回了空标题。".to_string());
    }
    let new_title: String = stripped.chars().take(30).collect();
    // 不变（noop）/ 重名（rename Err）等都直接透传，让前端 toast 给原因。
    crate::commands::memory::memory_rename(
        "butler_tasks".to_string(),
        item.title.clone(),
        new_title.clone(),
    )?;
    Ok(new_title)
}


#[cfg(test)]
#[path = "task_tests.rs"]
mod tests;
