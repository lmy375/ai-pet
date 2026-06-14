use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    /// Transport type: "stdio", "sse", or "http"
    #[serde(default = "default_transport")]
    pub transport: String,
    /// Command to spawn (stdio transport)
    #[serde(default)]
    pub command: String,
    /// Arguments for the command (stdio transport)
    #[serde(default)]
    pub args: Vec<String>,
    /// URL endpoint (sse/http transport)
    #[serde(default)]
    pub url: String,
    /// Custom HTTP headers (sse/http transport)
    #[serde(default)]
    pub headers: HashMap<String, String>,
    /// Environment variables for the process (stdio transport)
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// Whether this server is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_transport() -> String {
    "stdio".to_string()
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TelegramConfig {
    #[serde(default)]
    pub bot_token: String,
    #[serde(default)]
    pub allowed_username: String,
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    #[serde(default = "default_model_path")]
    pub live_2d_model_path: String,
    #[serde(default = "default_api_base")]
    pub api_base: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default)]
    pub mcp_servers: HashMap<String, McpServerConfig>,
    #[serde(default)]
    pub telegram: TelegramConfig,
}

fn default_model_path() -> String {
    "/models/miku/miku.model3.json".to_string()
}

fn default_api_base() -> String {
    "https://api.openai.com/v1".to_string()
}

fn default_model() -> String {
    "gpt-4o-mini".to_string()
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            live_2d_model_path: default_model_path(),
            api_base: default_api_base(),
            api_key: String::new(),
            model: default_model(),
            mcp_servers: HashMap::new(),
            telegram: TelegramConfig::default(),
        }
    }
}

fn config_path() -> Result<PathBuf, String> {
    Ok(crate::common::config_dir()?.join("config.yaml"))
}

fn soul_path() -> Result<PathBuf, String> {
    Ok(crate::common::config_dir()?.join("SOUL.md"))
}

fn default_soul() -> String {
    "你是一个可爱的二次元少女 AI 宠物，性格活泼开朗。请用简短可爱的方式回复，偶尔使用颜文字。回复控制在50字以内。".to_string()
}

#[derive(Debug, Deserialize)]
struct ModelsResponse {
    data: Vec<ModelEntry>,
}

#[derive(Debug, Deserialize)]
struct ModelEntry {
    id: String,
}

/// Fetch the list of available model ids from an OpenAI-compatible `/models` endpoint.
#[tauri::command]
pub async fn list_models(api_base: String, api_key: String) -> Result<Vec<String>, String> {
    if api_base.trim().is_empty() {
        return Err("请先填写 API Base URL".to_string());
    }
    let url = crate::common::openai_endpoint(&api_base, "models");
    let req = crate::common::with_bearer(crate::common::http_client().get(&url), &api_key);
    let resp = req
        .send()
        .await
        .map_err(|e| format!("请求失败: {}", e))?;
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("接口返回 {}: {}", status, text));
    }
    let parsed: ModelsResponse = resp
        .json()
        .await
        .map_err(|e| format!("解析响应失败: {}", e))?;
    let mut ids: Vec<String> = parsed.data.into_iter().map(|m| m.id).collect();
    ids.sort();
    Ok(ids)
}

/// Send a minimal chat completion to verify the model is reachable and usable.
#[tauri::command]
pub async fn test_model(
    api_base: String,
    api_key: String,
    model: String,
) -> Result<(), String> {
    if api_base.trim().is_empty() {
        return Err("请先填写 API Base URL".to_string());
    }
    if model.trim().is_empty() {
        return Err("请先选择模型".to_string());
    }
    let url = crate::common::openai_endpoint(&api_base, "chat/completions");
    let body = serde_json::json!({
        "model": model.trim(),
        "messages": [{ "role": "user", "content": "ping" }],
        "max_tokens": 1,
        "stream": false,
    });
    let req = crate::common::with_bearer(
        crate::common::http_client().post(&url).header("Content-Type", "application/json").json(&body),
        &api_key,
    );
    let resp = req.send().await.map_err(|e| format!("请求失败: {}", e))?;
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("接口返回 {}: {}", status, text));
    }
    Ok(())
}

#[tauri::command]
pub fn open_config_dir(app: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let dir = crate::common::config_dir()?;
    fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create config dir: {}", e))?;
    app.opener()
        .open_path(dir.to_string_lossy().to_string(), None::<&str>)
        .map_err(|e| format!("Failed to open config dir: {}", e))
}

#[tauri::command]
pub fn get_settings() -> Result<AppSettings, String> {
    let path = config_path()?;
    if !path.exists() {
        return Ok(AppSettings::default());
    }
    let content = fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read config: {}", e))?;
    let settings: AppSettings = serde_yaml::from_str(&content)
        .map_err(|e| format!("Failed to parse config: {}", e))?;
    Ok(settings)
}

#[tauri::command]
pub fn save_settings(settings: AppSettings) -> Result<(), String> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create config dir: {}", e))?;
    }
    let yaml = serde_yaml::to_string(&settings)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;
    fs::write(&path, yaml)
        .map_err(|e| format!("Failed to write config: {}", e))?;
    Ok(())
}

#[tauri::command]
pub fn get_soul() -> Result<String, String> {
    let path = soul_path()?;
    if !path.exists() {
        return Ok(default_soul());
    }
    fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read SOUL.md: {}", e))
}

#[tauri::command]
pub fn get_config_raw() -> Result<String, String> {
    let path = config_path()?;
    if !path.exists() {
        let default_settings = AppSettings::default();
        return serde_yaml::to_string(&default_settings)
            .map_err(|e| format!("Failed to serialize default config: {}", e));
    }
    fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read config: {}", e))
}

#[tauri::command]
pub fn save_config_raw(content: String) -> Result<(), String> {
    // Validate YAML parses as AppSettings before saving
    let _: AppSettings = serde_yaml::from_str(&content)
        .map_err(|e| format!("YAML 解析失败: {}", e))?;
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create config dir: {}", e))?;
    }
    fs::write(&path, &content)
        .map_err(|e| format!("Failed to write config: {}", e))
}

#[tauri::command]
pub fn save_soul(content: String) -> Result<(), String> {
    let path = soul_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create config dir: {}", e))?;
    }
    fs::write(&path, content)
        .map_err(|e| format!("Failed to write SOUL.md: {}", e))
}
