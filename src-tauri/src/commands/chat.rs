use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::State;

use crate::commands::debug::LogStore;
use crate::commands::shell::ShellStore;
use crate::config::AiConfig;
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
    ToolStart { name: String, arguments: String },
    ToolResult { name: String, result: String },
    Done {},
    Error { message: String },
}

/// Make a streaming LLM request; returns (collected_text, tool_calls)
async fn stream_llm_request(
    client: &reqwest::Client,
    url: &str,
    api_key: &str,
    body: &serde_json::Value,
    on_event: &Channel<StreamEvent>,
    ctx: &ToolContext,
) -> Result<(String, Vec<serde_json::Value>), String> {
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
        let _ = on_event.send(StreamEvent::Error { message: msg.clone() });
        return Err(msg);
    }

    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    let mut collected_text = String::new();
    let mut tool_calls_map: std::collections::HashMap<i64, (String, String, String)> =
        std::collections::HashMap::new();

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
                    let delta = &parsed["choices"][0]["delta"];

                    if let Some(text) = delta["content"].as_str() {
                        if !text.is_empty() {
                            collected_text.push_str(text);
                            let _ = on_event.send(StreamEvent::Chunk {
                                text: text.to_string(),
                            });
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

    Ok((collected_text, tool_calls))
}

#[tauri::command]
pub async fn chat(
    messages: Vec<ChatMessage>,
    on_event: Channel<StreamEvent>,
    log_store: State<'_, LogStore>,
    shell_store: State<'_, ShellStore>,
) -> Result<(), String> {
    let config = AiConfig::from_settings()?;

    let ctx = ToolContext::from_states(&log_store, &shell_store);

    let user_msg = messages
        .iter()
        .rev()
        .find(|m| m.role == "user")
        .and_then(|m| m.content.as_str())
        .unwrap_or_default();
    ctx.log(&format!("Chat request: model={}, user=\"{}\"", config.model, user_msg));

    let registry = ToolRegistry::new();
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
        let (text, tool_calls) =
            stream_llm_request(&client, &url, &config.api_key, &body, &on_event, &ctx).await?;

        if tool_calls.is_empty() {
            ctx.log(&format!("Final response ({} chars)", text.len()));
            let _ = on_event.send(StreamEvent::Done {});
            return Ok(());
        }

        ctx.log(&format!("Tool calls: {}", tool_calls.len()));

        // Add assistant message with tool_calls
        let mut assistant_msg = serde_json::json!({
            "role": "assistant",
            "content": if text.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(text) },
        });
        assistant_msg["tool_calls"] = serde_json::json!(tool_calls);
        conv_messages.push(assistant_msg);

        // Execute each tool call via registry
        for tc in &tool_calls {
            let tc_id = tc["id"].as_str().unwrap_or("");
            let tc_name = tc["function"]["name"].as_str().unwrap_or("");
            let tc_args = tc["function"]["arguments"].as_str().unwrap_or("{}");

            let _ = on_event.send(StreamEvent::ToolStart {
                name: tc_name.to_string(),
                arguments: tc_args.to_string(),
            });

            let result = registry.execute(tc_name, tc_args, &ctx).await;

            ctx.log(&format!("Tool result [{}]: {} chars", tc_name, result.len()));

            let _ = on_event.send(StreamEvent::ToolResult {
                name: tc_name.to_string(),
                result: result.clone(),
            });

            conv_messages.push(serde_json::json!({
                "role": "tool",
                "tool_call_id": tc_id,
                "content": result,
            }));
        }

        round += 1;
    }
}
