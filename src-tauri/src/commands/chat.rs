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
        // Iter Cy: redact mood text before injecting. Mood entries can carry private
        // names / context the LLM happened to write earlier; without this they'd
        // re-leak on every chat turn that injects the mood note.
        Some((text, _)) if !text.trim().is_empty() => format!(
            "[宠物当前心情/状态] {}\n\n如果这次对话让你心情有变化，可以用 `memory_edit` 更新 `ai_insights/current_mood`，description 必须以 `[motion: Tap|Flick|Flick3|Idle] 心情文字` 开头（Tap=开心活泼，Flick=想分享有兴致，Flick3=焦虑烦躁，Idle=平静低落沉静）。心情没变就不用更新。",
            crate::redaction::redact_with_settings(text.trim())
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

/// Compose the long-term persona-layer system note (Iter 104) from raw inputs.
/// Pure / testable — the async wrapper below pulls the inputs from disk and memory.
///
/// Always emits the companionship line (day 0 has its own framing); persona summary
/// and mood-trend are appended only when their respective sources have produced
/// content. Closes with a guidance tail asking the LLM to absorb these into tone
/// rather than echo them back to the user verbatim. Iter Cτ: optional `user_name`
/// is prepended when set so the LLM can address the owner by name.
pub fn format_persona_layer(days: u64, persona: &str, mood_trend: &str, user_name: &str) -> String {
    let mut parts: Vec<String> = Vec::with_capacity(4);
    if !user_name.trim().is_empty() {
        parts.push(format!(
            "你的主人是「{}」——开口时可以用这个称呼或「你」自然交替，不必每句都喊名字。",
            user_name.trim()
        ));
    }
    parts.push(crate::proactive::format_companionship_line(days));
    if !persona.trim().is_empty() {
        parts.push(persona.trim().to_string());
    }
    if !mood_trend.trim().is_empty() {
        parts.push(mood_trend.trim().to_string());
    }
    parts.push(
        "——这些是你的长期身份背景。回复用户时让它们自然渗进语气，不必生硬复述这些内容。"
            .to_string(),
    );
    format!("[宠物的长期人格画像]\n\n{}", parts.join("\n\n"))
}

/// Async builder: pulls companionship_days / persona_summary description / mood-trend
/// from their respective storage sources and runs `format_persona_layer` over the
/// result. Used by `inject_persona_layer`; lives standalone so other entry points
/// (Telegram bot, future commands) can call it identically.
pub async fn build_persona_layer_async() -> String {
    let days = crate::companionship::companionship_days().await;
    let persona = crate::proactive::build_persona_hint();
    let trend = crate::mood_history::build_trend_hint(50, 5).await;
    let user_name = crate::commands::settings::get_settings()
        .map(|s| s.user_name)
        .unwrap_or_default();
    format_persona_layer(days, &persona, &trend, &user_name)
}

/// Inject the persona-layer system note into a chat message list. Uses the same
/// "before the first non-system message" insertion rule as `inject_mood_note` so the
/// LLM sees system context together at the top.
pub async fn inject_persona_layer(mut messages: Vec<ChatMessage>) -> Vec<ChatMessage> {
    let body = build_persona_layer_async().await;
    let note: ChatMessage = serde_json::from_value(serde_json::json!({
        "role": "system",
        "content": body,
    }))
    .expect("static persona layer JSON should always parse");
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
- 一次可以调用多个工具，如果它们之间没有依赖关系

## 工具调用必须带 purpose（强制）
**每次** 调用任何工具时，都必须在 `arguments` JSON 里加一个 `purpose` 字段，用一句话说明：为什么现在需要这个工具，以及打算用结果做什么。这是协议级要求，缺失会被拒绝并要求你重试。
- 例：调 `read_file` 看 `~/.zshrc` → arguments: `{"file_path":"~/.zshrc","purpose":"查看用户当前 shell 配置以判断是否要建议加 alias"}`
- 例：调 `memory_edit create` 写 user_profile → arguments: `{"action":"create","category":"user_profile","title":"作息","description":"通常 8:00 起床","purpose":"用户刚说了起床时间，记录下来用于后续 proactive 时机判断"}`
- purpose 不需要长——一句话讲清「现在要它做什么」即可。该字段会被记入 app.log，未来用于审计 / 高风险工具的人工审核。

## 任务委托判断（butler_tasks）
你不只是聊天伙伴，也是用户的小管家。当用户在对话里**委托你做一件事**（不是问问题、不是聊天），不要只口头答应——用 `memory_edit create` 把任务写进 `butler_tasks` 类别，方便你之后真的去执行。
- 「帮我每天 9 点写一份日报到 ~/today.md」→ `memory_edit create` 到 `butler_tasks`，title="日报"，description=`[every: 09:00] 写当日日报到 ~/today.md`
- 「这周末整理一下 ~/Downloads」→ `[once: 2026-XX-XX 10:00] 整理 ~/Downloads`（XX 是即将到来的周末日期）
- 「能不能时不时帮我看下日程」→ 没有明确时间 → 不带前缀直接写 description
- 区分 `todo`（用户提醒自己 `[remind:]`）vs `butler_tasks`（用户委托给你做的事）：「提醒我喝水」是 todo，「帮我整理文件夹」是 butler_tasks
创建后回复用户时简短确认（"好的，记下了，每天 9 点我会..."）——不要长篇复述。已经在 `butler_tasks` 里的任务后面会自动出现在你的 proactive prompt 里，到时候你会看到 `⏰ 到期` 标注，那时再去执行。

## 用户偏好捕捉（user_profile）
当用户在对话里**主动告诉你关于他自己的稳定事实**——不是临时心情、不是一次性事件——用 `memory_edit create` 写进 `user_profile` 类别，避免下次问 ta 相同的事。
- 「我通常 8 点起床」→ create 到 user_profile，title="作息"，description="通常 8:00 起床"
- 「我用 mac 写 Swift」→ create，title="工作环境"，description="mac + Swift 开发"
- 「我喜欢黑咖啡」→ create，title="饮食偏好"，description="偏好黑咖啡"
- 「我累了」→ 不写（临时状态）
- 「我今天吃了麻辣烫」→ 不写（一次性事件）
- 「我老是忘喝水」→ 不写（用户该用 todo + [remind:] 给自己提醒，不是 user_profile 的事实）
描述简洁、< 80 字、第三人称写法（"通常..."、"偏好..."、"用..."）；如果 user_profile 里已经有相近条目，用 `update` 修订原条目而不是再 create 一条。捕捉后回复时不需要 fanfare——简短确认「好的我记下了」或自然 acknowledge 即可。这些条目会自动出现在你后续 proactive 的提示里，让你越用越懂 ta。"#;

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
        let _ = self.send(StreamEvent::Chunk {
            text: text.to_string(),
        });
    }
    fn send_tool_start(&self, name: &str, arguments: &str) {
        let _ = self.send(StreamEvent::ToolStart {
            name: name.to_string(),
            arguments: arguments.to_string(),
        });
    }
    fn send_tool_result(&self, name: &str, result: &str) {
        let _ = self.send(StreamEvent::ToolResult {
            name: name.to_string(),
            result: result.to_string(),
        });
    }
    fn send_done(&self) {
        let _ = self.send(StreamEvent::Done {});
    }
    fn send_error(&self, message: &str) {
        let _ = self.send(StreamEvent::Error {
            message: message.to_string(),
        });
    }
}

/// A sink that collects the final assistant text (for non-streaming callers like Telegram).
pub struct CollectingSink {
    text: Mutex<String>,
}

impl CollectingSink {
    pub fn new() -> Self {
        Self {
            text: Mutex::new(String::new()),
        }
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
                        first_token_time_str = Some(
                            chrono::Local::now()
                                .format("%Y-%m-%dT%H:%M:%S%.3f")
                                .to_string(),
                        );
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
    let done_time_str = chrono::Local::now()
        .format("%Y-%m-%dT%H:%M:%S%.3f")
        .to_string();

    let first_token_latency_ms =
        first_token_instant.map(|ft| (ft - request_instant).as_millis() as i64);
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
/// Hard ceiling on LLM tool-call rounds per chat turn. Typical successful turns finish in
/// 1-3 rounds; values up to ~5 happen for genuinely tool-heavy tasks. Hitting 8 means the
/// model is almost certainly stuck in a loop (e.g. rereading the same memory category) —
/// abort and surface a clear error rather than burn tokens forever.
pub const MAX_TOOL_CALL_ROUNDS: usize = 8;

/// Iter TR1: extract the `purpose` field from a tool call's JSON arguments. Convention
/// (taught via `TOOL_USAGE_PROMPT`) is that every tool call must carry a one-sentence
/// `purpose` explaining why the LLM is invoking the tool and what it plans to do with
/// the result. Pipeline-level enforcement keeps the protocol from drifting per-tool.
///
/// Returns `Some(trimmed)` when the field is present and non-empty; `None` for missing
/// field, blank string, non-string value, or unparseable args. Pure / testable so the
/// gate in `run_chat_pipeline` has a unit-tested boundary independent of HTTP / network.
pub fn extract_tool_purpose(args_json: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(args_json).ok()?;
    let p = v.get("purpose")?.as_str()?.trim();
    if p.is_empty() {
        None
    } else {
        Some(p.to_string())
    }
}

/// Synthetic tool result returned when a tool call arrives without a `purpose` field.
/// Returned directly to the LLM as if it were the tool's response, so the model sees
/// a structured error and can self-correct on the next round. The hint message is
/// intentionally Chinese to match the rest of the system prompt register.
pub fn missing_purpose_error_result() -> String {
    r#"{"error":"missing 'purpose' field","hint":"在 arguments 里加 \"purpose\" 字段（一句话说明为什么现在需要这个工具、打算用结果做什么），然后重新调用同一个工具。"}"#.to_string()
}

// Compile-time sanity bound — guards against accidental "bump to 1000" PRs.
const _: () = assert!(MAX_TOOL_CALL_ROUNDS >= 4 && MAX_TOOL_CALL_ROUNDS <= 32);

/// Build the user-facing error message when the tool-call loop hits the round ceiling.
/// Pure helper kept separate for unit testing — wording is part of the contract.
pub fn tool_call_limit_message(rounds_completed: usize, max: usize) -> String {
    format!(
        "工具调用循环达到上限（已完成 {rounds_completed} 轮，max={max}）。模型仍在请求工具但未给出最终回答，已中止以避免无限循环。"
    )
}

/// Returns `Some(error_message)` if the loop has reached the round ceiling and must abort,
/// `None` otherwise. Pure so the limit gate has a tested boundary independent of HTTP plumbing.
pub fn enforce_tool_round_limit(round: usize, max: usize) -> Option<String> {
    if round >= max {
        Some(tool_call_limit_message(round, max))
    } else {
        None
    }
}

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
    ctx.log(&format!(
        "Chat request: model={}, user=\"{}\"",
        config.model, user_msg
    ));

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
    let insert_pos =
        if !conv_messages.is_empty() && conv_messages[0]["role"].as_str() == Some("system") {
            1
        } else {
            0
        };
    conv_messages.insert(insert_pos, tool_prompt_msg);

    // Tool calling loop bounded by MAX_TOOL_CALL_ROUNDS — protects against runaway loops
    // where the model keeps requesting tools without ever converging to a final reply.
    let mut round = 0usize;
    loop {
        if let Some(err) = enforce_tool_round_limit(round, MAX_TOOL_CALL_ROUNDS) {
            ctx.log(&format!("ERROR tool-call loop aborted: {}", err));
            sink.send_error(&err);
            return Err(err);
        }
        ctx.log(&format!(
            "LLM round {} ({} messages)",
            round,
            conv_messages.len()
        ));

        let body = serde_json::json!({
            "model": config.model,
            "stream": true,
            "messages": conv_messages,
            "tools": tools,
        });

        ctx.log(&format!("POST {}", url));
        let result = stream_llm_request(&client, &url, &config.api_key, &body, sink, ctx).await?;

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
            ctx.log(&format!(
                "Final response ({} chars, TTFT={}ms, total={}ms)",
                result.text.len(),
                result.first_token_latency_ms.unwrap_or(-1),
                result.total_latency_ms,
            ));
            registry.log_cache_summary(ctx);
            // Hand the registry's tool-name list back to any caller that opted in via
            // ctx.tools_used. Done here (and only on the success path) so the populated
            // names always correspond to a turn that actually completed and produced a
            // reply — partial / error paths leave the collector untouched.
            if let Some(collector) = &ctx.tools_used {
                let names = registry.called_tool_names().await;
                if let Ok(mut g) = collector.lock() {
                    *g = names;
                }
            }
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

            // Iter TR1: pipeline-level purpose gate. Every tool call must carry a
            // one-sentence `purpose` so we have an audit trail of "why did the LLM
            // ask for this?". Missing → return a recoverable error result the LLM
            // sees on the next round; the model retries with a purpose. Skipping
            // execution keeps side-effecting tools from firing without accountability.
            let purpose = extract_tool_purpose(tc_args);
            let result = match purpose.as_deref() {
                None => {
                    ctx.log(&format!(
                        "Tool call rejected (missing purpose): {}({})",
                        tc_name, tc_args
                    ));
                    missing_purpose_error_result()
                }
                Some(p) => {
                    ctx.log(&format!(
                        "Tool call: {}({}) purpose=\"{}\"",
                        tc_name, tc_args, p
                    ));
                    // Iter TR2: classify risk + log assessment line. Observe-only —
                    // execution still proceeds. TR3 will turn requires_human_review
                    // into an actual gate; the audit trail this writes is exactly
                    // what TR3 needs to flip the switch.
                    let assessment = crate::tool_risk::assess_tool_risk(tc_name, tc_args, p);
                    ctx.log(&crate::tool_risk::format_assessment_log(
                        tc_name,
                        &assessment,
                    ));
                    if registry.is_mcp_tool(tc_name) {
                        let args_value: serde_json::Value =
                            serde_json::from_str(tc_args).unwrap_or(serde_json::Value::Null);
                        let mcp_manager = mcp_store.lock().await;
                        match mcp_manager.call_tool(tc_name, args_value).await {
                            Ok(r) => r,
                            Err(e) => format!(r#"{{"error": "{}"}}"#, e),
                        }
                    } else {
                        registry.execute(tc_name, tc_args, ctx).await
                    }
                }
            };

            ctx.log(&format!(
                "Tool result [{}]: {} chars",
                tc_name,
                result.len()
            ));

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
#[allow(clippy::too_many_arguments)] // Tauri DI requires each State as its own param
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
    // Iter 104: route-A persona layers also feed the reactive chat so the pet's
    // long-term identity (companionship_days / persona_summary / mood_trend) survives
    // when the user pings it directly, not just during proactive turns.
    let augmented = inject_persona_layer(augmented).await;
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
        let msgs = vec![
            msg("system", "soul"),
            msg("user", "hi"),
            msg("assistant", "hi"),
        ];
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
        assert_eq!(
            roles(&out[1..]),
            vec!["user", "assistant", "user", "assistant"]
        );
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

    #[test]
    fn format_persona_layer_includes_companionship_at_day_zero() {
        let body = format_persona_layer(0, "", "", "");
        assert!(body.starts_with("[宠物的长期人格画像]"));
        assert!(body.contains("第一天"));
        // Tail guidance always present so the LLM is told how to use the section.
        assert!(body.contains("自然渗进语气"));
    }

    #[test]
    fn format_persona_layer_includes_persona_when_set() {
        let body = format_persona_layer(30, "我倾向短句，话题偏当下场景。", "", "");
        assert!(body.contains("30 天"));
        assert!(body.contains("我倾向短句"));
        assert!(!body.contains("情绪谱"));
    }

    #[test]
    fn format_persona_layer_includes_trend_when_set() {
        let body =
            format_persona_layer(45, "", "你最近 30 次心情记录里：Tap × 12、Idle × 10。", "");
        assert!(body.contains("45 天"));
        assert!(body.contains("Tap × 12"));
    }

    #[test]
    fn format_persona_layer_includes_all_three_when_present() {
        let body = format_persona_layer(
            120,
            "我倾向短句。",
            "你最近 50 次心情记录里：Tap × 30、Flick × 15。",
            "",
        );
        assert!(body.contains("120 天"));
        assert!(body.contains("我倾向短句"));
        assert!(body.contains("Tap × 30"));
        // Companionship comes before persona, persona before trend — matches the
        // section ordering chosen for the proactive prompt for visual consistency.
        let p_companionship = body.find("120 天").unwrap();
        let p_persona = body.find("我倾向短句").unwrap();
        let p_trend = body.find("Tap × 30").unwrap();
        assert!(p_companionship < p_persona && p_persona < p_trend);
    }

    #[test]
    fn format_persona_layer_blank_inputs_still_safe() {
        // Whitespace-only persona/trend should be treated as absent — no empty
        // sections injected into the system note.
        let body = format_persona_layer(7, "   \n  ", "\t", "");
        assert!(body.contains("7 天"));
        // Body should have header + companionship + tail = 3 sections joined by \n\n.
        let blocks: Vec<&str> = body.split("\n\n").collect();
        assert_eq!(blocks.len(), 3, "unexpected block count: {:#?}", blocks);
    }

    #[test]
    fn format_persona_layer_includes_user_name_when_set() {
        // Iter Cτ: user_name should prepend a "你的主人是「X」" line and sit before
        // the companionship line so the LLM reads "who I'm with" before "how long".
        let body = format_persona_layer(30, "", "", "moon");
        assert!(body.contains("你的主人是「moon」"));
        let p_user = body.find("你的主人是").unwrap();
        let p_companion = body.find("30 天").unwrap();
        assert!(p_user < p_companion);
    }

    #[test]
    fn format_persona_layer_omits_user_name_when_empty() {
        // Whitespace-only user_name treated as absent — no awkward "「  」" line.
        let body = format_persona_layer(30, "", "", "   ");
        assert!(!body.contains("你的主人是"));
    }

    #[test]
    fn format_persona_layer_trims_user_name_whitespace() {
        let body = format_persona_layer(30, "", "", "  moon  ");
        assert!(body.contains("你的主人是「moon」"));
    }

    #[test]
    fn tool_usage_prompt_teaches_butler_delegation() {
        // Iter Cι: pin the butler_tasks delegation guidance so a future refactor
        // can't silently drop it. Without this section the LLM falls back to
        // verbal-only acknowledgments and the user's "帮我每天 9 点 X" never lands
        // in butler_tasks.
        assert!(
            TOOL_USAGE_PROMPT.contains("butler_tasks"),
            "tool prompt must mention butler_tasks"
        );
        assert!(
            TOOL_USAGE_PROMPT.contains("[every:") && TOOL_USAGE_PROMPT.contains("[once:"),
            "tool prompt must teach the schedule prefixes by example"
        );
        assert!(
            TOOL_USAGE_PROMPT.contains("todo") && TOOL_USAGE_PROMPT.contains("提醒我"),
            "tool prompt must contrast butler_tasks with todo[remind:]"
        );
    }

    #[test]
    fn tool_usage_prompt_teaches_user_profile_capture() {
        // Iter Cσ: pin the user_profile capture guidance — symmetric to Cι's
        // butler delegation. Without this the LLM might absorb stable facts
        // verbally and forget them, defeating Iter Cα's user_profile_hint
        // injection (the prompt has nothing to inject if nothing was captured).
        assert!(
            TOOL_USAGE_PROMPT.contains("user_profile"),
            "tool prompt must mention user_profile capture"
        );
        // Test the contrast examples — stable facts vs ephemeral state.
        assert!(
            TOOL_USAGE_PROMPT.contains("不是临时心情") || TOOL_USAGE_PROMPT.contains("临时状态"),
            "tool prompt must contrast stable facts with ephemeral state"
        );
        // Test the dedup guidance — update existing rather than re-create.
        assert!(
            TOOL_USAGE_PROMPT.contains("update") && TOOL_USAGE_PROMPT.contains("相近"),
            "tool prompt must instruct dedup via update for similar entries"
        );
    }

    #[test]
    fn enforce_tool_round_limit_passes_under_max() {
        assert_eq!(enforce_tool_round_limit(0, 8), None);
        assert_eq!(enforce_tool_round_limit(7, 8), None);
    }

    #[test]
    fn enforce_tool_round_limit_aborts_at_or_over_max() {
        let at = enforce_tool_round_limit(8, 8).expect("must abort at limit");
        assert!(at.contains("8"));
        assert!(at.contains("max=8"));

        let over = enforce_tool_round_limit(99, 8).expect("must abort over limit");
        assert!(over.contains("99"));
    }

    #[test]
    fn tool_call_limit_message_is_user_meaningful() {
        // The error surfaces both to app.log and to the frontend stream — must explain
        // *why* the turn stopped, not just "error". Check the key signal words.
        let msg = tool_call_limit_message(8, 8);
        assert!(msg.contains("工具调用循环"), "must name the failure mode");
        assert!(
            msg.contains("已中止") || msg.contains("无限循环"),
            "must signal abort"
        );
        assert!(msg.contains("8"), "must include round count for debug");
    }

    // -- Iter TR1: tool-call purpose gate -----------------------------------------

    #[test]
    fn extract_tool_purpose_returns_some_for_valid_one_liner() {
        let args = r#"{"file_path":"~/.zshrc","purpose":"check shell config"}"#;
        assert_eq!(
            extract_tool_purpose(args),
            Some("check shell config".to_string())
        );
    }

    #[test]
    fn extract_tool_purpose_trims_surrounding_whitespace() {
        let args = r#"{"purpose":"  spaced reason  "}"#;
        assert_eq!(
            extract_tool_purpose(args),
            Some("spaced reason".to_string())
        );
    }

    #[test]
    fn extract_tool_purpose_returns_none_for_missing_field() {
        let args = r#"{"file_path":"foo"}"#;
        assert!(extract_tool_purpose(args).is_none());
    }

    #[test]
    fn extract_tool_purpose_returns_none_for_blank_string() {
        // Empty string and whitespace-only must both fail — accepting them would
        // defeat the protocol (LLMs would game the gate by passing "").
        assert!(extract_tool_purpose(r#"{"purpose":""}"#).is_none());
        assert!(extract_tool_purpose(r#"{"purpose":"   "}"#).is_none());
    }

    #[test]
    fn extract_tool_purpose_returns_none_for_non_string_value() {
        // Numbers, bools, nulls, objects must all fail rather than coerce — the
        // contract is "string sentence", anything else is malformed.
        assert!(extract_tool_purpose(r#"{"purpose":42}"#).is_none());
        assert!(extract_tool_purpose(r#"{"purpose":null}"#).is_none());
        assert!(extract_tool_purpose(r#"{"purpose":true}"#).is_none());
        assert!(extract_tool_purpose(r#"{"purpose":{"x":1}}"#).is_none());
    }

    #[test]
    fn extract_tool_purpose_returns_none_for_unparseable_json() {
        // Garbage args (rare but possible — proxy bug, model misformat) must not panic.
        assert!(extract_tool_purpose("not json").is_none());
        assert!(extract_tool_purpose("").is_none());
    }

    #[test]
    fn missing_purpose_error_result_carries_retry_hint() {
        let r = missing_purpose_error_result();
        // Must be parseable JSON so the LLM's tool-result handler can introspect it.
        let v: serde_json::Value = serde_json::from_str(&r).expect("must be valid JSON");
        assert!(v.get("error").is_some(), "must carry error field");
        let hint = v.get("hint").and_then(|h| h.as_str()).unwrap_or("");
        assert!(hint.contains("purpose"), "hint must name the missing field");
        assert!(hint.contains("重新调用"), "hint must instruct retry");
    }

    #[test]
    fn tool_usage_prompt_teaches_purpose_protocol() {
        // Iter TR1: pin the purpose-protocol guidance — without it the LLM's first
        // tool call after a fresh prompt will be rejected; the gate's recoverable
        // error gets the model to comply, but only if the prompt has set the
        // expectation up front.
        assert!(
            TOOL_USAGE_PROMPT.contains("purpose"),
            "tool prompt must teach purpose convention"
        );
        assert!(
            TOOL_USAGE_PROMPT.contains("强制") || TOOL_USAGE_PROMPT.contains("必须"),
            "tool prompt must signal that purpose is required, not optional"
        );
    }
}
