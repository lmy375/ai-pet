use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::State;

use crate::commands::debug::LogStore;
use crate::config::AiConfig;

#[derive(Debug, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "event", content = "data")]
pub enum StreamEvent {
    Chunk { text: String },
    Done {},
    Error { message: String },
}

fn log(store: &State<'_, LogStore>, msg: &str) {
    let mut logs = store.0.lock().unwrap();
    let ts = chrono::Local::now().format("%H:%M:%S%.3f").to_string();
    logs.push(format!("[{}] {}", ts, msg));
    if logs.len() > 500 {
        let drain = logs.len() - 500;
        logs.drain(0..drain);
    }
}

#[tauri::command]
pub async fn chat(
    messages: Vec<ChatMessage>,
    on_event: Channel<StreamEvent>,
    store: State<'_, LogStore>,
) -> Result<(), String> {
    let config = AiConfig::from_settings()?;

    let user_msg = messages.iter().rev().find(|m| m.role == "user")
        .map(|m| m.content.clone()).unwrap_or_default();
    log(&store, &format!("Chat request: model={}, user=\"{}\"", config.model, user_msg));

    let body = serde_json::json!({
        "model": config.model,
        "stream": true,
        "messages": messages.iter().map(|m| {
            serde_json::json!({ "role": m.role, "content": m.content })
        }).collect::<Vec<_>>(),
    });

    let url = format!("{}/chat/completions", config.base_url.trim_end_matches('/'));
    log(&store, &format!("POST {}", url));

    let client = reqwest::Client::new();
    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", config.api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| {
            log(&store, &format!("ERROR: request failed: {}", e));
            e.to_string()
        })?;

    let status = response.status();
    log(&store, &format!("Response status: {}", status));

    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        let msg = format!("API error {}: {}", status, text);
        log(&store, &format!("ERROR: {}", msg));
        let _ = on_event.send(StreamEvent::Error { message: msg.clone() });
        return Err(msg);
    }

    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    let mut total_tokens = 0usize;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| {
            log(&store, &format!("ERROR: stream error: {}", e));
            e.to_string()
        })?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(pos) = buffer.find('\n') {
            let line = buffer[..pos].trim().to_string();
            buffer = buffer[pos + 1..].to_string();

            if let Some(data) = line.strip_prefix("data: ") {
                if data == "[DONE]" {
                    log(&store, &format!("Stream done. Total chunks: {}", total_tokens));
                    let _ = on_event.send(StreamEvent::Done {});
                    return Ok(());
                }
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(data) {
                    if let Some(text) = parsed["choices"][0]["delta"]["content"].as_str() {
                        if !text.is_empty() {
                            total_tokens += 1;
                            let _ = on_event.send(StreamEvent::Chunk { text: text.to_string() });
                        }
                    }
                }
            }
        }
    }

    log(&store, &format!("Stream ended. Total chunks: {}", total_tokens));
    let _ = on_event.send(StreamEvent::Done {});
    Ok(())
}
