//! `task_create` 工具：与 `propose_task` 互补 —— 不弹卡片，**直接**把
//! 任务写进 `butler_tasks` 内存类目。给无 UI 入口（Telegram、未来的
//! webhook 等）使用：用户在那些通道里写下"帮我…"的瞬间已经是确认本身，
//! 不需要再过一层确认卡。
//!
//! 与桌面 panel 路径的关系：
//! - **桌面**：propose_task → 卡片 → 用户点「创建」→ Tauri 命令 task_create
//! - **Telegram / webhook**：本工具 task_create → 直接 memory_edit
//!
//! 两条路径最终都落到同一个 `butler_tasks.YAML` 上，区别只是"是否有
//! 用户二次确认"。LLM 通过工具描述 + surface-specific system layer
//! 知道在哪个通道走哪条。
//!
//! `origin` 参数让任务带 `[origin:tg:<chat_id>]` 标记 —— 完成 / 失败
//! 时 TG bot 的 watcher 据此把通知回传到原会话。

use chrono::NaiveDateTime;
use serde::Deserialize;

use crate::commands::memory;
use crate::task_queue::{
    append_origin_marker, format_task_description, TaskHeader, TaskOrigin, TASK_PRIORITY_MAX,
};
use crate::tools::{Tool, ToolContext};

pub struct TaskCreateTool;

#[derive(Debug, Deserialize)]
struct TaskCreateArgs {
    title: String,
    #[serde(default)]
    body: String,
    priority: u8,
    /// `YYYY-MM-DDThh:mm`（无时区，本地时间）。空字符串与缺失同义。
    #[serde(default)]
    due: Option<String>,
    /// 任务来源标记，形如 `tg:<chat_id>`。空 / 缺失 → 不打标。完成 / 失
    /// 败时只有带标记的任务会被 TG watcher 通知回传。
    #[serde(default)]
    origin: Option<String>,
}

impl Tool for TaskCreateTool {
    fn name(&self) -> &str {
        "task_create"
    }

    fn definition(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "task_create",
                "description": "Directly create a butler_tasks entry without user confirmation. Use ONLY in surfaces that lack a confirmation UI (currently: Telegram). For desktop panel chat use `propose_task` instead — desktop has a confirmation card, this tool would bypass it.\n\nThe surface-specific system layer (e.g. `[Telegram dispatch]`) tells you which tool to use; default to `propose_task` if no such layer is injected.\n\nWHEN TO CALL (Telegram only):\n- 用户在 TG 用自然语言把一件具体事委托给你（「帮我…」「记得…」「这周末…」），且当前消息已是确认本身。\n- 例：「帮我整理 ~/Downloads 里的旧图片」/「记得明天 6 点提醒我交报告」。\n\nALWAYS pass `origin=\"tg:<chat_id>\"` when called from Telegram (the system layer provides chat_id) — without this, the task can't notify back when finished.\n\nAFTER CALLING: 在自然语言回复里简短承接（「好的，加到队列里了，做完会回这里告诉你」），让用户知道有任务真的入队。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "title": {
                            "type": "string",
                            "description": "短标题，10-30 字符。从用户原话里抽取核心动词 + 对象。"
                        },
                        "body": {
                            "type": "string",
                            "description": "（可选）展开描述，把限定条件写清楚。"
                        },
                        "priority": {
                            "type": "integer",
                            "minimum": 0,
                            "maximum": 9,
                            "description": "0-9 优先级。日常 1-3；紧迫 5-7；最高优先 8-9。"
                        },
                        "due": {
                            "type": "string",
                            "description": "（可选）截止时间，`YYYY-MM-DDThh:mm`，本地时区。"
                        },
                        "origin": {
                            "type": "string",
                            "description": "（Telegram 路径必填）来源标记，形如 `tg:<chat_id>`。完成/失败时通知会回传到该 chat。"
                        }
                    },
                    "required": ["title", "priority"]
                }
            }
        })
    }

    fn execute<'a>(
        &'a self,
        arguments: &'a str,
        ctx: &'a ToolContext,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = String> + Send + 'a>> {
        Box::pin(task_create_impl(arguments, ctx))
    }
}

async fn task_create_impl(arguments: &str, ctx: &ToolContext) -> String {
    let args: TaskCreateArgs = match serde_json::from_str(arguments) {
        Ok(a) => a,
        Err(e) => {
            ctx.log(&format!("task_create: bad args: {}", e));
            return format!(r#"{{"error": "invalid arguments: {}"}}"#, e);
        }
    };
    match build_description(&args) {
        Ok(description) => {
            let title = args.title.trim().to_string();
            match memory::memory_edit(
                "create".to_string(),
                "butler_tasks".to_string(),
                title.clone(),
                Some(description),
                Some(String::new()),
            ) {
                Ok(detail_path) => {
                    ctx.log(&format!(
                        "task_create: enqueued '{}' (origin={})",
                        title.replace(['\n', '\r'], " "),
                        args.origin.as_deref().unwrap_or("-"),
                    ));
                    format!(
                        r#"{{"created": true, "title": {:?}, "detail_path": {:?}}}"#,
                        title, detail_path
                    )
                }
                Err(e) => {
                    ctx.log(&format!("task_create: memory_edit failed: {}", e));
                    format!(r#"{{"error": "create failed: {}"}}"#, e.replace('"', "\\\""))
                }
            }
        }
        Err(msg) => {
            ctx.log(&format!("task_create: rejected: {}", msg));
            format!(r#"{{"error": "{}"}}"#, msg.replace('"', "\\\""))
        }
    }
}

/// 纯函数：把入参拼成最终 description。所有校验在这里集中，便于单测。
fn build_description(args: &TaskCreateArgs) -> Result<String, String> {
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
    let mut description = format_task_description(&header);
    if let Some(origin) = parse_origin_arg(args.origin.as_deref())? {
        description = append_origin_marker(&description, &origin);
    }
    Ok(description)
}

/// 解析 origin 参数。当前只支持 `"tg:<chat_id>"`；空 / None → Ok(None)；
/// 不识别格式 → Err，让 LLM 看到错误并修正。
fn parse_origin_arg(s: Option<&str>) -> Result<Option<TaskOrigin>, String> {
    let Some(raw) = s.map(str::trim).filter(|x| !x.is_empty()) else {
        return Ok(None);
    };
    if let Some(id_str) = raw.strip_prefix("tg:") {
        let id: i64 = id_str.trim().parse().map_err(|_| {
            format!("invalid origin (expect 'tg:<chat_id>'): {}", raw)
        })?;
        return Ok(Some(TaskOrigin::Tg(id)));
    }
    Err(format!("unknown origin format: {}", raw))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task_queue::{classify_status, parse_task_origin, TaskStatus};

    fn args(
        title: &str,
        body: &str,
        priority: u8,
        due: Option<&str>,
        origin: Option<&str>,
    ) -> TaskCreateArgs {
        TaskCreateArgs {
            title: title.to_string(),
            body: body.to_string(),
            priority,
            due: due.map(String::from),
            origin: origin.map(String::from),
        }
    }

    #[test]
    fn build_description_minimal_payload() {
        // body 为空 → description 只有 header（title 不进 description，由
        // memory_edit 的 title 字段独立保存）
        let d = build_description(&args("整理 Downloads", "", 2, None, None)).unwrap();
        assert_eq!(d, "[task pri=2]");
    }

    #[test]
    fn build_description_with_body_appends_text() {
        let d = build_description(&args("整理", "把 30 天前的图片归档", 2, None, None)).unwrap();
        assert_eq!(d, "[task pri=2] 把 30 天前的图片归档");
    }

    #[test]
    fn build_description_with_due_and_origin() {
        let d = build_description(&args(
            "跑步",
            "30 分钟",
            3,
            Some("2026-05-05T19:00"),
            Some("tg:12345"),
        ))
        .unwrap();
        // body 是 "30 分钟"；title "跑步" 由 memory_edit 单独保存
        assert_eq!(d, "[task pri=3 due=2026-05-05T19:00] 30 分钟 [origin:tg:12345]");
        // round-trip 校验：classify Pending、origin 可读
        let (s, _) = classify_status(&d);
        assert_eq!(s, TaskStatus::Pending);
        assert_eq!(parse_task_origin(&d), Some(TaskOrigin::Tg(12345)));
    }

    #[test]
    fn build_description_rejects_empty_title() {
        assert!(build_description(&args("", "x", 0, None, None)).is_err());
        assert!(build_description(&args("   ", "x", 0, None, None)).is_err());
    }

    #[test]
    fn build_description_rejects_priority_overflow() {
        let err = build_description(&args("x", "", 10, None, None)).unwrap_err();
        assert!(err.contains("priority"));
    }

    #[test]
    fn build_description_rejects_malformed_due() {
        assert!(build_description(&args("x", "", 0, Some("not-a-date"), None)).is_err());
    }

    #[test]
    fn build_description_treats_empty_due_as_none() {
        let d = build_description(&args("x", "", 0, Some(""), None)).unwrap();
        assert!(!d.contains("due="));
    }

    // ---- parse_origin_arg ----

    #[test]
    fn parse_origin_arg_telegram() {
        assert_eq!(
            parse_origin_arg(Some("tg:42")).unwrap(),
            Some(TaskOrigin::Tg(42))
        );
        assert_eq!(
            parse_origin_arg(Some("tg:-100123")).unwrap(),
            Some(TaskOrigin::Tg(-100123))
        );
    }

    #[test]
    fn parse_origin_arg_empty_or_none() {
        assert_eq!(parse_origin_arg(None).unwrap(), None);
        assert_eq!(parse_origin_arg(Some("")).unwrap(), None);
        assert_eq!(parse_origin_arg(Some("  ")).unwrap(), None);
    }

    #[test]
    fn parse_origin_arg_rejects_bad_id() {
        assert!(parse_origin_arg(Some("tg:not-a-number")).is_err());
        assert!(parse_origin_arg(Some("tg:")).is_err());
    }

    #[test]
    fn parse_origin_arg_rejects_unknown_prefix() {
        assert!(parse_origin_arg(Some("webhook:xyz")).is_err());
        assert!(parse_origin_arg(Some("123")).is_err());
    }
}
