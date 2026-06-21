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

/// Saved pet-window top-left position (physical pixels). `None` until the user
/// first moves the window. Lives in config.yaml alongside the rest of the
/// settings — it used to be a separate `window_state.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowPosition {
    pub x: i32,
    pub y: i32,
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
    /// Directory the gallery slideshow draws media from (empty = not chosen).
    #[serde(default)]
    pub gallery_dir: String,
    /// When true, the pet window shows the gallery slideshow instead of Live2D.
    #[serde(default)]
    pub gallery_enabled: bool,
    /// Seconds each image stays on screen before advancing.
    #[serde(default = "default_gallery_interval")]
    pub gallery_interval: u32,
    /// When true, the pet wakes up in the background on a fixed interval to run a
    /// heartbeat session (see `HEARTBEAT.md`).
    #[serde(default)]
    pub heartbeat_enabled: bool,
    /// Minutes between scheduled heartbeats.
    #[serde(default = "default_heartbeat_interval")]
    pub heartbeat_interval: u32,
    /// Saved pet-window position so it reopens where the user left it. Written
    /// (debounced) on window move, not through the Settings UI; omitted from the
    /// file until the window has been moved at least once.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<WindowPosition>,
}

fn default_gallery_interval() -> u32 {
    10
}

fn default_heartbeat_interval() -> u32 {
    60
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
            gallery_dir: String::new(),
            gallery_enabled: false,
            gallery_interval: default_gallery_interval(),
            heartbeat_enabled: false,
            heartbeat_interval: default_heartbeat_interval(),
            window: None,
        }
    }
}

fn config_path() -> Result<PathBuf, String> {
    Ok(crate::common::config_dir()?.join("config.yaml"))
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

/// Open an arbitrary directory/file in the OS file manager (e.g. the gallery
/// folder in Finder). Used by the settings "open" buttons.
#[tauri::command]
pub fn open_path(app: tauri::AppHandle, path: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    if path.trim().is_empty() {
        return Err("路径为空".to_string());
    }
    app.opener()
        .open_path(path, None::<&str>)
        .map_err(|e| format!("Failed to open path: {}", e))
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
pub fn save_settings(app: tauri::AppHandle, settings: AppSettings) -> Result<(), String> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create config dir: {}", e))?;
    }
    let yaml = serde_yaml::to_string(&settings)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;
    fs::write(&path, yaml)
        .map_err(|e| format!("Failed to write config: {}", e))?;
    // Notify every window so each reloads its in-memory copy (the panel and the
    // pet hold separate copies; without this the pet wouldn't pick up changes
    // like gallery mode until refocused).
    use tauri::Emitter;
    let _ = app.emit("settings-changed", ());
    Ok(())
}

/// Persist only the pet-window position into config.yaml (read-modify-write).
/// Unlike `save_settings` this does NOT emit `settings-changed` — a window move
/// shouldn't make every window reload its settings.
pub fn set_window_position(x: i32, y: i32) -> Result<(), String> {
    let mut settings = get_settings()?;
    settings.window = Some(WindowPosition { x, y });
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create config dir: {}", e))?;
    }
    let yaml = serde_yaml::to_string(&settings)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;
    fs::write(&path, yaml)
        .map_err(|e| format!("Failed to write config: {}", e))
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
pub fn save_config_raw(app: tauri::AppHandle, content: String) -> Result<(), String> {
    // Validate YAML parses as AppSettings before saving
    let _: AppSettings = serde_yaml::from_str(&content)
        .map_err(|e| format!("YAML 解析失败: {}", e))?;
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create config dir: {}", e))?;
    }
    fs::write(&path, &content)
        .map_err(|e| format!("Failed to write config: {}", e))?;
    use tauri::Emitter;
    let _ = app.emit("settings-changed", ());
    Ok(())
}
