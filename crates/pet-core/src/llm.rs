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

/// Normalize a configured API base into a genai `Endpoint`.
///
/// genai's adapters append their route to `base_url` in two incompatible ways:
/// the OpenAI model listing concatenates it raw (`{base_url}models`), while
/// chat resolves it as a relative URL (`Url::join("chat/completions")`). Both
/// only agree with the configured base when it ends in `/`. Without one,
/// `https://gw.example.com` lists models from `https://gw.example.commodels`
/// (unresolvable host) and `https://gw.example.com/v1` chats against
/// `https://gw.example.com/chat/completions` — the join eats the last segment.
///
/// Only the trailing slash is added. Guessing a missing `/v1` would break the
/// Anthropic route (`{base_url}messages`) and any gateway that isn't versioned.
pub fn endpoint(base: &str) -> Endpoint {
    Endpoint::from_owned(format!("{}/", base.trim().trim_end_matches('/')))
}

/// Resolve an agent's config to a concrete genai target: which protocol to
/// speak, where to send it, and with what key. Passing a full `ServiceTarget`
/// (rather than a model name) bypasses genai's model-name inference entirely.
pub fn service_target(config: &AiConfig) -> ServiceTarget {
    ServiceTarget {
        endpoint: endpoint(&config.base_url),
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
///
/// Reasoning goes out as genai's `ReasoningEffort` and nothing else. A numeric
/// budget is an Anthropic/Gemini concept with no field in the OpenAI protocol,
/// so those adapters drop it — the fix is to select the native provider, not to
/// smuggle a `thinking` object into an OpenAI payload. Settings warns about the
/// combination (see `provider::renders_reasoning_budget`).
pub fn chat_options(config: &AiConfig) -> ChatOptions {
    let mut opts = ChatOptions::default()
        .with_capture_usage(true)
        .with_capture_content(true)
        .with_capture_tool_calls(true)
        .with_capture_reasoning_content(true)
        // Peels inline `<think>…</think>` into the reasoning channel, which we
        // used to do by hand for models that inline it (DeepSeek-R1, Kimi).
        .with_normalize_reasoning_content(true);

    if let Some(effort) = reasoning_effort(&config.reasoning) {
        opts = opts.with_reasoning_effort(effort);
    }
    opts
}

/// Parse the configured reasoning control. A bare number is a thinking budget;
/// anything else is one of genai's effort keywords.
fn reasoning_effort(reasoning: &str) -> Option<ReasoningEffort> {
    let reasoning = reasoning.trim();
    if reasoning.is_empty() {
        return None;
    }
    if let Ok(budget) = reasoning.parse::<u32>() {
        return Some(ReasoningEffort::Budget(budget));
    }
    let effort = ReasoningEffort::from_keyword(&reasoning.to_ascii_lowercase());
    if effort.is_none() {
        // Send nothing rather than guess. Silently dropping a typo'd value
        // would hide it until someone wondered why the model stopped thinking.
        eprintln!("WARN: unknown reasoning value {reasoning:?}, ignoring");
    }
    effort
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

use genai::chat::{Binary, ChatRole, ContentPart, MessageContent, Tool};
use serde_json::Value;

/// Sessions store their LLM history as JSON (`Session::messages`) so the rest
/// of the app can pass it around as opaque `Vec<Value>` without depending on
/// genai's types. The JSON is a serialized genai `ChatMessage`, which is what
/// carries thought signatures and provider-opaque parts across turns.
///
/// Nothing outside this module constructs these: callers describe a turn with
/// `user_message` / `ChatMessage::assistant` and hand it here. That is the
/// point — genai's wire shape stays an implementation detail of `llm`.
pub fn store_message(msg: &ChatMessage) -> Value {
    serde_json::to_value(msg).unwrap_or(Value::Null)
}

/// Rehydrate stored history. Entries that don't deserialize are dropped rather
/// than failing the turn: a single unreadable message should cost its own
/// context, not the whole conversation.
pub fn load_messages(stored: &[Value]) -> Vec<ChatMessage> {
    stored
        .iter()
        .filter_map(|v| serde_json::from_value::<ChatMessage>(v.clone()).ok())
        .collect()
}

/// Whether a stored message has the given role, without deserializing the
/// whole message.
fn stored_role_is(v: &Value, role: ChatRole) -> bool {
    v.get("role").and_then(|r| r.as_str()) == Some(&role.to_string())
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

    /// A budget only reaches the wire on adapters with a native field for it;
    /// on the OpenAI protocols genai drops it. Nothing here tries to smuggle it
    /// through anyway — Settings flags the combination instead, so the config
    /// stays a faithful description of what gets sent.
    #[test]
    fn only_native_protocols_can_express_a_token_budget() {
        use crate::provider::{kind, renders_reasoning_budget};
        assert!(renders_reasoning_budget(kind("anthropic", "claude-sonnet-4-6")));
        assert!(renders_reasoning_budget(kind("gemini", "gemini-3-pro")));
        assert!(!renders_reasoning_budget(kind("openai", "claude-sonnet-4-6")));
        assert!(!renders_reasoning_budget(kind("openai_resp", "gpt-5.6")));
    }

    /// genai appends its routes to `base_url` with no separator of its own, so
    /// the trailing slash is what keeps a configured base pointing at the real
    /// endpoint. Both shapes below came from a working config and both broke on
    /// the raw base: the first listed models from `…commodels` (unresolvable),
    /// the second lost `/v1` on every chat request (`Url::join` drops the last
    /// segment of a slashless path).
    #[test]
    fn base_url_gets_the_trailing_slash_genai_routes_depend_on() {
        assert_eq!(endpoint("https://gw.example.com").base_url(), "https://gw.example.com/");
        assert_eq!(endpoint(" https://gw.example.com/v1 ").base_url(), "https://gw.example.com/v1/");
        // An already-correct base must come out unchanged.
        assert_eq!(endpoint("https://api.openai.com/v1/").base_url(), "https://api.openai.com/v1/");
        // A missing version segment is never guessed: `/v1` is an OpenAI-ism the
        // Anthropic route and unversioned gateways don't share.
        assert!(!endpoint("https://gw.example.com").base_url().contains("v1"));
    }

    #[test]
    fn reasoning_keywords_and_budgets_parse() {
        assert!(matches!(reasoning_effort("1024"), Some(ReasoningEffort::Budget(1024))));
        assert!(matches!(reasoning_effort(" Medium "), Some(ReasoningEffort::Medium)));
        assert!(matches!(reasoning_effort("max"), Some(ReasoningEffort::Max)));
        assert!(reasoning_effort("").is_none());
        // A typo must not silently become some other effort level.
        assert!(reasoning_effort("hihg").is_none());
    }

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
    fn unreadable_history_is_dropped_not_carried() {
        // An entry that can't be sent must not survive a load: keeping it in
        // storage would be invisible dead weight that grows forever.
        let stored = vec![
            serde_json::json!({"role": "user", "content": "not a genai message"}),
            store_message(&ChatMessage::user("current")),
            serde_json::json!({"nonsense": true}),
        ];
        let msgs = load_messages(&stored);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].content.texts(), vec!["current"]);
    }

    #[test]
    fn stored_genai_messages_round_trip() {
        // Storage is the only history format; a message must survive the
        // save/load round-trip with its parts intact.
        let original = user_message("hi", &["data:image/png;base64,QUJD".to_string()]);
        let loaded = load_messages(&[store_message(&original)]);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].content.texts(), vec!["hi"]);
        assert_eq!(loaded[0].content.binaries().len(), 1);
    }
}
