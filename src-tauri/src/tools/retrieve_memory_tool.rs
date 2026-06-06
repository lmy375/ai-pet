//! GOAL 038：`retrieve_memory` LLM tool——把 [[memory_retrieval]] 模块包成
//! LLM 可调用的工具。当 user turn 出现 "之前 / 上次 / 我说过 / 你记得 /
//! 那次" 等 retrospective 信号时，LLM 应主动调本工具而非凭 prompt window
//! in-context 内容硬猜。
//!
//! 工具描述里明确「失败 / 无命中 → 诚实答没找到，不编造」——spec 硬约束。

use crate::tools::context::ToolContext;
use crate::tools::tool::Tool;

pub struct RetrieveMemoryTool;

impl Tool for RetrieveMemoryTool {
    fn name(&self) -> &str {
        "retrieve_memory"
    }

    fn definition(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "retrieve_memory",
                "description": "Cross-source retrospective memory search. Call this whenever the owner asks about something from the past — \"之前 / 上次 / 我说过 / 你记得 / 那次 / 之前提过 / 上回\" / English equivalents — instead of guessing from in-context conversation.\n\n**Sources** (default `all`):\n- `memory` — PanelMemory items (user_profile / ai_insights / todo / butler_tasks / general)\n- `butler_history` — pet's audit log (reminder fires, cancels, snoozes, follow-ups, scheduled_report runs)\n\n**Output**: array of `{source, ts, text, link_id, score}` sorted by relevance.\n\n**When unsure**: pass `sources=all` and `top_n=5`. If results are empty, **answer honestly**「这个我没找到记录」— do NOT fabricate or paraphrase a non-existent memory. If results exist, cite the specific ts + verbatim snippet rather than your own paraphrase.\n\nNot for current-state lookups (use memory_list for those). Specifically retrospective.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Search query. Use the owner's own words when possible (\"咖啡店\" / \"项目 ddl\") — verbatim tokens score better than paraphrase."
                        },
                        "top_n": {
                            "type": "integer",
                            "description": "Max results (1-50). Default 5 — enough for citation, not so many that LLM context blows up.",
                            "minimum": 1,
                            "maximum": 50
                        },
                        "sources": {
                            "type": "string",
                            "description": "Comma / whitespace separated source names. `all` or empty = scan everything. Subset example: `memory,butler_history`. Unknown tokens are silently skipped."
                        }
                    },
                    "required": ["query"]
                }
            }
        })
    }

    fn execute<'a>(
        &'a self,
        arguments: &'a str,
        ctx: &'a ToolContext,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = String> + Send + 'a>> {
        Box::pin(execute_impl(arguments, ctx))
    }
}

async fn execute_impl(arguments: &str, ctx: &ToolContext) -> String {
    let args: serde_json::Value = serde_json::from_str(arguments).unwrap_or_default();
    let query = args["query"].as_str().unwrap_or("").trim().to_string();
    let top_n = args["top_n"].as_u64().map(|n| n as usize).unwrap_or(5);
    let sources_arg = args["sources"].as_str().unwrap_or("");

    if query.is_empty() {
        return serde_json::json!({ "error": "`query` 不能为空" }).to_string();
    }

    let sources = crate::memory_retrieval::parse_sources(sources_arg);
    let items = crate::memory_retrieval::retrieve(&query, top_n, &sources).await;

    ctx.log(&format!(
        "retrieve_memory: query='{}' sources={} top_n={} hits={}",
        query,
        sources.len(),
        top_n,
        items.len()
    ));

    serde_json::json!({
        "query": query,
        "count": items.len(),
        "items": items,
    })
    .to_string()
}
