use genai::chat::{ChatMessage, ChatRole};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Mutex;

use crate::config::AiConfig;
use crate::item::ItemBuilder;
use crate::mcp::McpStore;
use crate::system_prompt::SystemPrompt;
use crate::tools::ToolContext;

mod agent_loop;
#[cfg(test)]
mod tests;

use agent_loop::run_agent_loop;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "event", content = "data")]
pub enum StreamEvent {
    Chunk { text: String },
    /// Chain-of-thought from a reasoning model — the `reasoning_content` /
    /// `reasoning` delta field, or text peeled out of inline
    /// `<think>` tags. Kept on its own channel so the UI can show it in a
    /// collapsed "thinking" block instead of mixing it into the answer.
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
        #[serde(rename = "totalTokens")]
        total_tokens: u64,
        #[serde(rename = "contextWindow")]
        context_window: u32,
    },
    Done {},
    Error { message: String },
}

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

/// What a completed agent run produced: the assistant's final text, and the
/// full conversation to persist (the freshly-built system block stripped).
/// Callers store `messages` verbatim and never inspect it — that's what keeps
/// thought signatures and tool rounds intact.
#[derive(Debug, Clone, Serialize)]
pub struct ChatOutcome {
    pub text: String,
    pub messages: Vec<Value>,
}

/// Where one agent run's stream events go. Implementations either forward them
/// to an interface or collect them.
///
/// Sinks that persist a display transcript implement `items()`; the default
/// methods then fold each event into that builder before emitting it. Only
/// sinks with a genuinely different delivery order (the chat turn's `seq`
/// bookkeeping) implement the `send_*` methods themselves.
pub trait ChatEventSink: Send + Sync {
    /// The transcript builder this sink folds into, if it persists one.
    fn items(&self) -> Option<&Mutex<ItemBuilder>> {
        None
    }

    /// Deliver one event. The single required method.
    fn emit(&self, event: StreamEvent);

    fn send_chunk(&self, text: &str) {
        if let Some(items) = self.items() {
            items.lock().unwrap().chunk(text);
        }
        self.emit(StreamEvent::Chunk { text: text.to_string() });
    }

    fn send_reasoning(&self, text: &str) {
        if let Some(items) = self.items() {
            items.lock().unwrap().reasoning(text);
        }
        self.emit(StreamEvent::Reasoning { text: text.to_string() });
    }

    fn send_tool_start(&self, name: &str, arguments: &str) {
        if let Some(items) = self.items() {
            items.lock().unwrap().tool_start(name, arguments);
        }
        self.emit(StreamEvent::ToolStart {
            name: name.to_string(),
            arguments: arguments.to_string(),
        });
    }

    fn send_tool_result(&self, name: &str, result: &str) {
        if let Some(items) = self.items() {
            items.lock().unwrap().tool_result(name, result);
        }
        self.emit(StreamEvent::ToolResult {
            name: name.to_string(),
            result: result.to_string(),
        });
    }

    fn send_image(&self, data_url: &str) {
        if let Some(items) = self.items() {
            items.lock().unwrap().image(data_url);
        }
        self.emit(StreamEvent::Image { data_url: data_url.to_string() });
    }

    fn send_usage(&self, total_tokens: u64, context_window: u32) {
        self.emit(StreamEvent::Usage { total_tokens, context_window });
    }

    fn send_done(&self) {
        if let Some(items) = self.items() {
            items.lock().unwrap().done();
        }
        self.emit(StreamEvent::Done {});
    }

    fn send_error(&self, message: &str) {
        if let Some(items) = self.items() {
            items.lock().unwrap().error(message);
        }
        self.emit(StreamEvent::Error { message: message.to_string() });
    }
}

/// The sink for non-streaming callers (Telegram, heartbeats, sub-agents). The
/// final assistant text is returned by `run_agent` directly, so all streaming
/// events are discarded — except images a tool surfaces (e.g. `screenshot`),
/// which are buffered so a caller that can render them (Telegram) may forward
/// them afterwards via `take_images()`. Callers that don't care about images
/// (heartbeats, sub-agents) simply never call `take_images`.
#[derive(Default)]
pub struct ImageCollectingSink {
    images: Mutex<Vec<String>>,
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
    fn emit(&self, _event: StreamEvent) {}
    fn send_image(&self, data_url: &str) {
        self.images.lock().unwrap().push(data_url.to_string());
    }
}

/// Everything one agent run needs. Every caller — the chat turn runner, group
/// chat, swarm, heartbeats, sub-agents and Telegram — goes through this, so
/// history normalization, system-prompt assembly and stripping the system
/// block on the way out happen in exactly one place.
pub struct AgentRun<'a> {
    /// Persisted conversation history, WITHOUT a live system block (it is
    /// rebuilt from the prompt and the current memory files on every run).
    pub conv: Vec<Value>,
    /// The user input to append; `None` when the input is already in `conv`
    /// (group injections) or the run has no user turn of its own.
    pub turn: Option<UserTurn>,
    /// Which system block fronts this run.
    pub prompt: SystemPrompt<'a>,
    pub sink: &'a dyn ChatEventSink,
    pub config: &'a AiConfig,
    pub mcp_store: &'a McpStore,
    pub ctx: &'a ToolContext,
    /// Keep at most this many conversation turns (starting at a `user`
    /// message). `None` leaves the history as stored.
    pub max_turns: Option<usize>,
}

/// Run one agent to completion with tool calling. Returns the final assistant
/// text and the conversation to persist (system block stripped).
pub async fn run_agent(run: AgentRun<'_>) -> Result<ChatOutcome, String> {
    let AgentRun { conv, turn, prompt, sink, config, mcp_store, ctx, max_turns } = run;

    if let Some(turn) = &turn {
        ctx.log(&format!(
            "Chat request: model={}, user=\"{}\"",
            config.model, turn.text
        ));
    }

    // Normalize the incoming history once: anything that no longer parses is
    // dropped here rather than being silently skipped on every request while
    // living on in the session file forever. What gets stored is exactly what
    // gets sent.
    let mut messages: Vec<ChatMessage> = crate::llm::load_messages(&conv);
    if let Some(turn) = &turn {
        messages.push(crate::llm::user_message(&turn.text, &turn.images));
    }
    if let Some(max_turns) = max_turns {
        crate::session::trim_for_context(&mut messages, max_turns);
    }
    // Rebuild the system prompt (persona + long-term memory + tool guidance)
    // from the current memory files on every run, so edits the pet makes to
    // USER.md / MEMORY.md take effect immediately.
    prompt.prepend(&mut messages);
    // The leading system block is rebuilt per run and stripped before the
    // conversation is stored, so item ranges must ignore it.
    let system_prefix = messages
        .iter()
        .take_while(|m| m.role == ChatRole::System)
        .count();

    let preview = crate::llm::preview(&messages);
    let (text, messages) =
        run_agent_loop(messages, system_prefix, &preview, sink, config, mcp_store, ctx).await?;

    // Drop the leading system block before handing the conversation back. It is
    // rebuilt from the memory files on every run, so storing it would stack up
    // another copy of the tool-usage prompt per turn.
    let messages = messages
        .into_iter()
        .skip_while(|m| m.role == ChatRole::System)
        .map(|m| crate::llm::store_message(&m))
        .collect();
    Ok(ChatOutcome { text, messages })
}
