use futures_util::future::join_all;
use genai::chat::{ChatMessage, ChatRequest, MessageContent, ToolCall, ToolResponse};
use serde_json::Value;

use super::ChatEventSink;
use crate::config::AiConfig;
use crate::logging::{write_llm_log, LlmLogBody, LlmLogMeta, RoundStat};
use crate::mcp::McpStore;
use crate::tools::{RunGate, ToolContext, ToolPolicy, ToolRegistry};

/// The tool-calling loop over an already-assembled message list (system prompt
/// included). Returns the final assistant text and the full conversation.
pub(super) async fn run_agent_loop(
    mut messages: Vec<ChatMessage>,
    preview: &str,
    sink: &dyn ChatEventSink,
    config: &AiConfig,
    mcp_store: &McpStore,
    ctx: &ToolContext,
) -> Result<(String, Vec<ChatMessage>), String> {
    // The run gate carries the whole context: kind (chat / heartbeat / group /
    // swarm), sub-agent depth and whether a Tavily key is configured. The owner's
    // own layer (tools switched off, descriptions rewritten) is applied by
    // `for_gate` on top and can only subtract from the gate.
    let gate = RunGate::from_context(ctx, !config.search_api_key.trim().is_empty());
    let registry =
        ToolRegistry::new(mcp_store, &config.mcp_servers, ToolPolicy::for_gate(gate)).await;
    let client = crate::llm::client();
    let options = crate::llm::chat_options(config);
    let tools = crate::llm::tools_from_definitions(&registry.definitions());

    // Tool calling loop (unlimited rounds)
    let mut round = 0usize;
    // Per-round latency for the LLM log. The log body only keeps the first and
    // final rounds, so this is where every round's timings survive.
    let mut round_stats: Vec<RoundStat> = Vec::new();
    loop {
        ctx.log(&format!("LLM round {} ({} messages)", round, messages.len()));

        let chat_req = ChatRequest::new(messages.clone()).with_tools(tools.clone());

        let result =
            crate::llm::stream_chat(&client, config, chat_req, &options, sink, ctx).await?;

        // Surface this round's token usage to the UI's context ring. The frontend
        // keeps the latest, so the final round (fullest context) wins.
        if let Some(total) = result.total_tokens {
            sink.send_usage(total, config.context_window);
        }

        if !result.reasoning.is_empty() {
            ctx.log(&format!("Reasoning ({} chars)", result.reasoning.len()));
        }

        round_stats.push(RoundStat {
            round,
            ttft_ms: result.first_token_latency_ms,
            total_ms: result.total_latency_ms,
            tools: result.tool_calls.iter().map(|tc| tc.fn_name.clone()).collect(),
        });

        // Only the first and final rounds write the whole request body: each
        // round's request is a superset of the previous one, so the final body
        // already holds every message.
        let body_messages: Option<Vec<Value>> = if round == 0 || result.tool_calls.is_empty() {
            Some(messages.iter().map(crate::llm::store_message).collect())
        } else {
            None
        };
        // Every round's calls go to the one-line `llm.log` the evals parse.
        let tool_calls: Vec<Value> = result
            .tool_calls
            .iter()
            .map(|tc| serde_json::to_value(tc).unwrap_or(Value::Null))
            .collect();
        write_llm_log(
            LlmLogMeta {
                session: &ctx.log_session,
                model: &config.model,
                rounds: &round_stats,
                preview,
                request_time: &result.request_time,
                response_text: &result.text,
                reasoning: &result.reasoning,
                tool_calls: &tool_calls,
            },
            body_messages.as_deref().map(|messages| LlmLogBody {
                messages,
                first_token_time: result.first_token_time.as_deref(),
                done_time: &result.done_time,
            }),
        );

        // An aborted round comes back with its tool calls cleared, so it takes
        // the final-response path below: whatever text streamed before the
        // stop is kept in context as an ordinary assistant message, and the
        // next turn continues from it.
        if result.cancelled {
            ctx.log(&format!("Cancelled by user ({} chars kept)", result.text.len()));
        }

        if result.tool_calls.is_empty() {
            ctx.log(&format!(
                "Final response ({} chars, TTFT={}ms, total={}ms)",
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
                messages.push(ChatMessage::assistant(result.text.clone()));
            }
            return Ok((result.text, messages));
        }

        ctx.log(&format!("Tool calls: {}", result.tool_calls.len()));

        // The assistant turn genai captured, which puts any thought signatures
        // ahead of the tool calls. Replaying it verbatim is what lets Anthropic
        // extended thinking and the Responses API survive a tool round-trip —
        // rebuilding it from `text` + `tool_calls` would drop those parts.
        if let Some(turn) = &result.assistant_turn {
            messages.push(turn.clone());
        }

        // Run every call from this assistant turn concurrently. Result order is
        // preserved below so the UI and provider transcript stay deterministic.
        let outputs = execute_tool_calls(&result.tool_calls, sink, &registry, ctx).await;
        for (tc, output) in result.tool_calls.iter().zip(outputs) {
            // `fn_name` is carried alongside `call_id` because Gemini correlates
            // tool responses by function name, not by id.
            messages.push(ChatMessage::tool(MessageContent::from_tool_responses(vec![
                ToolResponse::new(tc.call_id.clone(), output).with_fn_name(tc.fn_name.clone()),
            ])));
        }

        // Some tools (e.g. `screenshot`) produce an image the model must actually
        // SEE — a `tool` message can't carry one, so they queue a data URL on the
        // context. Drain it here, after every `tool` message for this round is in
        // place (keeping them contiguous for call_id pairing), and append the
        // images as a `user` message — the same multimodal path used for pastes.
        let imgs = ctx.take_images();
        if !imgs.is_empty() {
            // Surface each image to the UI so it renders as an image bubble —
            // the frontend never sees `messages`, only stream events.
            for url in &imgs {
                sink.send_image(url);
            }
            messages.push(crate::llm::user_message("", &imgs));
        }

        if ctx.cancel.is_cancelled() {
            sink.send_done();
            return Ok((String::new(), messages));
        }

        round += 1;
    }
}

/// Execute one assistant turn's tool calls concurrently, preserving the order of
/// `tool_calls` in both the emitted result events and the returned outputs.
pub(super) async fn execute_tool_calls(
    tool_calls: &[ToolCall],
    sink: &dyn ChatEventSink,
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
        let call_ctx = ctx.for_tool_call();

        sink.send_tool_start(&name, &arguments);

        async move {
            // The registry routes built-ins to their impl and MCP tools to the
            // server that advertised them.
            let execute = registry.execute(&name, &arguments, &call_ctx);
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
