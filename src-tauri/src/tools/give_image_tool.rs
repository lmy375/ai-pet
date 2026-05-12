//! `give_image` LLM 工具：让模型在用户说"画一张..." / "做张图给我看看"等自然
//! 语言请求时直接调起生图，不用 user 显式敲 `/image`。
//!
//! ## 协议设计
//!
//! 工具结果是双 payload：
//! - 给 LLM 看的：`{"ok": true, "count": N, "size": "WxH", "model": "dall-e-3"}`
//!   —— 简短，让模型知道生成了几张。**不**包含 base64 数据，避免 ~1MB/张的
//!   data URL 灌进下一轮上下文（既费 token 又毫无价值，模型已经知道自己刚生
//!   成了什么）。
//! - 给前端看的：同上字段 + `_attachments: [data URLs]` 数组。前端 ToolCallBlock
//!   识别 `_attachments` 字段，渲染图片缩略图网格。
//!
//! chat 管道层在工具执行后会从 conv_messages（次轮 LLM 上下文）里 strip 掉
//! `_attachments` 字段，但 send_tool_result 给前端时仍带全字段 —— 见 chat.rs
//! 里 strip_tool_attachments 的注释。

use crate::commands::image::run_image_generate;
use crate::tools::{Tool, ToolContext};

pub struct GiveImageTool;

impl Tool for GiveImageTool {
    fn name(&self) -> &str {
        "give_image"
    }

    fn definition(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "give_image",
                "description": "Generate an image (or several) and show it to the user. Use this when the user asks you to draw / paint / create / make an image — for example '画一张兔子', 'draw me a sunset', '做张图看看'. Don't use it for memory recall or environment-awareness questions. The image is shown in the chat UI; you don't see the bytes — assume it succeeded if `ok` is true and continue the conversation naturally (e.g. brief comment like '画好啦~' or '这是我画的'). Don't fabricate URLs or base64 in your reply.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "prompt": {
                            "type": "string",
                            "description": "What to draw, in natural language. English or Chinese both work; the underlying model interprets it. Example: 'a watercolor painting of a rabbit dancing on the moon'."
                        },
                        "n": {
                            "type": "integer",
                            "description": "How many images to generate. Defaults to 1. Most providers cap at 1-4; dall-e-3 only supports 1.",
                            "minimum": 1,
                            "maximum": 8
                        }
                    },
                    "required": ["prompt"]
                }
            }
        })
    }

    fn execute<'a>(
        &'a self,
        arguments: &'a str,
        ctx: &'a ToolContext,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = String> + Send + 'a>> {
        Box::pin(give_image_impl(arguments, ctx))
    }
}

async fn give_image_impl(arguments: &str, ctx: &ToolContext) -> String {
    let args: serde_json::Value = serde_json::from_str(arguments).unwrap_or_default();
    let prompt = args["prompt"].as_str().unwrap_or("").trim().to_string();
    let n = args["n"].as_u64().unwrap_or(1) as u32;

    if prompt.is_empty() {
        return r#"{"error": "prompt is required and cannot be empty"}"#.to_string();
    }

    ctx.log(&format!("give_image: prompt={:?} n={}", prompt, n));

    match run_image_generate(&prompt, n, None).await {
        Ok(result) => {
            let count = result.urls.len();
            let failed = result.errors.len();
            // 注意 _attachments 含 base64 → 体积大；chat.rs 在塞回 next-round
            // LLM 上下文前会 strip 这个字段。前端事件路径仍能拿到完整的。
            // errors 不剔除 —— 让 LLM 看到"画了 X/N 张，其中 N-X 失败因为 Y"，
            // 模型可以自然回"我画了 3 张，1 张被 policy 拒了"。
            serde_json::json!({
                "ok": count > 0,
                "count": count,
                "failed": failed,
                "errors": result.errors,
                "_attachments": result.urls,
            })
            .to_string()
        }
        Err(e) => serde_json::json!({
            "ok": false,
            "error": e,
        })
        .to_string(),
    }
}
