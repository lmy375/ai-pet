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
    append_cancelled_marker, classify_status, compare_for_queue, format_task_description,
    parse_task_header, parse_task_result, parse_task_tags, strip_error_markers,
    strip_origin_marker, strip_result_marker, TaskHeader, TaskStatus, TaskView,
    TASK_PRIORITY_MAX,
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
    let index = memory::memory_list(Some("butler_tasks".to_string()))?;
    let cat = match index.categories.get("butler_tasks") {
        Some(c) => c,
        None => {
            return Ok(TaskListResponse { tasks: vec![] });
        }
    };

    let mut views: Vec<TaskView> = cat
        .items
        .iter()
        .map(|item| build_task_view(item))
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
    let Ok(index) = memory::memory_list(Some("butler_tasks".to_string())) else {
        return 0;
    };
    let Some(cat) = index.categories.get("butler_tasks") else {
        return 0;
    };
    let now = chrono::Local::now().naive_local();
    count_overdue(&cat.items, now)
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

/// 在 butler_tasks 类目里按 title 匹配返回最早一条。命中失败返回 None；
/// 历史上 memory_edit 对重名标题有 `_1` 后缀机制，所以理论上重名是 detail
/// 文件层的差异化，title 字段本身仍可重复。这里取首条与"最早创建"对齐。
fn find_butler_task(title: &str) -> Option<memory::MemoryItem> {
    let index = memory::memory_list(Some("butler_tasks".to_string())).ok()?;
    let cat = index.categories.get("butler_tasks")?;
    cat.items.iter().find(|i| i.title == title).cloned()
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
    // 面板里隐藏 [origin:...] / [result:...] 标记 — 前者是 routing 协议，
    // 后者已在 result 字段独立展示，body 里再有就是重复。tag 不剥（让用户
    // 在 body 里也看得到 #xxx 词）。
    let body = strip_result_marker(&strip_origin_marker(&body_raw));
    let (status, error_message) = classify_status(raw);
    let tags = parse_task_tags(raw);
    let result = parse_task_result(raw);
    TaskView {
        title: item.title.clone(),
        body,
        priority,
        due: due_str,
        status,
        error_message,
        tags,
        result,
        created_at: item.created_at.clone(),
        updated_at: item.updated_at.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::memory::MemoryItem;
    use crate::task_queue::TaskStatus;

    fn item(title: &str, desc: &str, created_at: &str) -> MemoryItem {
        MemoryItem {
            title: title.to_string(),
            description: desc.to_string(),
            detail_path: String::new(),
            created_at: created_at.to_string(),
            updated_at: created_at.to_string(),
        }
    }

    #[test]
    fn build_view_extracts_header_fields() {
        let v = build_task_view(&item(
            "归档下载",
            "[task pri=3 due=2026-05-05T18:00] 把图片按月份归档",
            "2026-05-04T13:00:00+08:00",
        ));
        assert_eq!(v.priority, 3);
        assert_eq!(v.due.as_deref(), Some("2026-05-05T18:00"));
        assert_eq!(v.body, "把图片按月份归档");
        assert_eq!(v.status, TaskStatus::Pending);
    }

    #[test]
    fn build_view_falls_back_for_legacy_entries() {
        // 历史条目没有 task header，但带 [once:] 等老前缀 — 也能展示，
        // 只是 priority 默认 0、due None。这是兼容性保证，不能丢老数据。
        let v = build_task_view(&item(
            "晚上跑步",
            "[once: 2026-05-04T19:00] 跑 30 分钟",
            "2026-05-04T13:00:00+08:00",
        ));
        assert_eq!(v.priority, 0);
        assert_eq!(v.due, None);
        assert_eq!(v.body, "[once: 2026-05-04T19:00] 跑 30 分钟");
        assert_eq!(v.status, TaskStatus::Pending);
    }

    #[test]
    fn build_view_collects_tags_and_result() {
        let v = build_task_view(&item(
            "整理 Downloads",
            "[task pri=2] 把图片归档 #organize #weekly [done] [result: 归档 38 个文件到 ~/Archive/]",
            "2026-05-04T13:00:00+08:00",
        ));
        assert_eq!(v.tags, vec!["organize", "weekly"]);
        assert_eq!(v.result.as_deref(), Some("归档 38 个文件到 ~/Archive/"));
        assert_eq!(v.status, TaskStatus::Done);
        // body 不应该包含 [result:...] 段（已在 result 字段独立展示）
        assert!(!v.body.contains("[result"));
        // body 应保留 #tag 让用户在描述里也看到
        assert!(v.body.contains("#organize"));
    }

    #[test]
    fn build_view_handles_missing_tags_and_result() {
        let v = build_task_view(&item(
            "x",
            "[task pri=1] 普通任务",
            "2026-05-04T13:00:00+08:00",
        ));
        assert!(v.tags.is_empty());
        assert!(v.result.is_none());
    }

    #[test]
    fn build_view_hides_origin_marker_from_displayed_body() {
        // origin 标记是宠物侧 routing 协议，面板上的 body 不应该带它
        let v = build_task_view(&item(
            "整理 Downloads",
            "[task pri=2] 把图片归档 [origin:tg:12345]",
            "2026-05-04T13:00:00+08:00",
        ));
        assert_eq!(v.body, "把图片归档");
    }

    #[test]
    fn build_view_picks_up_error_status_with_message() {
        let v = build_task_view(&item(
            "整理 Downloads",
            "[task pri=2] [error: 路径找不到] 复查",
            "2026-05-04T13:00:00+08:00",
        ));
        assert_eq!(v.status, TaskStatus::Error);
        assert_eq!(v.error_message.as_deref(), Some("路径找不到"));
    }

    // ---------------- task_set_priority validation ----------------

    #[test]
    fn set_priority_rejects_empty_title() {
        // 校验路径不依赖 memory IO —— 只测早退出分支
        let r = task_set_priority(String::new(), 3);
        assert!(r.is_err());
        let r = task_set_priority("   ".to_string(), 3);
        assert!(r.is_err());
    }

    #[test]
    fn set_priority_rejects_out_of_range() {
        let r = task_set_priority("any".to_string(), 10);
        assert!(r.is_err());
        let err = r.unwrap_err();
        assert!(err.contains("priority must be 0..=9"));
    }

    // ---------------- task_set_due validation ----------------

    #[test]
    fn set_due_rejects_empty_title() {
        // 空 title 应在 IO 之前早退；不依赖 memory mock
        assert!(task_set_due(String::new(), Some("2026-05-05T18:00".to_string())).is_err());
        assert!(task_set_due("   ".to_string(), None).is_err());
    }

    #[test]
    fn set_due_rejects_invalid_format() {
        // 任意非空 title + 不符合 datetime-local 协议的 due 也应早退
        let r = task_set_due("any".to_string(), Some("2026/05/05 18:00".to_string()));
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("invalid due"));
    }

    // ---------------- task_save_detail validation ----------------

    #[test]
    fn save_detail_rejects_empty_title() {
        // 空 title 早退于 IO 之前，不需 memory mock
        assert!(task_save_detail(String::new(), "any content".to_string()).is_err());
        assert!(task_save_detail("   ".to_string(), String::new()).is_err());
    }

    // ---------------- count_overdue ----------------

    fn now() -> chrono::NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(2026, 5, 5)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap()
    }

    #[test]
    fn count_overdue_returns_zero_for_empty() {
        assert_eq!(count_overdue(&[], now()), 0);
    }

    #[test]
    fn count_overdue_includes_pending_with_passed_due() {
        let items = vec![item(
            "整理 Downloads",
            "[task pri=2 due=2026-05-04T18:00] 整理",
            "2026-05-04T13:00:00+08:00",
        )];
        assert_eq!(count_overdue(&items, now()), 1);
    }

    #[test]
    fn count_overdue_includes_error_with_passed_due() {
        let items = vec![item(
            "跑步",
            "[task pri=1 due=2026-05-04T18:00] [error: 下雨] 跑步",
            "2026-05-04T13:00:00+08:00",
        )];
        assert_eq!(count_overdue(&items, now()), 1);
    }

    #[test]
    fn count_overdue_excludes_done_and_cancelled() {
        // 终态任务即便 due 已过也不入计数 —— 用户已经 move on
        let items = vec![
            item(
                "已做完",
                "[task pri=2 due=2026-05-04T18:00] 整理 [done]",
                "2026-05-04T13:00:00+08:00",
            ),
            item(
                "已取消",
                "[task pri=2 due=2026-05-04T18:00] 整理 [cancelled: 不做了]",
                "2026-05-04T13:00:00+08:00",
            ),
        ];
        assert_eq!(count_overdue(&items, now()), 0);
    }

    #[test]
    fn count_overdue_excludes_future_due() {
        let items = vec![item(
            "未来任务",
            "[task pri=1 due=2026-05-06T18:00] 还没到",
            "2026-05-04T13:00:00+08:00",
        )];
        assert_eq!(count_overdue(&items, now()), 0);
    }

    #[test]
    fn count_overdue_excludes_no_due() {
        // 无 due 字段 — "过期"概念不适用
        let items = vec![item(
            "无截止",
            "[task pri=3] 闲时做",
            "2026-05-04T13:00:00+08:00",
        )];
        assert_eq!(count_overdue(&items, now()), 0);
    }

    #[test]
    fn count_overdue_excludes_legacy_no_header() {
        // 旧格式（无 [task pri=...] header）→ parse 失败 → 视作无 due → 不计入
        let items = vec![item(
            "旧任务",
            "[once: 2026-05-04T18:00] 跑步",
            "2026-05-04T13:00:00+08:00",
        )];
        assert_eq!(count_overdue(&items, now()), 0);
    }

    #[test]
    fn count_overdue_treats_due_eq_now_as_overdue() {
        // 边界一致性：`due == now` 与 compare_for_queue 都判 overdue
        let items = vec![item(
            "刚好到点",
            "[task pri=2 due=2026-05-05T12:00] 该开始了",
            "2026-05-04T13:00:00+08:00",
        )];
        assert_eq!(count_overdue(&items, now()), 1);
    }

    #[test]
    fn count_overdue_aggregates_multiple() {
        let items = vec![
            item("过期1", "[task pri=2 due=2026-05-04T10:00] a", "2026-05-04T09:00:00+08:00"),
            item("过期2", "[task pri=1 due=2026-05-05T11:30] [error: x] b", "2026-05-04T09:00:00+08:00"),
            item("未过期", "[task pri=3 due=2026-05-06T10:00] c", "2026-05-04T09:00:00+08:00"),
            item("已完成", "[task pri=1 due=2026-05-04T08:00] d [done]", "2026-05-04T09:00:00+08:00"),
            item("无 due", "[task pri=2] e", "2026-05-04T09:00:00+08:00"),
        ];
        assert_eq!(count_overdue(&items, now()), 2);
    }
}
