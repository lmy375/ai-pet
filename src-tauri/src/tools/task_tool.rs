//! `propose_task` 工具：把"自然语言派单"做成 LLM 可调用的 side-channel —
//! 工具本身**不持久化**，仅把校验过的提案 JSON 抛出去；前端 panel 在
//! 工具结果里识别出来后渲染确认卡，等用户点「创建任务」才调
//! `commands::task::task_create` 真正入队。
//!
//! 这条路径的重点是**两道门**：
//! 1. LLM 自己判断要不要 propose（工具描述里写明触发 / 不触发的情境）；
//! 2. 用户在前端确认（即便 LLM 误识别，最坏后果只是用户点一下「取消」）。
//!
//! 与 `task_create` Tauri 命令相比，本工具明确"提议而非创建"。同样的
//! 参数校验（title 非空 / priority 0..=9 / due 可解析）在两侧都要跑 —
//! 工具这一层是为了在 LLM 再思考前给出可读错误，确保前端拿到的 JSON
//! 已经能直接渲染成卡片。

use chrono::NaiveDateTime;
use serde::Deserialize;

use crate::task_queue::TASK_PRIORITY_MAX;
use crate::tools::{Tool, ToolContext};

pub struct ProposeTaskTool;

#[derive(Debug, Deserialize)]
struct ProposeTaskArgs {
    title: String,
    #[serde(default)]
    body: String,
    priority: u8,
    /// `YYYY-MM-DDThh:mm`（无时区，本地时间）。空字符串与缺失同义。
    #[serde(default)]
    due: Option<String>,
}

impl Tool for ProposeTaskTool {
    fn name(&self) -> &str {
        "propose_task"
    }

    fn definition(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "propose_task",
                "description": "Propose a task to the user when they ask you in natural language to do something that fits the task queue (the desktop panel's '任务' tab). The proposal will appear as a confirmation card in the panel chat — the user clicks 「创建任务」 to enqueue it, or 「取消」 to dismiss. This tool DOES NOT create the task itself; it only emits the proposal payload.\n\nWHEN TO CALL:\n- 用户用「帮我…」「记得…」「明天…之前…」等自然语言把一件具体事委托给你（且这件事不是当前对话里立刻就能聊完的）。\n- 例：「帮我整理 ~/Downloads 里的旧图片」/「记得明天下午 6 点前提醒我把报告交了」/「这周末把家里电费充一下」。\n\nWHEN NOT TO CALL:\n- 用户只是闲聊 / 提问 / 表达情绪。\n- 用户让你立刻就做且能在当前 turn 里完成（如「现在帮我看看天气」直接调 get_weather）。\n- 用户已经手动在面板里建过同名任务（先用 memory_list 查 butler_tasks 确认）。\n\nAFTER CALLING: 在自然语言回复里简短承接，比如「好的，把它加到队列了，去面板瞅一眼？」，让用户知道卡片是工具产物。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "title": {
                            "type": "string",
                            "description": "短标题，10-30 字符。从用户原话里抽取最核心的动词 + 对象，例：「整理 Downloads 旧图片」。"
                        },
                        "body": {
                            "type": "string",
                            "description": "（可选）展开描述。把用户原话里的限定条件（具体路径 / 时间 / 范围 / 操作意图）写清楚，方便后续宠物或用户回看。"
                        },
                        "priority": {
                            "type": "integer",
                            "minimum": 0,
                            "maximum": 9,
                            "description": "0-9 的优先级。日常杂活 1-3；用户语气紧迫（「赶紧」「今天必须」）取 5-7；明确说「最优先」取 8-9。"
                        },
                        "due": {
                            "type": "string",
                            "description": "（可选）截止时间，格式 `YYYY-MM-DDThh:mm`，本地时区，无时区后缀。用户提到「今晚」/「明天下午」等需要结合当前时间换算成绝对时间。"
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
        Box::pin(propose_task_impl(arguments, ctx))
    }
}

async fn propose_task_impl(arguments: &str, ctx: &ToolContext) -> String {
    let args: ProposeTaskArgs = match serde_json::from_str(arguments) {
        Ok(a) => a,
        Err(e) => {
            ctx.log(&format!("propose_task: bad args: {}", e));
            return format!(r#"{{"error": "invalid arguments: {}"}}"#, e);
        }
    };
    match validate(&args) {
        Ok(payload) => {
            ctx.log(&format!(
                "propose_task: emitted proposal '{}' (pri={})",
                payload.title.replace(['\n', '\r'], " "),
                payload.priority
            ));
            serde_json::to_string(&payload)
                .unwrap_or_else(|e| format!(r#"{{"error": "serialize failed: {}"}}"#, e))
        }
        Err(msg) => {
            ctx.log(&format!("propose_task: rejected: {}", msg));
            format!(r#"{{"error": "{}"}}"#, msg.replace('"', "\\\""))
        }
    }
}

/// 工具结果对外的 JSON 形态。`proposed: true` 是前端识别"这是一个待
/// 渲染卡片的提案"的关键标志；`due` 用 `Option<String>` 让 JSON 在
/// 缺省时输出 `null` —— 比省略字段更利于前端类型 narrowing。
#[derive(Debug, serde::Serialize, PartialEq)]
struct ProposalPayload {
    proposed: bool,
    title: String,
    body: String,
    priority: u8,
    due: Option<String>,
}

/// 纯函数：参数 → 提案 / 错误消息。把所有校验集中在这里便于单测，
/// 异步包装层只负责 IO 风格的日志 + 序列化。
fn validate(args: &ProposeTaskArgs) -> Result<ProposalPayload, String> {
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
    let due = match args.due.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => {
            NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M")
                .map_err(|e| format!("invalid due (expect YYYY-MM-DDThh:mm): {}", e))?;
            Some(s.to_string())
        }
        None => None,
    };
    Ok(ProposalPayload {
        proposed: true,
        title: title.to_string(),
        body: args.body.trim().to_string(),
        priority: args.priority,
        due,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(title: &str, body: &str, priority: u8, due: Option<&str>) -> ProposeTaskArgs {
        ProposeTaskArgs {
            title: title.to_string(),
            body: body.to_string(),
            priority,
            due: due.map(String::from),
        }
    }

    #[test]
    fn validate_accepts_minimal_payload() {
        let p = validate(&args("整理 Downloads", "", 2, None)).unwrap();
        assert_eq!(p.proposed, true);
        assert_eq!(p.title, "整理 Downloads");
        assert_eq!(p.body, "");
        assert_eq!(p.priority, 2);
        assert_eq!(p.due, None);
    }

    #[test]
    fn validate_trims_title_and_body() {
        let p = validate(&args("  整理  ", "  把旧图片归档  ", 1, None)).unwrap();
        assert_eq!(p.title, "整理");
        assert_eq!(p.body, "把旧图片归档");
    }

    #[test]
    fn validate_rejects_empty_title() {
        assert!(validate(&args("", "x", 0, None)).is_err());
        assert!(validate(&args("    ", "x", 0, None)).is_err());
    }

    #[test]
    fn validate_rejects_priority_out_of_range() {
        let err = validate(&args("x", "", 10, None)).unwrap_err();
        assert!(err.contains("priority"));
    }

    #[test]
    fn validate_accepts_well_formed_due() {
        let p = validate(&args("x", "", 0, Some("2026-05-05T18:00"))).unwrap();
        assert_eq!(p.due.as_deref(), Some("2026-05-05T18:00"));
    }

    #[test]
    fn validate_rejects_malformed_due() {
        assert!(validate(&args("x", "", 0, Some("not-a-date"))).is_err());
        assert!(validate(&args("x", "", 0, Some("2026-13-99T25:99"))).is_err());
    }

    #[test]
    fn validate_treats_empty_due_as_none() {
        // 前端 datetime-local 留空时常传 ""，不该当成"格式错"
        let p = validate(&args("x", "", 0, Some(""))).unwrap();
        assert_eq!(p.due, None);
        let p = validate(&args("x", "", 0, Some("   "))).unwrap();
        assert_eq!(p.due, None);
    }
}
