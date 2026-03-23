use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;

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

#[tauri::command]
pub async fn chat(
    messages: Vec<ChatMessage>,
    on_event: Channel<StreamEvent>,
) -> Result<(), String> {
    let config = AiConfig::from_env()?;

    let body = serde_json::json!({
        "model": config.model,
        "stream": true,
        "messages": messages.iter().map(|m| {
            serde_json::json!({ "role": m.role, "content": m.content })
        }).collect::<Vec<_>>(),
    });

    let client = reqwest::Client::new();
    let response = client
        .post(format!(
            "{}/chat/completions",
            config.base_url.trim_end_matches('/')
        ))
        .header("Authorization", format!("Bearer {}", config.api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        let msg = format!("API error {}: {}", status, text);
        let _ = on_event.send(StreamEvent::Error {
            message: msg.clone(),
        });
        return Err(msg);
    }

    let mut stream = response.bytes_stream();
    let mut buffer = String::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(pos) = buffer.find('\n') {
            let line = buffer[..pos].trim().to_string();
            buffer = buffer[pos + 1..].to_string();

            if let Some(data) = line.strip_prefix("data: ") {
                if data == "[DONE]" {
                    let _ = on_event.send(StreamEvent::Done {});
                    return Ok(());
                }
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(data) {
                    if let Some(text) = parsed["choices"][0]["delta"]["content"].as_str() {
                        if !text.is_empty() {
                            let _ = on_event.send(StreamEvent::Chunk {
                                text: text.to_string(),
                            });
                        }
                    }
                }
            }
        }
    }

    let _ = on_event.send(StreamEvent::Done {});
    Ok(())
}
