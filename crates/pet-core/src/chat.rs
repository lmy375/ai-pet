use futures_util::future::join_all;
use genai::chat::{ChatMessage as GenAiMessage, ChatRequest, MessageContent, ToolCall, ToolResponse};
use serde::{Deserialize, Serialize};

use crate::config::AiConfig;
use crate::logging::{write_llm_log, RoundStat};
use crate::mcp::McpStore;
use crate::tools::ToolContext;
use crate::tools::{ToolPolicy, ToolRegistry};

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
/// One turn of user input, as the interfaces describe it: plain text plus any
/// attached images as `data:` URLs. Deliberately provider-neutral — callers
/// never build LLM messages themselves, so genai's wire shape stays inside
/// `llm` (see `llm::user_message`).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct UserTurn {
    pub text: String,
    #[serde(default)]
    pub images: Vec<String>,
}

impl UserTurn {
    /// A text-only turn — heartbeats, group runs and background-task
    /// resumptions never carry images.
    pub fn text(text: impl Into<String>) -> Self {
        Self { text: text.into(), images: Vec::new() }
    }
}

/// What a completed chat turn produced: the assistant's final text, and the
/// full conversation to persist. Callers store `messages` verbatim and never
/// inspect it — that's what keeps thought signatures and tool rounds intact.
#[derive(Debug, Clone, Serialize)]
pub struct ChatOutcome {
    pub text: String,
    pub messages: Vec<serde_json::Value>,
}

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
    conv_messages: Vec<serde_json::Value>,
    turn: UserTurn,
    sink: &dyn ChatEventSink,
    config: &AiConfig,
    mcp_store: &McpStore,
    ctx: &ToolContext,
) -> Result<ChatOutcome, String> {
    ctx.log(&format!(
        "Chat request: model={}, user=\"{}\"",
        config.model, turn.text
    ));

    // Normalize the incoming history once: anything that no longer parses is
    // dropped here rather than being silently skipped on every request while
    // living on in the session file forever. What gets stored is exactly what
    // gets sent.
    let mut conv_messages: Vec<serde_json::Value> = crate::llm::load_messages(&conv_messages)
        .iter()
        .map(crate::llm::store_message)
        .collect();

    conv_messages.push(crate::llm::store_message(&crate::llm::user_message(
        &turn.text,
        &turn.images,
    )));

    // Rebuild the system prompt (persona + long-term memory + tool guidance)
    // from the current memory files on every turn, so edits the pet makes to
    // USER.md / MEMORY.md take effect immediately instead of being frozen at
    // session creation.
    crate::prompt::prepend_system_messages(&mut conv_messages, &config.agent_id);

    let (text, messages) = run_agent_loop(conv_messages, sink, config, mcp_store, ctx).await?;
    // Drop the leading system block before handing the conversation back. It is
    // rebuilt from the memory files on every turn, so storing it would stack up
    // another copy of the tool-usage prompt per turn.
    let messages = messages
        .into_iter()
        .skip_while(crate::llm::is_system_message)
        .collect();
    Ok(ChatOutcome { text, messages })
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
    mcp_store: &McpStore,
    ctx: &ToolContext,
) -> Result<(String, Vec<serde_json::Value>), String> {
    // Get MCP tool definitions for this agent (the servers it references in the
    // global pool — connections themselves are shared between agents).
    let mcp_defs = mcp_store.read().await.definitions(&config.mcp_servers);
    // Context gates: sub-agents (depth > 0) don't get the spawn tool, so they
    // can't recurse; `chat` is offered only to heartbeat sessions; `web_search`
    // only when a Tavily key is configured. `from_config` adds the owner's own
    // layer on top (tools switched off, descriptions rewritten), which can only
    // subtract from these.
    let registry = ToolRegistry::new(
        mcp_defs,
        ToolPolicy {
            depth: ctx.depth,
            include_chat: ctx.is_heartbeat,
            include_web_search: !config.search_api_key.trim().is_empty(),
            include_swarm: ctx.swarm.is_some(),
            include_swarm_worker: ctx.swarm_task_id.is_some() && ctx.swarm_worker_id.is_some(),
            ..ToolPolicy::from_config()
        },
    );
    let client = crate::llm::client();
    let options = crate::llm::chat_options(config);
    let tools = crate::llm::tools_from_definitions(&registry.definitions());

    // Tool calling loop (unlimited rounds)
    let mut round = 0usize;
    // Per-round latency for the LLM log. The log body only keeps the final
    // round (every round's request is a superset of the previous one), so this
    // is the only place earlier rounds' timings survive.
    let mut round_stats: Vec<RoundStat> = Vec::new();
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

        let tool_calls: Vec<serde_json::Value> = result
            .tool_calls
            .iter()
            .map(|tc| serde_json::to_value(tc).unwrap_or(serde_json::Value::Null))
            .collect();
        round_stats.push(RoundStat {
            round,
            ttft_ms: result.first_token_latency_ms,
            total_ms: result.total_latency_ms,
            tools: result.tool_calls.iter().map(|tc| tc.fn_name.clone()).collect(),
        });
        write_llm_log(
            &ctx.log_session,
            &config.model,
            &round_stats,
            &conv_messages,
            &result.text,
            &result.reasoning,
            &tool_calls,
            &result.request_time,
            result.first_token_time.as_deref(),
            &result.done_time,
        );

        // An aborted round comes back with its tool calls cleared, so it takes
        // the final-response path below: whatever text streamed before the
        // stop is kept in context as an ordinary assistant message, and the
        // next turn continues from it.
        if result.cancelled {
            ctx.log(&format!("Cancelled by user ({} chars kept)", result.text.len()));
        }

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

        // Run every call from this assistant turn concurrently. Result order is
        // preserved below so the UI and provider transcript stay deterministic.
        let outputs = execute_tool_calls(
            &result.tool_calls,
            sink,
            config,
            mcp_store,
            &registry,
            ctx,
        )
        .await;
        for (tc, output) in result.tool_calls.iter().zip(outputs) {
            // `fn_name` is carried alongside `call_id` because Gemini correlates
            // tool responses by function name, not by id.
            conv_messages.push(crate::llm::store_message(&GenAiMessage::tool(
                MessageContent::from_tool_responses(vec![
                    ToolResponse::new(tc.call_id.clone(), output)
                        .with_fn_name(tc.fn_name.clone()),
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

        if ctx.cancel.is_cancelled() {
            sink.send_done();
            return Ok((String::new(), conv_messages));
        }

        round += 1;
    }
}

/// Execute one assistant turn's tool calls concurrently, preserving the order of
/// `tool_calls` in both the emitted result events and the returned outputs.
async fn execute_tool_calls(
    tool_calls: &[ToolCall],
    sink: &dyn ChatEventSink,
    config: &AiConfig,
    mcp_store: &McpStore,
    registry: &ToolRegistry,
    ctx: &ToolContext,
) -> Vec<String> {
    let executions = tool_calls.iter().map(|tc| {
        let name = tc.fn_name.clone();
        // Tools take their arguments as a JSON string.
        let arguments = if tc.fn_arguments.is_null() {
            "{}".to_string()
        } else {
            tc.fn_arguments.to_string()
        };
        let arguments_for_mcp = tc.fn_arguments.clone();
        let call_ctx = ctx.for_tool_call();

        sink.send_tool_start(&name, &arguments);

        async move {
            let execute = async {
                if registry.is_mcp_tool(&name) {
                    // Read locks let different MCP calls run concurrently while
                    // still excluding reconnect/shutdown mutations.
                    call_ctx.log(&format!("MCP tool call: {}({})", name, arguments));
                    let hub = mcp_store.read().await;
                    match hub
                        .call_tool(&config.mcp_servers, &name, arguments_for_mcp)
                        .await
                    {
                        Ok(result) => result,
                        Err(error) => crate::tools::tool_error(error),
                    }
                } else {
                    registry.execute(&name, &arguments, &call_ctx).await
                }
            };
            // Cancellation reaches every in-flight call at once. Each call still
            // returns a tool response, since providers reject dangling calls.
            let output = tokio::select! {
                biased;
                _ = call_ctx.cancel.cancelled() => {
                    call_ctx.log(&format!("Tool call [{}] cancelled by user", name));
                    crate::tools::tool_error("cancelled by user")
                }
                output = execute => output,
            };

            (output, call_ctx.take_images())
        }
    });

    let results = join_all(executions).await;
    let mut outputs = Vec::with_capacity(results.len());
    for (tc, (output, images)) in tool_calls.iter().zip(results) {
        ctx.log(&format!("Tool result [{}]: {} chars", tc.fn_name, output.len()));
        sink.send_tool_result(&tc.fn_name, &output);
        for image in images {
            ctx.emit_image(image);
        }
        outputs.push(output);
    }
    outputs
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
            "id": crate::session::item_id(),
            "type": "tool",
            "content": "",
            "toolCalls": calls,
            "ts": Self::now_ms(),
        }));
    }

    pub fn commit_text(&mut self) {
        let text = std::mem::take(&mut self.accumulated);
        let reasoning = std::mem::take(&mut self.reasoning);
        // A round that produced only thinking still becomes an item: the model
        // thought and then called a tool with no preamble. Both fields were
        // already taken above, so bailing out on empty text alone would drop
        // the thought for good. This mirrors `commitText` in useChat.ts, which
        // commits when either side is non-empty — the live view and the
        // reloaded transcript have to agree.
        if text.trim().is_empty() && reasoning.trim().is_empty() {
            return;
        }
        let mut item = serde_json::json!({
            "id": crate::session::item_id(),
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
        // Thinking for a new round starts only after the previous round's tool
        // calls returned, so flush their group here. Otherwise a reasoning-only
        // run would collect every round's calls and commit them as one item
        // after all the thinking, at `done` — the live view and the reloaded
        // transcript both then show "all thoughts, then all tools".
        self.flush_tool_calls();
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
            "id": crate::session::item_id(),
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
            "id": crate::session::item_id(),
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use genai::chat::ToolCall;
    use tokio::sync::{Barrier, Notify};

    use super::{execute_tool_calls, ImageCollectingSink, ItemBuilder};
    use crate::config::AiConfig;
    use crate::logging::LogStore;
    use crate::shell::ShellStore;
    use crate::tools::{Tool, ToolContext, ToolRegistry};

    struct BarrierTool {
        name: &'static str,
        barrier: Arc<Barrier>,
    }

    struct ImageTool {
        name: &'static str,
        image: &'static str,
        wait_for: Option<Arc<Notify>>,
        notify: Option<Arc<Notify>>,
    }

    impl Tool for BarrierTool {
        fn name(&self) -> &str {
            self.name
        }

        fn definition(&self) -> serde_json::Value {
            serde_json::json!({})
        }

        fn execute<'a>(
            &'a self,
            _arguments: &'a str,
            _ctx: &'a ToolContext,
        ) -> Pin<Box<dyn Future<Output = String> + Send + 'a>> {
            let name = self.name.to_string();
            let barrier = self.barrier.clone();
            Box::pin(async move {
                barrier.wait().await;
                name
            })
        }
    }

    impl Tool for ImageTool {
        fn name(&self) -> &str {
            self.name
        }

        fn definition(&self) -> serde_json::Value {
            serde_json::json!({})
        }

        fn execute<'a>(
            &'a self,
            _arguments: &'a str,
            ctx: &'a ToolContext,
        ) -> Pin<Box<dyn Future<Output = String> + Send + 'a>> {
            let name = self.name.to_string();
            let image = self.image.to_string();
            let wait_for = self.wait_for.clone();
            let notify = self.notify.clone();
            Box::pin(async move {
                if let Some(wait_for) = wait_for {
                    wait_for.notified().await;
                }
                ctx.emit_image(image);
                if let Some(notify) = notify {
                    notify.notify_one();
                }
                name
            })
        }
    }

    fn tool_call(call_id: &str, name: &str) -> ToolCall {
        ToolCall {
            call_id: call_id.to_string(),
            fn_name: name.to_string(),
            fn_arguments: serde_json::json!({}),
            thought_signatures: None,
        }
    }

    fn test_context() -> (AiConfig, ToolContext) {
        let config = AiConfig {
            agent_id: "test".to_string(),
            api_key: String::new(),
            base_url: String::new(),
            model: String::new(),
            provider: String::new(),
            context_window: 0,
            search_api_key: String::new(),
            mcp_servers: Vec::new(),
            reasoning: String::new(),
        };
        let ctx = ToolContext::new(
            LogStore(Arc::new(Mutex::new(Vec::new()))),
            ShellStore(Arc::new(Mutex::new(HashMap::new()))),
            config.clone(),
            crate::mcp::new_mcp_store(),
            "test-session".to_string(),
            None,
            None,
            false,
        );
        (config, ctx)
    }

    /// Both tools must enter `execute` before either can finish. A serial loop
    /// would park on the first barrier forever and hit the timeout.
    #[tokio::test]
    async fn tool_calls_in_one_round_run_concurrently() {
        let barrier = Arc::new(Barrier::new(2));
        let registry = ToolRegistry::for_test(vec![
            Box::new(BarrierTool { name: "first", barrier: barrier.clone() }),
            Box::new(BarrierTool { name: "second", barrier }),
        ]);
        let calls = vec![tool_call("call-1", "first"), tool_call("call-2", "second")];
        let (config, ctx) = test_context();
        let mcp_store = ctx.mcp_store.clone();
        let sink = ImageCollectingSink::new();

        let outputs = tokio::time::timeout(
            Duration::from_secs(2),
            execute_tool_calls(&calls, &sink, &config, &mcp_store, &registry, &ctx),
        )
        .await
        .expect("tool calls did not reach the barrier concurrently");

        assert_eq!(outputs, vec!["first", "second"]);
    }

    /// The second call finishes (and emits its image) first. The parent context
    /// must still receive images in call order, matching the tool results.
    #[tokio::test]
    async fn images_are_merged_in_tool_call_order() {
        let notify = Arc::new(Notify::new());
        let registry = ToolRegistry::for_test(vec![
            Box::new(ImageTool {
                name: "first",
                image: "data:first",
                wait_for: Some(notify.clone()),
                notify: None,
            }),
            Box::new(ImageTool {
                name: "second",
                image: "data:second",
                wait_for: None,
                notify: Some(notify),
            }),
        ]);
        let calls = vec![tool_call("call-1", "first"), tool_call("call-2", "second")];
        let (config, ctx) = test_context();
        let mcp_store = ctx.mcp_store.clone();
        let sink = ImageCollectingSink::new();

        let outputs = execute_tool_calls(&calls, &sink, &config, &mcp_store, &registry, &ctx).await;

        assert_eq!(outputs, vec!["first", "second"]);
        assert_eq!(ctx.take_images(), vec!["data:first", "data:second"]);
    }

    /// The model thinks, then calls a tool without writing a preamble. That
    /// thought has to reach the session file: it used to be taken out of the
    /// builder and discarded by the empty-text early return, so the GUI showed
    /// it while streaming and lost it the moment the turn finished and the
    /// persisted transcript replaced the live view.
    #[test]
    fn thinking_before_a_tool_call_survives_into_the_transcript() {
        let mut b = ItemBuilder::default();
        b.reasoning("weighing the options");
        b.tool_start("bash", "{}");
        b.tool_result("bash", "ok");
        b.chunk("done");
        b.done();

        let items = b.take_items();
        assert_eq!(items.len(), 3);
        assert_eq!(items[0]["type"], "assistant");
        assert_eq!(items[0]["content"], "");
        assert_eq!(items[0]["reasoning"], "weighing the options");
        assert_eq!(items[1]["type"], "tool");
        // The thought belongs to the round that produced the call, not to the
        // answer written after the tool returned.
        assert_eq!(items[2]["content"], "done");
        assert!(items[2].get("reasoning").is_none());
    }

    /// Each round's thinking must stay next to the tool calls it produced. The
    /// builder used to leave a previous round's calls in the pending buffer
    /// while the next round's reasoning accumulated, then commit them all as one
    /// item after every thought item — "all thinking, then all tools" in both
    /// the live view and the reloaded transcript.
    #[test]
    fn thinking_stays_adjacent_to_its_own_tool_round() {
        let mut b = ItemBuilder::default();
        b.reasoning("round one");
        b.tool_start("bash", "{}");
        b.tool_result("bash", "ok");
        b.reasoning("round two");
        b.tool_start("bash", "{}");
        b.tool_result("bash", "ok");
        b.chunk("answer");
        b.done();

        let items = b.take_items();
        let kinds: Vec<&str> = items.iter().map(|i| i["type"].as_str().unwrap()).collect();
        assert_eq!(kinds, ["assistant", "tool", "assistant", "tool", "assistant"]);
        assert_eq!(items[0]["reasoning"], "round one");
        assert_eq!(items[1]["toolCalls"][0]["name"], "bash");
        assert_eq!(items[2]["reasoning"], "round two");
        assert_eq!(items[3]["toolCalls"][0]["name"], "bash");
        assert_eq!(items[4]["content"], "answer");
    }

    /// A round with neither text nor thinking adds nothing (the empty-stream
    /// case, and every no-op `commit_text` between tool calls).
    #[test]
    fn an_empty_round_commits_no_item() {
        let mut b = ItemBuilder::default();
        b.done();
        assert!(b.take_items().is_empty());
    }
}
