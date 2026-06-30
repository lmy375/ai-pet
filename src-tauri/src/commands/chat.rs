use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::State;

use crate::commands::debug::{write_llm_log, LogStore};
use crate::commands::shell::ShellStore;
use crate::config::AiConfig;
use crate::mcp::McpManagerStore;
use crate::tools::ToolContext;
use crate::tools::ToolRegistry;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: serde_json::Value, // string or null
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

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

/// Implementation for Tauri's Channel (used by the frontend streaming path).
impl ChatEventSink for Channel<StreamEvent> {
    fn send_chunk(&self, text: &str) {
        let _ = self.send(StreamEvent::Chunk { text: text.to_string() });
    }
    fn send_reasoning(&self, text: &str) {
        let _ = self.send(StreamEvent::Reasoning { text: text.to_string() });
    }
    fn send_tool_start(&self, name: &str, arguments: &str) {
        let _ = self.send(StreamEvent::ToolStart { name: name.to_string(), arguments: arguments.to_string() });
    }
    fn send_tool_result(&self, name: &str, result: &str) {
        let _ = self.send(StreamEvent::ToolResult { name: name.to_string(), result: result.to_string() });
    }
    fn send_image(&self, data_url: &str) {
        let _ = self.send(StreamEvent::Image { data_url: data_url.to_string() });
    }
    fn send_usage(&self, prompt_tokens: u64, total_tokens: u64, context_window: u32) {
        let _ = self.send(StreamEvent::Usage { prompt_tokens, total_tokens, context_window });
    }
    fn send_done(&self) {
        let _ = self.send(StreamEvent::Done {});
    }
    fn send_error(&self, message: &str) {
        let _ = self.send(StreamEvent::Error { message: message.to_string() });
    }
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

/// Length of the longest suffix of `s` that is a (proper) prefix of `tag`.
/// Used to hold back a few trailing chars that might be the start of a `<think>`
/// tag split across stream chunks. `tag` is ASCII, so byte-prefix == char-prefix.
fn partial_tag_suffix(s: &str, tag: &str) -> usize {
    let max = (tag.len() - 1).min(s.len());
    (1..=max)
        .rev()
        .find(|&k| s.is_char_boundary(s.len() - k) && s.as_bytes()[s.len() - k..] == tag.as_bytes()[..k])
        .unwrap_or(0)
}

/// Streaming splitter for models that inline chain-of-thought as
/// `<think>…</think>` in the `content` field (e.g. some QwQ/local builds).
/// Feed it content deltas; it routes text to either the visible answer or the
/// reasoning channel, tolerating tags that straddle chunk boundaries.
#[derive(Default)]
struct ThinkSplitter {
    in_think: bool,
    pending: String,
}

impl ThinkSplitter {
    const OPEN: &'static str = "<think>";
    const CLOSE: &'static str = "</think>";

    /// Push a content delta; returns `(visible_answer, reasoning)`.
    fn push(&mut self, text: &str) -> (String, String) {
        self.pending.push_str(text);
        self.drain(false)
    }

    /// Flush at stream end — no further text can complete a partial tag, so
    /// whatever is buffered is emitted verbatim to the current channel.
    fn finish(&mut self) -> (String, String) {
        self.drain(true)
    }

    fn drain(&mut self, eof: bool) -> (String, String) {
        let (mut answer, mut reasoning) = (String::new(), String::new());
        loop {
            let tag = if self.in_think { Self::CLOSE } else { Self::OPEN };
            if let Some(pos) = self.pending.find(tag) {
                let before: String = self.pending.drain(..pos).collect();
                self.pending.drain(..tag.len()); // discard the tag itself
                if self.in_think { reasoning.push_str(&before) } else { answer.push_str(&before) }
                self.in_think = !self.in_think;
            } else {
                // No complete tag yet: emit everything except a trailing run that
                // could be the prefix of a not-yet-finished tag (unless at EOF).
                let keep = if eof { 0 } else { partial_tag_suffix(&self.pending, tag) };
                let emit: String = self.pending.drain(..self.pending.len() - keep).collect();
                if self.in_think { reasoning.push_str(&emit) } else { answer.push_str(&emit) }
                break;
            }
        }
        (answer, reasoning)
    }
}

/// Result from a streaming LLM request
struct LlmResult {
    text: String,
    /// Accumulated chain-of-thought (reasoning_content + peeled `<think>`),
    /// empty for non-reasoning models.
    reasoning: String,
    tool_calls: Vec<serde_json::Value>,
    request_time: String,
    first_token_time: Option<String>,
    done_time: String,
    first_token_latency_ms: Option<i64>,
    total_latency_ms: i64,
    /// Token usage from the API's final `usage` chunk (requires
    /// `stream_options.include_usage`). `None` if the provider omits it.
    prompt_tokens: Option<u64>,
    total_tokens: Option<u64>,
}

/// Make a streaming LLM request; returns LlmResult with timing info
async fn stream_llm_request(
    client: &reqwest::Client,
    url: &str,
    api_key: &str,
    body: &serde_json::Value,
    sink: &dyn ChatEventSink,
    ctx: &ToolContext,
) -> Result<LlmResult, String> {
    let request_time_str = crate::common::iso_now();
    let request_instant = std::time::Instant::now();

    let response = crate::common::with_bearer(
        client.post(url).header("Content-Type", "application/json").json(body),
        api_key,
    )
        .send()
        .await
        .map_err(|e| {
            ctx.log(&format!("ERROR: request failed: {}", e));
            e.to_string()
        })?;

    let status = response.status();
    ctx.log(&format!("Response status: {}", status));

    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        let msg = format!("API error {}: {}", status, text);
        ctx.log(&format!("ERROR: {}", msg));
        sink.send_error(&msg);
        return Err(msg);
    }

    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    let mut collected_text = String::new();
    let mut collected_reasoning = String::new();
    let mut splitter = ThinkSplitter::default();
    let mut tool_calls_map: std::collections::HashMap<i64, (String, String, String)> =
        std::collections::HashMap::new();
    let mut first_token_instant: Option<std::time::Instant> = None;
    let mut first_token_time_str: Option<String> = None;
    let mut prompt_tokens: Option<u64> = None;
    let mut total_tokens: Option<u64> = None;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(pos) = buffer.find('\n') {
            let line = buffer[..pos].trim().to_string();
            buffer = buffer[pos + 1..].to_string();

            if let Some(data) = line.strip_prefix("data: ") {
                if data == "[DONE]" {
                    break;
                }
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(data) {
                    // The final usage chunk (from stream_options.include_usage)
                    // carries `usage` with empty `choices`. Capture it for the
                    // context-occupancy ring.
                    if let Some(u) = parsed.get("usage").filter(|u| u.is_object()) {
                        prompt_tokens = u["prompt_tokens"].as_u64();
                        total_tokens = u["total_tokens"].as_u64();
                    }

                    // Record first token time on the first meaningful data chunk
                    if first_token_instant.is_none() {
                        first_token_instant = Some(std::time::Instant::now());
                        first_token_time_str = Some(crate::common::iso_now());
                    }

                    let delta = &parsed["choices"][0]["delta"];

                    // Reasoning models stream thought in a separate delta field,
                    // and providers disagree on its name: DeepSeek-R1 / MiniMax use
                    // `reasoning_content`; some NVIDIA-hosted builds (e.g. Kimi) use
                    // `reasoning`. Accept either (a null `reasoning` → as_str None).
                    let rtext = delta["reasoning_content"]
                        .as_str()
                        .or_else(|| delta["reasoning"].as_str());
                    if let Some(rtext) = rtext {
                        if !rtext.is_empty() {
                            collected_reasoning.push_str(rtext);
                            sink.send_reasoning(rtext);
                        }
                    }

                    if let Some(text) = delta["content"].as_str() {
                        if !text.is_empty() {
                            // Peel any inline <think>…</think> into the reasoning
                            // channel; the rest is the visible answer.
                            let (answer, reasoning) = splitter.push(text);
                            if !reasoning.is_empty() {
                                collected_reasoning.push_str(&reasoning);
                                sink.send_reasoning(&reasoning);
                            }
                            if !answer.is_empty() {
                                collected_text.push_str(&answer);
                                sink.send_chunk(&answer);
                            }
                        }
                    }

                    if let Some(tcs) = delta["tool_calls"].as_array() {
                        for tc in tcs {
                            let idx = tc["index"].as_i64().unwrap_or(0);
                            let entry = tool_calls_map
                                .entry(idx)
                                .or_insert_with(|| (String::new(), String::new(), String::new()));
                            if let Some(id) = tc["id"].as_str() {
                                entry.0.push_str(id);
                            }
                            if let Some(name) = tc["function"]["name"].as_str() {
                                entry.1.push_str(name);
                            }
                            if let Some(args) = tc["function"]["arguments"].as_str() {
                                entry.2.push_str(args);
                            }
                        }
                    }
                }
            }
        }
    }

    // Flush any text the splitter was holding back as a possible partial tag.
    let (answer, reasoning) = splitter.finish();
    if !reasoning.is_empty() {
        collected_reasoning.push_str(&reasoning);
        sink.send_reasoning(&reasoning);
    }
    if !answer.is_empty() {
        collected_text.push_str(&answer);
        sink.send_chunk(&answer);
    }

    let done_instant = std::time::Instant::now();
    let done_time_str = crate::common::iso_now();

    let first_token_latency_ms = first_token_instant
        .map(|ft| (ft - request_instant).as_millis() as i64);
    let total_latency_ms = (done_instant - request_instant).as_millis() as i64;

    let mut tool_calls: Vec<(i64, serde_json::Value)> = tool_calls_map
        .into_iter()
        .map(|(idx, (id, name, args))| {
            (
                idx,
                serde_json::json!({
                    "id": id,
                    "type": "function",
                    "function": { "name": name, "arguments": args }
                }),
            )
        })
        .collect();
    tool_calls.sort_by_key(|(idx, _)| *idx);
    let tool_calls: Vec<serde_json::Value> = tool_calls.into_iter().map(|(_, v)| v).collect();

    Ok(LlmResult {
        text: collected_text,
        reasoning: collected_reasoning,
        tool_calls,
        request_time: request_time_str,
        first_token_time: first_token_time_str,
        done_time: done_time_str,
        first_token_latency_ms,
        total_latency_ms,
        prompt_tokens,
        total_tokens,
    })
}

/// Run the full LLM chat pipeline with tool calling. Returns final assistant text.
/// This is the core logic shared by the Tauri command and Telegram bot.
pub async fn run_chat_pipeline(
    messages: Vec<ChatMessage>,
    sink: &dyn ChatEventSink,
    config: &AiConfig,
    mcp_store: &McpManagerStore,
    ctx: &ToolContext,
) -> Result<String, String> {
    let user_msg = messages
        .iter()
        .rev()
        .find(|m| m.role == "user")
        .and_then(|m| m.content.as_str())
        .unwrap_or_default();
    ctx.log(&format!("Chat request: model={}, user=\"{}\"", config.model, user_msg));

    // Build initial messages
    let mut conv_messages: Vec<serde_json::Value> = messages
        .iter()
        .map(|m| {
            let mut msg = serde_json::json!({ "role": m.role, "content": m.content });
            if let Some(ref tc) = m.tool_calls {
                msg["tool_calls"] = serde_json::json!(tc);
            }
            if let Some(ref id) = m.tool_call_id {
                msg["tool_call_id"] = serde_json::json!(id);
            }
            if let Some(ref name) = m.name {
                msg["name"] = serde_json::json!(name);
            }
            msg
        })
        .collect();

    // Rebuild the system prompt (persona + long-term memory + tool guidance)
    // from the current memory files on every turn, so edits the pet makes to
    // USER.md / MEMORY.md take effect immediately instead of being frozen at
    // session creation.
    crate::commands::prompt::prepend_system_messages(&mut conv_messages, &config.agent_id);

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
    let registry =
        ToolRegistry::new(mcp_defs, ctx.depth, ctx.is_heartbeat, web_search_enabled, ctx.is_group);
    let client = crate::common::http_client();
    let url = crate::common::openai_endpoint(&config.base_url, "chat/completions");
    let tools = registry.definitions();

    // Tool calling loop (unlimited rounds)
    let mut round = 0usize;
    loop {
        ctx.log(&format!("LLM round {} ({} messages)", round, conv_messages.len()));

        let body = serde_json::json!({
            "model": config.model,
            "stream": true,
            "stream_options": { "include_usage": true },
            "messages": conv_messages,
            "tools": tools,
        });

        ctx.log(&format!("POST {}", url));
        let result =
            stream_llm_request(&client, &url, &config.api_key, &body, sink, ctx).await?;

        // Surface this round's token usage to the UI's context ring. The frontend
        // keeps the latest, so the final round (fullest context) wins.
        if let (Some(prompt), Some(total)) = (result.prompt_tokens, result.total_tokens) {
            sink.send_usage(prompt, total, config.context_window);
        }

        if !result.reasoning.is_empty() {
            ctx.log(&format!("Reasoning ({} chars)", result.reasoning.len()));
        }

        // Write LLM request/response to llm.log with timing
        write_llm_log(
            &ctx.log_session,
            round,
            &body,
            &result.text,
            &result.tool_calls,
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
                conv_messages.push(serde_json::json!({
                    "role": "assistant",
                    "content": result.text.clone(),
                }));
            }
            return Ok((result.text, conv_messages));
        }

        ctx.log(&format!("Tool calls: {}", result.tool_calls.len()));

        // Add assistant message with tool_calls
        let text = result.text;
        let tool_calls = result.tool_calls;
        let mut assistant_msg = serde_json::json!({
            "role": "assistant",
            "content": if text.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(text) },
        });
        assistant_msg["tool_calls"] = serde_json::json!(tool_calls);
        conv_messages.push(assistant_msg);

        // Execute each tool call via registry or MCP manager
        for tc in &tool_calls {
            let tc_id = tc["id"].as_str().unwrap_or("");
            let tc_name = tc["function"]["name"].as_str().unwrap_or("");
            let tc_args = tc["function"]["arguments"].as_str().unwrap_or("{}");

            sink.send_tool_start(tc_name, tc_args);

            let result = if registry.is_mcp_tool(tc_name) {
                // Route to MCP manager
                ctx.log(&format!("MCP tool call: {}({})", tc_name, tc_args));
                let args_value: serde_json::Value =
                    serde_json::from_str(tc_args).unwrap_or(serde_json::Value::Null);
                let managers = mcp_store.lock().await;
                let call_res = match managers.get(&config.agent_id) {
                    Some(m) => m.call_tool(tc_name, args_value).await,
                    None => Err(format!("No MCP manager for agent {}", config.agent_id)),
                };
                match call_res {
                    Ok(r) => r,
                    Err(e) => crate::tools::tool_error(e),
                }
            } else {
                // Built-in tool
                registry.execute(tc_name, tc_args, ctx).await
            };

            ctx.log(&format!("Tool result [{}]: {} chars", tc_name, result.len()));

            sink.send_tool_result(tc_name, &result);

            conv_messages.push(serde_json::json!({
                "role": "tool",
                "tool_call_id": tc_id,
                "content": result,
            }));
        }

        // Some tools (e.g. `screenshot`) produce an image the model must actually
        // SEE — a `tool` message can't carry one, so they queue a data URL on the
        // context. Drain it here, after every `tool` message for this round is in
        // place (keeping them contiguous for tool_call_id pairing), and append the
        // images as a `user` message — the same multimodal path used for pastes.
        let imgs = ctx.take_images();
        if !imgs.is_empty() {
            // Surface each image to the UI so it renders as an image bubble —
            // the frontend never sees `conv_messages`, only stream events.
            for url in &imgs {
                sink.send_image(url);
            }
            let content: Vec<serde_json::Value> = imgs
                .iter()
                .map(|url| serde_json::json!({"type": "image_url", "image_url": {"url": url}}))
                .collect();
            conv_messages.push(serde_json::json!({"role": "user", "content": content}));
        }

        round += 1;
    }
}

/// Emits background-task completions so the conversation can be resumed
/// automatically (see `useChat`'s `background-finished` listener).
///
/// Targets the ACTIVE window only (pet or panel — they share one conversation),
/// so the completion is injected into the window the user is looking at and never
/// into both. Both windows listen; backend routing guarantees a single delivery.
struct TauriNotifier {
    app: tauri::AppHandle,
}

impl crate::commands::shell::TaskNotifier for TauriNotifier {
    fn notify(&self, completion: &crate::commands::shell::TaskCompletion) {
        use tauri::Emitter;
        let label = crate::commands::window::active_window_label(&self.app);
        // If the target window is gone the task still stays in the store
        // (queryable via check_task_status); log rather than silently drop.
        if let Err(e) = self.app.emit_to(&label, "background-finished", completion.clone()) {
            eprintln!("failed to emit background-finished for task {}: {}", completion.task_id, e);
        }
    }
}

#[tauri::command]
pub async fn chat(
    messages: Vec<ChatMessage>,
    on_event: Channel<StreamEvent>,
    session_id: String,
    app: tauri::AppHandle,
    log_store: State<'_, LogStore>,
    shell_store: State<'_, ShellStore>,
    mcp_store: State<'_, McpManagerStore>,
) -> Result<(), String> {
    let config = AiConfig::from_settings()?;
    let mcp = mcp_store.inner().clone();
    let notifier: std::sync::Arc<dyn crate::commands::shell::TaskNotifier> =
        std::sync::Arc::new(TauriNotifier { app: app.clone() });
    let ctx = ToolContext::from_states(
        &log_store,
        &shell_store,
        config.clone(),
        mcp.clone(),
        session_id,
        Some(notifier),
        Some(app),
    );
    run_chat_pipeline(messages, &on_event, &config, &mcp, &ctx).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::ThinkSplitter;

    /// Drive a splitter chunk-by-chunk and concatenate each channel.
    fn split(chunks: &[&str]) -> (String, String) {
        let mut s = ThinkSplitter::default();
        let (mut answer, mut reasoning) = (String::new(), String::new());
        for c in chunks {
            let (a, r) = s.push(c);
            answer.push_str(&a);
            reasoning.push_str(&r);
        }
        let (a, r) = s.finish();
        answer.push_str(&a);
        reasoning.push_str(&r);
        (answer, reasoning)
    }

    #[test]
    fn no_think_tags_is_all_answer() {
        assert_eq!(split(&["hello world"]), ("hello world".into(), String::new()));
    }

    #[test]
    fn whole_think_block_then_answer() {
        assert_eq!(
            split(&["<think>reasoning here</think>the answer"]),
            ("the answer".into(), "reasoning here".into())
        );
    }

    #[test]
    fn open_tag_split_across_chunks() {
        // The "<thi" + "nk>" boundary must not leak into the visible answer.
        assert_eq!(
            split(&["<thi", "nk>secret</think>", "visible"]),
            ("visible".into(), "secret".into())
        );
    }

    #[test]
    fn close_tag_split_across_chunks() {
        assert_eq!(
            split(&["<think>mid", "dle</thi", "nk>done"]),
            ("done".into(), "middle".into())
        );
    }

    #[test]
    fn lone_lt_in_answer_is_not_held_forever() {
        // A partial-tag suffix with no following tag must flush at EOF.
        assert_eq!(split(&["1 < 2 and 3 < 4"]), ("1 < 2 and 3 < 4".into(), String::new()));
    }
}
