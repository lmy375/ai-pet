//! The LLM transport: everything that speaks to a provider goes through here.
//!
//! genai owns the wire protocol (OpenAI chat-completions, OpenAI Responses,
//! Anthropic, Gemini, …) and hands back provider-agnostic `ChatStreamEvent`s.
//! This module's job is the two things genai deliberately doesn't do for us:
//!
//! 1. Point it at *our* endpoint with *our* protocol choice. genai infers the
//!    adapter from the model name and falls back to Ollama when nothing
//!    matches, which is wrong for every gateway-hosted model — so we always
//!    hand it a fully-resolved `ServiceTarget` (see `crate::provider`).
//! 2. Keep the two failure signals we depend on. genai normalizes away
//!    provider-specific protocol detail; a gateway returning 200 with an empty
//!    stream therefore looks like a perfectly good empty answer. Treating that
//!    as "the model is done" silently zeroed out whole runs (6/10 DeepSWE
//!    tasks), so `stream_chat` still reports it as an error.

use crate::chat::ChatEventSink;
use crate::config::AiConfig;
use crate::tools::ToolContext;
use futures_util::StreamExt;
use genai::chat::{
    ChatMessage, ChatOptions, ChatRequest, ChatStreamEvent, ReasoningEffort, ToolCall,
};
use genai::resolver::{AuthData, Endpoint};
use genai::{Client, ModelIden, ServiceTarget};

/// The shared genai client. Holds no per-agent state — endpoint, auth and
/// adapter all arrive per request via `ServiceTarget`, so one client serves
/// every agent, the group orchestrator and the heartbeat scheduler at once.
pub fn client() -> Client {
    Client::default()
}

/// Resolve an agent's config to a concrete genai target: which protocol to
/// speak, where to send it, and with what key. Passing a full `ServiceTarget`
/// (rather than a model name) bypasses genai's model-name inference entirely.
pub fn service_target(config: &AiConfig) -> ServiceTarget {
    ServiceTarget {
        endpoint: Endpoint::from_owned(config.base_url.clone()),
        auth: AuthData::from_single(config.api_key.clone()),
        model: ModelIden::new(
            crate::provider::kind(&config.provider, &config.model),
            config.model.clone(),
        ),
    }
}

/// Per-request options. The captures are what make `StreamEnd` carry the final
/// assistant turn, which the tool loop feeds straight back into the next
/// request — that round-trip is how Anthropic thinking signatures and Responses
/// reasoning items survive multi-turn tool use.
pub fn chat_options(config: &AiConfig) -> ChatOptions {
    let mut opts = ChatOptions::default()
        .with_capture_usage(true)
        .with_capture_content(true)
        .with_capture_tool_calls(true)
        .with_capture_reasoning_content(true)
        // Peels inline `<think>…</think>` into the reasoning channel, which we
        // used to do by hand for models that inline it (DeepSeek-R1, Kimi).
        .with_normalize_reasoning_content(true);

    if let Some(effort) = reasoning_effort(config) {
        opts = opts.with_reasoning_effort(effort);
    }
    opts
}

/// Collapse the two legacy reasoning knobs onto genai's single one. An explicit
/// thinking budget wins over the effort label: it's the more specific request,
/// and Anthropic models only think when given one.
fn reasoning_effort(config: &AiConfig) -> Option<ReasoningEffort> {
    if config.thinking_enabled {
        return Some(ReasoningEffort::Budget(config.thinking_budget_tokens));
    }
    match config.reasoning_effort.trim().to_ascii_lowercase().as_str() {
        "" => None,
        "minimal" => Some(ReasoningEffort::Minimal),
        "low" => Some(ReasoningEffort::Low),
        "medium" => Some(ReasoningEffort::Medium),
        "high" => Some(ReasoningEffort::High),
        "xhigh" => Some(ReasoningEffort::XHigh),
        "max" => Some(ReasoningEffort::Max),
        "none" => Some(ReasoningEffort::None),
        other => {
            // Unknown label: send nothing rather than guess. Silently dropping
            // it would hide a typo'd config until someone wondered why the
            // model stopped thinking.
            eprintln!("WARN: unknown reasoning_effort {other:?}, ignoring");
            None
        }
    }
}

/// Result of one streaming round.
pub struct LlmResult {
    pub text: String,
    /// Accumulated chain-of-thought, empty for non-reasoning models.
    pub reasoning: String,
    pub tool_calls: Vec<ToolCall>,
    /// The assistant turn to append before the tool responses. Carries thought
    /// signatures ahead of the tool calls, in the order providers expect
    /// (Gemini 3 rejects the other order). `None` when there were no tool calls.
    pub assistant_turn: Option<ChatMessage>,
    pub request_time: String,
    pub first_token_time: Option<String>,
    pub done_time: String,
    pub first_token_latency_ms: Option<i64>,
    pub total_latency_ms: i64,
    pub prompt_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
}

/// Stream one chat round, forwarding events to `sink` as they arrive.
pub async fn stream_chat(
    client: &Client,
    config: &AiConfig,
    chat_req: ChatRequest,
    options: &ChatOptions,
    sink: &dyn ChatEventSink,
    ctx: &ToolContext,
) -> Result<LlmResult, String> {
    let request_time = crate::common::iso_now();
    let request_instant = std::time::Instant::now();

    let target = service_target(config);
    ctx.log(&format!(
        "POST {} [{}] model={}",
        target.endpoint.base_url(),
        target.model.adapter_kind,
        config.model
    ));

    let response = client
        .exec_chat_stream(target, chat_req, Some(options))
        .await
        .map_err(|e| {
            let msg = format!("LLM request failed: {e}");
            ctx.log(&format!("ERROR: {msg}"));
            sink.send_error(&msg);
            msg
        })?;

    let mut stream = response.stream;
    let mut text = String::new();
    let mut reasoning = String::new();
    let mut tool_calls: Vec<ToolCall> = Vec::new();
    let mut assistant_turn: Option<ChatMessage> = None;
    let mut prompt_tokens: Option<u64> = None;
    let mut total_tokens: Option<u64> = None;
    let mut first_token_instant: Option<std::time::Instant> = None;
    let mut first_token_time: Option<String> = None;

    let mark_first_token = |first_token_instant: &mut Option<std::time::Instant>,
                                first_token_time: &mut Option<String>| {
        if first_token_instant.is_none() {
            *first_token_instant = Some(std::time::Instant::now());
            *first_token_time = Some(crate::common::iso_now());
        }
    };

    while let Some(event) = stream.next().await {
        let event = event.map_err(|e| {
            let msg = format!("LLM stream error: {e}");
            ctx.log(&format!("ERROR: {msg}"));
            sink.send_error(&msg);
            msg
        })?;

        match event {
            ChatStreamEvent::Start => {}
            ChatStreamEvent::Chunk(chunk) => {
                if chunk.content.is_empty() {
                    continue;
                }
                mark_first_token(&mut first_token_instant, &mut first_token_time);
                text.push_str(&chunk.content);
                sink.send_chunk(&chunk.content);
            }
            ChatStreamEvent::ReasoningChunk(chunk) => {
                if chunk.content.is_empty() {
                    continue;
                }
                mark_first_token(&mut first_token_instant, &mut first_token_time);
                reasoning.push_str(&chunk.content);
                sink.send_reasoning(&chunk.content);
            }
            // Opaque provider token that has to be echoed back verbatim on the
            // next turn. Nothing to show the user; it rides along in the
            // captured assistant turn below.
            ChatStreamEvent::ThoughtSignatureChunk(_) => {}
            ChatStreamEvent::ToolCallChunk(chunk) => {
                mark_first_token(&mut first_token_instant, &mut first_token_time);
                tool_calls.push(chunk.tool_call);
            }
            ChatStreamEvent::End(end) => {
                if let Some(usage) = &end.captured_usage {
                    prompt_tokens = usage.prompt_tokens.map(|t| t.max(0) as u64);
                    total_tokens = usage.total_tokens.map(|t| t.max(0) as u64);
                }
                // Prefer the captured aggregates: genai reconciles partial
                // tool-call chunks there, so a provider that streams arguments
                // in fragments still yields whole calls.
                if let Some(captured) = end.captured_tool_calls() {
                    if !captured.is_empty() {
                        tool_calls = captured.into_iter().cloned().collect();
                    }
                }
                assistant_turn = end.into_assistant_message_for_tool_use();
            }
        }
    }

    let done_instant = std::time::Instant::now();

    // A gateway can end a stream cleanly having sent nothing at all (HTTP 200,
    // no chunks). That is a failure, not an answer — see the module header.
    if text.trim().is_empty() && reasoning.is_empty() && tool_calls.is_empty() {
        let msg = "LLM returned an empty response (no text, no tool calls)";
        ctx.log(&format!("ERROR: {msg}"));
        sink.send_error(msg);
        return Err(msg.to_string());
    }

    Ok(LlmResult {
        text,
        reasoning,
        tool_calls,
        assistant_turn,
        request_time,
        first_token_time,
        done_time: crate::common::iso_now(),
        first_token_latency_ms: first_token_instant
            .map(|ft| (ft - request_instant).as_millis() as i64),
        total_latency_ms: (done_instant - request_instant).as_millis() as i64,
        prompt_tokens,
        total_tokens,
    })
}

// region: --- Message storage & conversion

use genai::chat::{Binary, ChatRole, ContentPart, MessageContent, Tool, ToolResponse};
use serde_json::Value;

/// Sessions store their LLM history as JSON (`Session::messages`) so the rest
/// of the app — group orchestrator, Telegram, CLI — can keep passing it around
/// as opaque `Vec<Value>`. The JSON is a serialized genai `ChatMessage`, which
/// is what preserves thought signatures and provider-opaque parts across turns.
pub fn store_message(msg: &ChatMessage) -> Value {
    serde_json::to_value(msg).unwrap_or(Value::Null)
}

/// Rehydrate stored history, accepting two shapes:
///
/// - serialized genai `ChatMessage`s — what this module writes, and what
///   carries thought signatures and provider-opaque parts forward; and
/// - OpenAI wire format (`{role, content, tool_calls}`) — which is both what
///   pre-migration sessions hold AND what the frontend still appends
///   (`useChat.ts`) and Telegram builds. That is deliberate: those callers
///   describe a turn in the simplest shape, and normalizing happens here,
///   once. Do NOT delete this path as "legacy" — it is a live input format.
///
/// Visible history (`Session::items`) is a separate, provider-neutral array
/// and is untouched by any of this.
pub fn load_messages(stored: &[Value]) -> Vec<ChatMessage> {
    stored
        .iter()
        .filter_map(|v| {
            serde_json::from_value::<ChatMessage>(v.clone())
                .ok()
                .or_else(|| from_openai_message(v))
        })
        .collect()
}

/// Convert one OpenAI chat-completions message into a genai `ChatMessage`.
fn from_openai_message(v: &Value) -> Option<ChatMessage> {
    let role = match v.get("role")?.as_str()? {
        "system" | "developer" => ChatRole::System,
        "user" => ChatRole::User,
        "assistant" => ChatRole::Assistant,
        "tool" => ChatRole::Tool,
        _ => return None,
    };

    // A tool result: `{role:"tool", tool_call_id, content}`.
    if role == ChatRole::Tool {
        let call_id = v.get("tool_call_id")?.as_str()?.to_string();
        let content = v.get("content").and_then(|c| c.as_str()).unwrap_or_default();
        return Some(ChatMessage::tool(MessageContent::from_tool_responses(vec![
            ToolResponse::new(call_id, content),
        ])));
    }

    let mut parts: Vec<ContentPart> = Vec::new();
    match v.get("content") {
        Some(Value::String(text)) if !text.is_empty() => {
            parts.push(ContentPart::from_text(text.clone()))
        }
        Some(Value::Array(blocks)) => {
            for block in blocks {
                match block.get("type").and_then(|t| t.as_str()) {
                    Some("text") => {
                        if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                            parts.push(ContentPart::from_text(text));
                        }
                    }
                    Some("image_url") => {
                        if let Some(url) = block
                            .get("image_url")
                            .and_then(|i| i.get("url"))
                            .and_then(|u| u.as_str())
                        {
                            if let Some(binary) = binary_from_url(url) {
                                parts.push(ContentPart::Binary(binary));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }

    // Assistant turns can carry tool calls with no text at all.
    if let Some(calls) = v.get("tool_calls").and_then(|t| t.as_array()) {
        for call in calls {
            let fn_name = call
                .get("function")
                .and_then(|f| f.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or_default()
                .to_string();
            // OpenAI streams arguments as a JSON *string*; genai wants the
            // parsed value.
            let fn_arguments = call
                .get("function")
                .and_then(|f| f.get("arguments"))
                .and_then(|a| a.as_str())
                .and_then(|a| serde_json::from_str::<Value>(a).ok())
                .unwrap_or(Value::Null);
            parts.push(ContentPart::ToolCall(ToolCall {
                call_id: call
                    .get("id")
                    .and_then(|i| i.as_str())
                    .unwrap_or_default()
                    .to_string(),
                fn_name,
                fn_arguments,
                thought_signatures: None,
            }));
        }
    }

    if parts.is_empty() {
        return None;
    }
    Some(ChatMessage::new(role, MessageContent::from_parts(parts)))
}

/// Whether a stored message has the given role. Stored history is genai
/// `ChatMessage` JSON, but sessions written before the migration hold OpenAI
/// wire format — whose role is the lowercase `"user"` rather than `"User"` —
/// so both spellings have to count.
fn stored_role_is(v: &Value, role: ChatRole) -> bool {
    v.get("role")
        .and_then(|r| r.as_str())
        .is_some_and(|r| r.eq_ignore_ascii_case(&role.to_string()))
}

/// Whether a stored message is a user turn. Used to slice conversations into
/// turns (`session::recent_turns`).
pub fn is_user_message(v: &Value) -> bool {
    stored_role_is(v, ChatRole::User)
}

/// Whether a stored message is a system turn.
pub fn is_system_message(v: &Value) -> bool {
    stored_role_is(v, ChatRole::System)
}

/// Turn an image reference into a genai `Binary`. Accepts the `data:` URLs the
/// whole app already speaks — clipboard pastes, Telegram photos and the
/// `screenshot` tool all produce them — and passes plain http(s) URLs through
/// for the providers that accept them.
pub fn binary_from_url(url: &str) -> Option<Binary> {
    let Some(rest) = url.strip_prefix("data:") else {
        if url.starts_with("http://") || url.starts_with("https://") {
            // MIME isn't knowable without fetching; image/* covers every
            // current producer and is what providers expect for a bare URL.
            return Some(Binary::from_url("image/*", url, None));
        }
        return None;
    };
    let (mime, b64) = rest.split_once(";base64,")?;
    if mime.is_empty() || b64.is_empty() {
        return None;
    }
    Some(Binary::from_base64(mime, b64, None))
}

/// Convert the tool registry's OpenAI-shaped definitions
/// (`{type:"function", function:{name, description, parameters}}`) into genai
/// `Tool`s. genai re-renders them per provider, which is what makes Gemini's
/// stricter `functionDeclarations` dialect someone else's problem.
pub fn tools_from_definitions(defs: &Value) -> Vec<Tool> {
    let Some(items) = defs.as_array() else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|def| {
            // Accept both the wrapped OpenAI form and a bare function object,
            // since MCP servers hand us definitions in either shape.
            let f = def.get("function").unwrap_or(def);
            let name = f.get("name")?.as_str()?;
            let mut tool = Tool::new(name);
            if let Some(desc) = f.get("description").and_then(|d| d.as_str()) {
                tool = tool.with_description(desc);
            }
            if let Some(schema) = f.get("parameters") {
                tool = tool.with_schema(schema.clone());
            }
            Some(tool)
        })
        .collect()
}

/// Build a user message from text plus any attached images.
pub fn user_message(text: &str, images: &[String]) -> ChatMessage {
    if images.is_empty() {
        return ChatMessage::user(text);
    }
    let mut parts: Vec<ContentPart> = Vec::new();
    if !text.is_empty() {
        parts.push(ContentPart::from_text(text));
    }
    parts.extend(
        images
            .iter()
            .filter_map(|url| binary_from_url(url))
            .map(ContentPart::Binary),
    );
    ChatMessage::user(MessageContent::from_parts(parts))
}

// endregion: --- Message storage & conversion

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_url_splits_into_mime_and_payload() {
        let b = binary_from_url("data:image/jpeg;base64,QUJD").expect("parsed");
        assert_eq!(b.content_type, "image/jpeg");
        assert!(b.is_image());
        // A data URL missing the base64 marker must not be silently accepted as
        // an image — that would ship a broken part to the provider.
        assert!(binary_from_url("data:image/png,notbase64").is_none());
        assert!(binary_from_url("/local/path.png").is_none());
    }

    #[test]
    fn legacy_openai_history_survives_the_migration() {
        // Exactly the shapes sitting in sessions written before genai.
        let stored = vec![
            serde_json::json!({"role": "system", "content": "be nice"}),
            serde_json::json!({"role": "user", "content": [
                {"type": "text", "text": "what is this"},
                {"type": "image_url", "image_url": {"url": "data:image/png;base64,QUJD"}}
            ]}),
            serde_json::json!({"role": "assistant", "content": "", "tool_calls": [
                {"id": "call_1", "type": "function",
                 "function": {"name": "shell", "arguments": "{\"cmd\":\"ls\"}"}}
            ]}),
            serde_json::json!({"role": "tool", "tool_call_id": "call_1", "content": "a.txt"}),
        ];
        let msgs = load_messages(&stored);
        assert_eq!(msgs.len(), 4);

        assert_eq!(msgs[1].role, ChatRole::User);
        assert_eq!(msgs[1].content.texts(), vec!["what is this"]);
        assert_eq!(msgs[1].content.binaries().len(), 1);

        // The arguments string must arrive parsed, not as a JSON string.
        let calls = msgs[2].content.tool_calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].fn_name, "shell");
        assert_eq!(calls[0].fn_arguments["cmd"], "ls");

        let responses = msgs[3].content.tool_responses();
        assert_eq!(responses[0].call_id, "call_1");
        assert_eq!(responses[0].content, "a.txt");
    }

    #[test]
    fn stored_genai_messages_round_trip() {
        // New-format history must load back as itself, not fall through to the
        // legacy path (which would drop it entirely).
        let original = user_message("hi", &["data:image/png;base64,QUJD".to_string()]);
        let loaded = load_messages(&[store_message(&original)]);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].content.texts(), vec!["hi"]);
        assert_eq!(loaded[0].content.binaries().len(), 1);
    }
}
