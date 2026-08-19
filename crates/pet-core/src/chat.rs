use genai::chat::{ChatMessage as GenAiMessage, ChatRequest, MessageContent, ToolResponse};
use serde::Serialize;

use crate::config::AiConfig;
use crate::logging::write_llm_log;
use crate::mcp::McpManagerStore;
use crate::tools::ToolContext;
use crate::tools::ToolRegistry;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "event", content = "data")]
pub enum StreamEvent {
    Chunk { text: String },
    /// Chain-of-thought from a reasoning model — the `reasoning_content` /
    /// `reasoning` delta field, or text peeled out of inline `<think>…</think>`
    /// tags. Kept on its own channel so the UI can show it in a collapsed
    /// "thinking" block instead of mixing it into the answer.
    Reasoning { text: String },
    ToolStart { name: String, arguments: String },
    ToolResult { name: String, result: String },
    /// A data URL a tool produced for the model to see (e.g. `screenshot`).
    /// Surfaced so the UI can render it as an image bubble, not just feed it
    /// to the model. NOTE: enum-level `rename_all` only renames variants, not
    /// variant fields — so this field needs an explicit `rename` to reach the
    /// frontend as `dataUrl`.
    Image {
        #[serde(rename = "dataUrl")]
        data_url: String,
    },
    /// Token usage for the round that just completed, surfaced so the UI can
    /// render a context-occupancy ring. Sent once per LLM round; the frontend
    /// keeps the latest (the final round carries the fullest context). As with
    /// `Image`, `rename_all` only renames variants, so each field needs an
    /// explicit `rename` to reach the frontend in camelCase.
    Usage {
        #[serde(rename = "promptTokens")]
        prompt_tokens: u64,
        #[serde(rename = "totalTokens")]
        total_tokens: u64,
        #[serde(rename = "contextWindow")]
        context_window: u32,
    },
    Done {},
    Error { message: String },
}

/// Abstraction for chat event delivery — allows both Tauri streaming and non-streaming callers.
pub trait ChatEventSink: Send + Sync {
    fn send_chunk(&self, text: &str);
    fn send_reasoning(&self, text: &str);
    fn send_tool_start(&self, name: &str, arguments: &str);
    fn send_tool_result(&self, name: &str, result: &str);
    fn send_image(&self, data_url: &str);
    fn send_usage(&self, prompt_tokens: u64, total_tokens: u64, context_window: u32);
    fn send_done(&self);
    fn send_error(&self, message: &str);
}

/// The sink for non-streaming callers (Telegram, heartbeats, sub-agents). The
/// final assistant text is returned by `run_chat_pipeline` directly, so all
/// streaming events are discarded — except images a tool surfaces (e.g.
/// `screenshot`), which are buffered so a caller that can render them (Telegram)
/// may forward them afterwards via `take_images()`. Callers that don't care
/// about images (heartbeats, sub-agents) simply never call `take_images`.
#[derive(Default)]
pub struct ImageCollectingSink {
    images: std::sync::Mutex<Vec<String>>,
}

impl ImageCollectingSink {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn take_images(&self) -> Vec<String> {
        std::mem::take(&mut *self.images.lock().unwrap())
    }
}

impl ChatEventSink for ImageCollectingSink {
    fn send_chunk(&self, _text: &str) {}
    fn send_reasoning(&self, _text: &str) {}
    fn send_tool_start(&self, _name: &str, _arguments: &str) {}
    fn send_tool_result(&self, _name: &str, _result: &str) {}
    fn send_image(&self, data_url: &str) {
        self.images.lock().unwrap().push(data_url.to_string());
    }
    fn send_usage(&self, _prompt_tokens: u64, _total_tokens: u64, _context_window: u32) {}
    fn send_done(&self) {}
    fn send_error(&self, _message: &str) {}
}

/// Run the full LLM chat pipeline with tool calling. Returns final assistant text.
/// This is the core logic shared by the Tauri command and Telegram bot.
pub async fn run_chat_pipeline(
    mut conv_messages: Vec<serde_json::Value>,
    sink: &dyn ChatEventSink,
    config: &AiConfig,
    mcp_store: &McpManagerStore,
    ctx: &ToolContext,
) -> Result<String, String> {
    let user_msg = crate::llm::load_messages(&conv_messages)
        .iter()
        .rev()
        .find(|m| m.role == genai::chat::ChatRole::User)
        .and_then(|m| m.content.first_text().map(|t| t.to_string()))
        .unwrap_or_default();
    ctx.log(&format!("Chat request: model={}, user=\"{}\"", config.model, user_msg));

    // Rebuild the system prompt (persona + long-term memory + tool guidance)
    // from the current memory files on every turn, so edits the pet makes to
    // USER.md / MEMORY.md take effect immediately instead of being frozen at
    // session creation.
    crate::prompt::prepend_system_messages(&mut conv_messages, &config.agent_id);

    let (text, _conv) = run_agent_loop(conv_messages, sink, config, mcp_store, ctx).await?;
    Ok(text)
}

/// Run the tool-calling loop over an already-assembled message list (system
/// prompt MUST already be included). Returns the final assistant text AND the
/// full conversation (including every tool round and the final assistant
/// message), so callers that need to persist the accumulated context — notably
/// the group-chat orchestrator, which keeps each agent's private session across
/// turns — can do so. Callers that only want the text ignore the second element.
///
/// Split out from `run_chat_pipeline` so callers that supply their own system
/// prompt — notably the `spawn_subagent` tool, which gives a sub-agent a
/// task-focused prompt instead of the pet persona — can reuse the exact same
/// loop, registry, MCP routing and streaming infrastructure.
pub async fn run_agent_loop(
    mut conv_messages: Vec<serde_json::Value>,
    sink: &dyn ChatEventSink,
    config: &AiConfig,
    mcp_store: &McpManagerStore,
    ctx: &ToolContext,
) -> Result<(String, Vec<serde_json::Value>), String> {
    // Get MCP tool definitions for this agent (each agent has its own server set).
    let mcp_defs = {
        let managers = mcp_store.lock().await;
        managers.get(&config.agent_id).map(|m| m.definitions()).unwrap_or_default()
    };
    // Sub-agents (depth > 0) don't get the spawn tool, so they can't recurse.
    // The `chat` tool is offered only to heartbeat sessions. `web_search` is
    // offered only when a Tavily key is configured.
    let web_search_enabled = !config.search_api_key.trim().is_empty();
    let registry = ToolRegistry::new(
        mcp_defs,
        ctx.depth,
        ctx.is_heartbeat,
        web_search_enabled,
        ctx.group.is_some(),
    );
    let client = crate::llm::client();
    let options = crate::llm::chat_options(config);
    let tools = crate::llm::tools_from_definitions(&registry.definitions());

    // Tool calling loop (unlimited rounds)
    let mut round = 0usize;
    loop {
        ctx.log(&format!("LLM round {} ({} messages)", round, conv_messages.len()));

        let chat_req = ChatRequest::new(crate::llm::load_messages(&conv_messages))
            .with_tools(tools.clone());

        let result =
            crate::llm::stream_chat(&client, config, chat_req, &options, sink, ctx).await?;

        // Surface this round's token usage to the UI's context ring. The frontend
        // keeps the latest, so the final round (fullest context) wins.
        if let (Some(prompt), Some(total)) = (result.prompt_tokens, result.total_tokens) {
            sink.send_usage(prompt, total, config.context_window);
        }

        if !result.reasoning.is_empty() {
            ctx.log(&format!("Reasoning ({} chars)", result.reasoning.len()));
        }

        write_llm_log(
            &ctx.log_session,
            round,
            &serde_json::json!({ "model": config.model, "messages": conv_messages }),
            &result.text,
            &result.reasoning,
            &result
                .tool_calls
                .iter()
                .map(|tc| serde_json::to_value(tc).unwrap_or(serde_json::Value::Null))
                .collect::<Vec<_>>(),
            &result.request_time,
            result.first_token_time.as_deref(),
            &result.done_time,
            result.first_token_latency_ms,
            result.total_latency_ms,
        );

        if result.tool_calls.is_empty() {
            ctx.log(&format!("Final response ({} chars, TTFT={}ms, total={}ms)",
                result.text.len(),
                result.first_token_latency_ms.unwrap_or(-1),
                result.total_latency_ms,
            ));
            sink.send_done();
            // Append the final assistant message so the returned conversation is a
            // complete, valid continuation (used by the group orchestrator to keep
            // an agent's private context). Skip when empty — an empty assistant
            // message is not a useful context entry and some providers reject one.
            if !result.text.is_empty() {
                conv_messages.push(crate::llm::store_message(&GenAiMessage::assistant(
                    result.text.clone(),
                )));
            }
            return Ok((result.text, conv_messages));
        }

        ctx.log(&format!("Tool calls: {}", result.tool_calls.len()));

        // The assistant turn genai captured, which puts any thought signatures
        // ahead of the tool calls. Replaying it verbatim is what lets Anthropic
        // extended thinking and the Responses API survive a tool round-trip —
        // rebuilding it from `text` + `tool_calls` would drop those parts.
        if let Some(turn) = &result.assistant_turn {
            conv_messages.push(crate::llm::store_message(turn));
        }

        // Execute each tool call via registry or MCP manager
        for tc in &result.tool_calls {
            let tc_name = tc.fn_name.as_str();
            // Tools take their arguments as a JSON string.
            let tc_args = if tc.fn_arguments.is_null() {
                "{}".to_string()
            } else {
                tc.fn_arguments.to_string()
            };

            sink.send_tool_start(tc_name, &tc_args);

            let output = if registry.is_mcp_tool(tc_name) {
                // Route to MCP manager
                ctx.log(&format!("MCP tool call: {}({})", tc_name, tc_args));
                let managers = mcp_store.lock().await;
                let call_res = match managers.get(&config.agent_id) {
                    Some(m) => m.call_tool(tc_name, tc.fn_arguments.clone()).await,
                    None => Err(format!("No MCP manager for agent {}", config.agent_id)),
                };
                match call_res {
                    Ok(r) => r,
                    Err(e) => crate::tools::tool_error(e),
                }
            } else {
                // Built-in tool
                registry.execute(tc_name, &tc_args, ctx).await
            };

            ctx.log(&format!("Tool result [{}]: {} chars", tc_name, output.len()));

            sink.send_tool_result(tc_name, &output);

            // `fn_name` is carried alongside `call_id` because Gemini correlates
            // tool responses by function name, not by id.
            conv_messages.push(crate::llm::store_message(&GenAiMessage::tool(
                MessageContent::from_tool_responses(vec![
                    ToolResponse::new(tc.call_id.clone(), output).with_fn_name(tc_name),
                ]),
            )));
        }

        // Some tools (e.g. `screenshot`) produce an image the model must actually
        // SEE — a `tool` message can't carry one, so they queue a data URL on the
        // context. Drain it here, after every `tool` message for this round is in
        // place (keeping them contiguous for call_id pairing), and append the
        // images as a `user` message — the same multimodal path used for pastes.
        let imgs = ctx.take_images();
        if !imgs.is_empty() {
            // Surface each image to the UI so it renders as an image bubble —
            // the frontend never sees `conv_messages`, only stream events.
            for url in &imgs {
                sink.send_image(url);
            }
            conv_messages.push(crate::llm::store_message(&crate::llm::user_message("", &imgs)));
        }

        round += 1;
    }
}

/// Builds the display-transcript items (`ChatItem` JSON, see `useChat.ts`) for
/// one agent run from its stream events, mirroring the frontend's stream
/// reducer so persisted items match what a live listener would have rendered.
/// Shared by the group orchestrator's `GroupSink` and the CLI's terminal sink —
/// any sink that must persist a session without a frontend doing it.
#[derive(Default)]
pub struct ItemBuilder {
    /// Accumulated assistant text not yet committed.
    accumulated: String,
    /// Accumulated chain-of-thought for the current assistant item. Display-only.
    reasoning: String,
    /// Tool calls in the current (not-yet-flushed) group.
    tool_calls: Vec<serde_json::Value>,
    /// Committed display items.
    items: Vec<serde_json::Value>,
}

impl ItemBuilder {
    fn now_ms() -> i64 {
        chrono::Local::now().timestamp_millis()
    }

    pub fn flush_tool_calls(&mut self) {
        if self.tool_calls.is_empty() {
            return;
        }
        let calls = std::mem::take(&mut self.tool_calls);
        self.items.push(serde_json::json!({
            "type": "tool",
            "content": "",
            "toolCalls": calls,
            "ts": Self::now_ms(),
        }));
    }

    pub fn commit_text(&mut self) {
        let text = std::mem::take(&mut self.accumulated);
        let reasoning = std::mem::take(&mut self.reasoning);
        if text.trim().is_empty() {
            return;
        }
        let mut item = serde_json::json!({
            "type": "assistant",
            "content": text,
            "ts": Self::now_ms(),
        });
        if !reasoning.is_empty() {
            item["reasoning"] = serde_json::json!(reasoning);
        }
        self.items.push(item);
    }

    pub fn chunk(&mut self, text: &str) {
        self.flush_tool_calls();
        self.accumulated.push_str(text);
    }

    pub fn reasoning(&mut self, text: &str) {
        self.reasoning.push_str(text);
    }

    pub fn tool_start(&mut self, name: &str, arguments: &str) {
        // Commit any assistant text streamed before this tool call.
        self.commit_text();
        self.tool_calls.push(serde_json::json!({
            "name": name,
            "arguments": arguments,
            "isRunning": false,
        }));
    }

    pub fn tool_result(&mut self, name: &str, result: &str) {
        // Attach to the first tool call of that name still missing a result.
        if let Some(tc) = self
            .tool_calls
            .iter_mut()
            .find(|tc| tc["name"] == serde_json::json!(name) && tc.get("result").is_none())
        {
            tc["result"] = serde_json::json!(result);
        }
    }

    pub fn image(&mut self, data_url: &str) {
        self.flush_tool_calls();
        self.items.push(serde_json::json!({
            "type": "assistant",
            "content": "",
            "images": [data_url],
            "ts": Self::now_ms(),
        }));
    }

    pub fn done(&mut self) {
        self.flush_tool_calls();
        self.commit_text();
    }

    pub fn error(&mut self, message: &str) {
        self.flush_tool_calls();
        self.items.push(serde_json::json!({
            "type": "error",
            "content": message,
            "ts": Self::now_ms(),
        }));
    }

    /// Take the accumulated display items (after the run finishes).
    pub fn take_items(&mut self) -> Vec<serde_json::Value> {
        std::mem::take(&mut self.items)
    }
}
