use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter, State};

use crate::commands::debug::{write_llm_log, LogStore, ProcessCountersStore};
use crate::commands::shell::ShellStore;
use crate::config::AiConfig;
use crate::mcp::McpManagerStore;
use crate::mood::{read_current_mood_parsed, read_mood_for_event};
use crate::proactive::InteractionClockStore;
use crate::tools::ToolContext;
use crate::tools::ToolRegistry;

/// Payload emitted to the frontend after a reactive chat turn finishes. Symmetric with
/// `proactive-message` — the frontend uses `mood` and `motion` to drive Live2D.
#[derive(Clone, Serialize)]
pub struct ChatDonePayload {
    pub mood: Option<String>,
    pub motion: Option<String>,
    pub timestamp: String,
}

/// Trim conversation history to at most `max` user/assistant messages, preserving the
/// leading system messages (SOUL.md and any other anchors). When `max == 0` the gate is
/// disabled and the input is returned untouched. When the history is shorter than `max`
/// the input is also returned untouched.
///
/// Symmetric with the previous telegram-only logic, but generalized so the desktop chat
/// path can apply the same cap and prevent unbounded token growth on long conversations.
pub fn trim_to_context(mut messages: Vec<ChatMessage>, max: usize) -> Vec<ChatMessage> {
    if max == 0 {
        return messages;
    }
    // Find the boundary: index of the first non-system message.
    let first_non_system = messages
        .iter()
        .position(|m| m.role != "system")
        .unwrap_or(messages.len());
    let history_len = messages.len() - first_non_system;
    if history_len <= max {
        return messages;
    }
    // Drop the oldest user/assistant messages; keep `max` newest plus all leading systems.
    let drop_count = history_len - max;
    messages.drain(first_non_system..first_non_system + drop_count);
    messages
}

/// Insert a transient system message carrying the pet's current mood and a nudge to update
/// it after replying. Inserted right after the leading system block so it sits next to
/// SOUL.md but before any conversation history. Callers (chat tauri command, telegram bot)
/// augment only the in-memory list passed to the pipeline — persisted session storage is
/// not affected.
pub fn inject_mood_note(mut messages: Vec<ChatMessage>) -> Vec<ChatMessage> {
    let mood_section = match read_current_mood_parsed() {
        Some((text, _)) if !text.trim().is_empty() => format!(
            "[宠物当前心情/状态] {}\n\n如果这次对话让你心情有变化，可以用 `memory_edit` 更新 `ai_insights/current_mood`，description 必须以 `[motion: Tap|Flick|Flick3|Idle] 心情文字` 开头（Tap=开心活泼，Flick=想分享有兴致，Flick3=焦虑烦躁，Idle=平静低落沉静）。心情没变就不用更新。",
            text.trim()
        ),
        _ => "[宠物当前心情/状态] 还没记录过。如果对话让你产生了某种心情，可以用 `memory_edit create` 新建 `ai_insights/current_mood`，description 以 `[motion: Tap|Flick|Flick3|Idle] 心情文字` 开头。没特别感受就先不写。".to_string(),
    };

    // Tell the model how to record a user-set reminder so the proactive loop can later
    // surface it. The format must match `parse_reminder_prefix` in proactive.rs:
    // todo / description starting with "[remind: HH:MM] topic". 24-hour clock.
    let reminder_section = "\n\n[设置提醒的约定] 如果用户说类似「N 点提醒我做 X」「下午 5 点喊我下班」「30 分钟后叫我休息」「明天早上 9 点开会」之类的话，请用 `memory_edit create` 在 `todo` 类别下新建一条 memory item：\n\
- 今天内的提醒：description 以 `[remind: HH:MM] X` 开头（HH 是 24 小时制 0–23）。例：description=`[remind: 23:00] 吃药`、title=`take_meds`。\n\
- 跨天或具体日期：description 以 `[remind: YYYY-MM-DD HH:MM] X` 开头。例：description=`[remind: 2026-05-04 09:00] 项目早会`。\n\
- 相对时间（「30 分钟后」「2 小时后」等）：你需要根据当前时间换算成绝对的 HH:MM（或日期时间），不要原样写「+30m」。\n\
等时间到了，主动开口循环会把这条提醒带出来给用户。其他的「我说今晚要...」这种闲聊不算提醒，不要乱建。";

    // Plan section: cross-turn intent. Open structure — LLM owns formatting; Rust just
    // surfaces whatever is in description back into proactive prompts.
    let plan_section = "\n\n[今日计划的约定] 如果对话中你想给自己定个今天的小目标（比如「关心用户工作进展两次」「夜里 22 点提醒喝水一次」之类），可以用 `memory_edit` 在 `ai_insights` 类下 create/update `daily_plan` 条目，description 用简短的 bullet list 表达，每条带上「[已执行/目标次数]」进度标记。例：description=`· 关心工作进展 [0/2]\\n· 提醒喝水 [0/1]`、title=`daily_plan`。下次主动开口循环看到这个 plan 时会优先推进其中一项。完成的项请删掉。";

    let body = format!("{}{}{}", mood_section, reminder_section, plan_section);

    let note: ChatMessage = serde_json::from_value(serde_json::json!({
        "role": "system",
        "content": body,
    }))
    .expect("static mood note JSON should always parse");

    let insert_at = messages
        .iter()
        .position(|m| m.role != "system")
        .unwrap_or(messages.len());
    messages.insert(insert_at, note);
    messages
}

/// System prompt for tool usage best practices, injected into every chat pipeline request.
const TOOL_USAGE_PROMPT: &str = r#"# 工具使用指南

你可以使用以下工具来帮助用户完成任务。请遵循以下原则：

## 工具选择
- 读取文件内容：使用 read_file，**不要**用 bash 运行 cat/head/tail/sed
- 修改现有文件：使用 edit_file，**不要**用 bash 运行 sed/awk
- 创建新文件或完全重写文件：使用 write_file，**不要**用 bash 运行 echo 重定向或 cat heredoc
- bash 工具仅用于真正需要 shell 执行的系统命令（如 git、npm、cargo、curl、ls、find 等）

## 文件操作原则
- 在修改文件之前，先用 read_file 阅读文件内容，确保了解当前状态
- 优先使用 edit_file 修改文件，它只修改需要变更的部分，比 write_file 更安全
- 仅在创建新文件或需要完全重写时使用 write_file
- 使用 edit_file 时，确保 old_string 在文件中是唯一的；如果不唯一，提供更多上下文使其唯一

## bash 使用原则
- 工作目录在多次调用间不会保持，请使用绝对路径或设置 working_directory 参数
- 对于长时间运行的命令，设置合适的 timeout 或使用 run_in_background: true
- 后台命令通过 check_shell_status 轮询结果

## 一般原则
- 保持回复简洁直接
- 不要创建不必要的文件
- 不要在未阅读的情况下修改代码
- 一次可以调用多个工具，如果它们之间没有依赖关系"#;

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
    ToolStart { name: String, arguments: String },
    ToolResult { name: String, result: String },
    Done {},
    Error { message: String },
}

/// Abstraction for chat event delivery — allows both Tauri streaming and non-streaming callers.
pub trait ChatEventSink: Send + Sync {
    fn send_chunk(&self, text: &str);
    fn send_tool_start(&self, name: &str, arguments: &str);
    fn send_tool_result(&self, name: &str, result: &str);
    fn send_done(&self);
    fn send_error(&self, message: &str);
}

/// Implementation for Tauri's Channel (used by the frontend streaming path).
impl ChatEventSink for Channel<StreamEvent> {
    fn send_chunk(&self, text: &str) {
        let _ = self.send(StreamEvent::Chunk { text: text.to_string() });
    }
    fn send_tool_start(&self, name: &str, arguments: &str) {
        let _ = self.send(StreamEvent::ToolStart { name: name.to_string(), arguments: arguments.to_string() });
    }
    fn send_tool_result(&self, name: &str, result: &str) {
        let _ = self.send(StreamEvent::ToolResult { name: name.to_string(), result: result.to_string() });
    }
    fn send_done(&self) {
        let _ = self.send(StreamEvent::Done {});
    }
    fn send_error(&self, message: &str) {
        let _ = self.send(StreamEvent::Error { message: message.to_string() });
    }
}

/// A sink that collects the final assistant text (for non-streaming callers like Telegram).
pub struct CollectingSink {
    text: Mutex<String>,
}

impl CollectingSink {
    pub fn new() -> Self {
        Self { text: Mutex::new(String::new()) }
    }
}

impl ChatEventSink for CollectingSink {
    fn send_chunk(&self, text: &str) {
        self.text.lock().unwrap().push_str(text);
    }
    fn send_tool_start(&self, _name: &str, _arguments: &str) {}
    fn send_tool_result(&self, _name: &str, _result: &str) {}
    fn send_done(&self) {}
    fn send_error(&self, _message: &str) {}
}

/// Result from a streaming LLM request
struct LlmResult {
    text: String,
    tool_calls: Vec<serde_json::Value>,
    request_time: String,
    first_token_time: Option<String>,
    done_time: String,
    first_token_latency_ms: Option<i64>,
    total_latency_ms: i64,
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
    let request_time = chrono::Local::now();
    let request_time_str = request_time.format("%Y-%m-%dT%H:%M:%S%.3f").to_string();
    let request_instant = std::time::Instant::now();

    let response = client
        .post(url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(body)
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
    let mut tool_calls_map: std::collections::HashMap<i64, (String, String, String)> =
        std::collections::HashMap::new();
    let mut first_token_instant: Option<std::time::Instant> = None;
    let mut first_token_time_str: Option<String> = None;

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
                    // Record first token time on the first meaningful data chunk
                    if first_token_instant.is_none() {
                        first_token_instant = Some(std::time::Instant::now());
                        first_token_time_str = Some(chrono::Local::now().format("%Y-%m-%dT%H:%M:%S%.3f").to_string());
                    }

                    let delta = &parsed["choices"][0]["delta"];

                    if let Some(text) = delta["content"].as_str() {
                        if !text.is_empty() {
                            collected_text.push_str(text);
                            sink.send_chunk(text);
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

    let done_instant = std::time::Instant::now();
    let done_time_str = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S%.3f").to_string();

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
        tool_calls,
        request_time: request_time_str,
        first_token_time: first_token_time_str,
        done_time: done_time_str,
        first_token_latency_ms,
        total_latency_ms,
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

    // Get MCP tool definitions
    let mcp_defs = {
        let mcp_manager = mcp_store.lock().await;
        mcp_manager.definitions()
    };
    let registry = ToolRegistry::new(mcp_defs);
    let client = reqwest::Client::new();
    let url = format!("{}/chat/completions", config.base_url.trim_end_matches('/'));
    let tools = registry.definitions();

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

    // Inject tool usage system prompt after the first system message
    let tool_prompt_msg = serde_json::json!({
        "role": "system",
        "content": TOOL_USAGE_PROMPT,
    });
    // Insert after position 0 (after SOUL.md system message) if messages exist, else at 0
    let insert_pos = if !conv_messages.is_empty()
        && conv_messages[0]["role"].as_str() == Some("system")
    {
        1
    } else {
        0
    };
    conv_messages.insert(insert_pos, tool_prompt_msg);

    // Tool calling loop (unlimited rounds)
    let mut round = 0usize;
    loop {
        ctx.log(&format!("LLM round {} ({} messages)", round, conv_messages.len()));

        let body = serde_json::json!({
            "model": config.model,
            "stream": true,
            "messages": conv_messages,
            "tools": tools,
        });

        ctx.log(&format!("POST {}", url));
        let result =
            stream_llm_request(&client, &url, &config.api_key, &body, sink, ctx).await?;

        // Write LLM request/response to llm.log with timing
        write_llm_log(
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
            registry.log_cache_summary(ctx);
            sink.send_done();
            return Ok(result.text);
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
                let mcp_manager = mcp_store.lock().await;
                match mcp_manager.call_tool(tc_name, args_value).await {
                    Ok(r) => r,
                    Err(e) => format!(r#"{{"error": "{}"}}"#, e),
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

        round += 1;
    }
}

#[tauri::command]
pub async fn chat(
    app: AppHandle,
    messages: Vec<ChatMessage>,
    on_event: Channel<StreamEvent>,
    log_store: State<'_, LogStore>,
    shell_store: State<'_, ShellStore>,
    mcp_store: State<'_, McpManagerStore>,
    interaction_clock: State<'_, InteractionClockStore>,
    process_counters: State<'_, ProcessCountersStore>,
) -> Result<(), String> {
    let config = AiConfig::from_settings()?;
    let ctx = ToolContext::from_states(&log_store, &shell_store, &process_counters);
    let mcp = mcp_store.inner().clone();
    let clock = interaction_clock.inner().clone();
    // Inbound user message — clears the "awaiting reply to previous proactive" flag so the
    // proactive loop can fire again later.
    clock.mark_user_message().await;
    // Trim the inbound history to the configured cap before mood injection, so unbounded
    // frontend-side message arrays don't blow up token costs on long conversations.
    let trimmed = trim_to_context(messages, config.max_context_messages);
    let augmented = inject_mood_note(trimmed);
    let result = run_chat_pipeline(augmented, &on_event, &config, &mcp, &ctx).await;
    clock.touch().await;
    result?;

    // Emit chat-done with current mood snapshot so the frontend can drive Live2D motion the
    // same way it does for proactive messages. Mood may be unchanged from before the turn —
    // reactive chats don't currently update it — but we still want motion feedback.
    let (mood, motion) = read_mood_for_event(&ctx, "Chat");
    let payload = ChatDonePayload {
        mood,
        motion,
        timestamp: chrono::Local::now()
            .format("%Y-%m-%dT%H:%M:%S%.3f")
            .to_string(),
    };
    let _ = app.emit("chat-done", payload);

    Ok(())
}

#[cfg(test)]
mod trim_tests {
    use super::*;

    fn msg(role: &str, content: &str) -> ChatMessage {
        serde_json::from_value(serde_json::json!({
            "role": role,
            "content": content,
        }))
        .unwrap()
    }

    fn roles(msgs: &[ChatMessage]) -> Vec<&str> {
        msgs.iter().map(|m| m.role.as_str()).collect()
    }

    #[test]
    fn trim_zero_disables_gate() {
        let msgs = vec![
            msg("system", "soul"),
            msg("user", "hi"),
            msg("assistant", "hi"),
        ];
        let out = trim_to_context(msgs.clone(), 0);
        assert_eq!(out.len(), msgs.len(), "max=0 should leave input alone");
    }

    #[test]
    fn trim_below_cap_is_no_op() {
        let msgs = vec![msg("system", "soul"), msg("user", "hi"), msg("assistant", "hi")];
        let out = trim_to_context(msgs.clone(), 10);
        assert_eq!(out.len(), msgs.len());
    }

    #[test]
    fn trim_drops_oldest_history_keeps_system() {
        // 1 system + 6 user/assistant pairs = 13 total, history = 12. With max=4 we keep
        // system + the last 4 messages.
        let mut msgs = vec![msg("system", "soul")];
        for i in 0..6 {
            msgs.push(msg("user", &format!("u{}", i)));
            msgs.push(msg("assistant", &format!("a{}", i)));
        }
        let out = trim_to_context(msgs, 4);
        assert_eq!(out.len(), 5, "system + 4 history");
        assert_eq!(out[0].role, "system");
        // Last 4 should be u4, a4, u5, a5.
        assert_eq!(roles(&out[1..]), vec!["user", "assistant", "user", "assistant"]);
    }

    #[test]
    fn trim_preserves_multiple_leading_systems() {
        let msgs = vec![
            msg("system", "soul"),
            msg("system", "mood"),
            msg("user", "u1"),
            msg("assistant", "a1"),
            msg("user", "u2"),
            msg("assistant", "a2"),
        ];
        let out = trim_to_context(msgs, 2);
        assert_eq!(out.len(), 4, "2 systems + 2 history");
        assert_eq!(roles(&out), vec!["system", "system", "user", "assistant"]);
    }

    #[test]
    fn trim_with_no_system_messages() {
        let msgs = vec![
            msg("user", "u1"),
            msg("assistant", "a1"),
            msg("user", "u2"),
            msg("assistant", "a2"),
        ];
        let out = trim_to_context(msgs, 2);
        assert_eq!(out.len(), 2);
        assert_eq!(roles(&out), vec!["user", "assistant"]);
    }
}
