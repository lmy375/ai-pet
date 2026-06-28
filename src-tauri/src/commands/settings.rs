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

/// One configurable agent. Each agent has its own model, persona/memory
/// (under `memory/<id>/`), MCP tool set, Telegram bot and heartbeat schedule.
/// Global concerns (Live2D, gallery, language) live on `AppSettings` instead.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Stable identifier, also the memory subdir name (`memory/<id>/`) and the
    /// Telegram session id suffix (`telegram-<id>`). Never changes once created.
    #[serde(default = "default_agent_id")]
    pub id: String,
    /// Human-readable name shown in the agent switcher / settings.
    #[serde(default = "default_agent_name")]
    pub name: String,
    #[serde(default = "default_api_base")]
    pub api_base: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default = "default_model")]
    pub model: String,
    /// Context-window size (tokens), the denominator of the chat context-usage
    /// ring. Not exposed by the OpenAI API, so it's user-configured.
    #[serde(default = "default_context_window")]
    pub context_window: u32,
    #[serde(default)]
    pub mcp_servers: HashMap<String, McpServerConfig>,
    #[serde(default)]
    pub telegram: TelegramConfig,
    /// When true, this agent wakes up in the background on a fixed interval to
    /// run a heartbeat session (see `HEARTBEAT.md`).
    #[serde(default)]
    pub heartbeat_enabled: bool,
    /// Minutes between scheduled heartbeats.
    #[serde(default = "default_heartbeat_interval")]
    pub heartbeat_interval: u32,
    /// How many recent conversation "turns" of the active session a heartbeat
    /// forks in (one turn = a user message + the assistant/tool messages that
    /// follow it). 0 = carry no history, falling back to HEARTBEAT.md-only.
    #[serde(default = "default_heartbeat_context_turns")]
    pub heartbeat_context_turns: u32,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            id: default_agent_id(),
            name: default_agent_name(),
            api_base: default_api_base(),
            api_key: String::new(),
            model: default_model(),
            context_window: default_context_window(),
            mcp_servers: HashMap::new(),
            telegram: TelegramConfig::default(),
            heartbeat_enabled: false,
            heartbeat_interval: default_heartbeat_interval(),
            heartbeat_context_turns: default_heartbeat_context_turns(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    #[serde(default = "default_model_path")]
    pub live_2d_model_path: String,
    /// UI language: "zh" or "en".
    #[serde(default = "default_language")]
    pub language: String,
    /// Directory the gallery slideshow draws media from (empty = not chosen).
    #[serde(default)]
    pub gallery_dir: String,
    /// When true, the pet window shows the gallery slideshow instead of Live2D.
    #[serde(default)]
    pub gallery_enabled: bool,
    /// Seconds each image stays on screen before advancing.
    #[serde(default = "default_gallery_interval")]
    pub gallery_interval: u32,
    /// Tavily API key for the `web_search` tool, shared by all agents. Empty =
    /// web search disabled (the tool isn't offered to the model — see
    /// `ToolRegistry::new`).
    #[serde(default)]
    pub search_api_key: String,
    /// Id of the agent that answers the desktop chat window. Switching agents in
    /// the chat UI just rewrites this; chat history is global/shared.
    #[serde(default = "default_agent_id")]
    pub active_agent: String,
    /// The configured agents. Always at least one after `ensure`.
    #[serde(default = "default_agents")]
    pub agents: Vec<AgentConfig>,
    /// Saved pet-window position so it reopens where the user left it. Written
    /// (debounced) on window move, not through the Settings UI; omitted from the
    /// file until the window has been moved at least once.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<WindowPosition>,
}

impl AppSettings {
    /// The agent that answers the desktop chat window: the one whose id matches
    /// `active_agent`, falling back to the first agent. `None` only when there
    /// are no agents at all.
    pub fn active_agent_config(&self) -> Option<&AgentConfig> {
        self.agents
            .iter()
            .find(|a| a.id == self.active_agent)
            .or_else(|| self.agents.first())
    }

    /// Look up an agent by id.
    pub fn agent(&self, id: &str) -> Option<&AgentConfig> {
        self.agents.iter().find(|a| a.id == id)
    }
}

/// The active agent's id (resolved like `active_agent_config`), or "default"
/// when settings can't be read. Used by memory/session paths that need an agent
/// even outside a chat turn.
pub fn active_agent_id() -> String {
    get_settings()
        .ok()
        .and_then(|s| s.active_agent_config().map(|a| a.id.clone()))
        .unwrap_or_else(default_agent_id)
}

fn default_gallery_interval() -> u32 {
    10
}

fn default_heartbeat_interval() -> u32 {
    60
}

fn default_heartbeat_context_turns() -> u32 {
    10
}

fn default_model_path() -> String {
    "/models/miku/miku.model3.json".to_string()
}

fn default_api_base() -> String {
    "https://api.openai.com/v1".to_string()
}

fn default_context_window() -> u32 {
    128000
}

fn default_model() -> String {
    "gpt-4o-mini".to_string()
}

fn default_language() -> String {
    "zh".to_string()
}

fn default_agent_id() -> String {
    "default".to_string()
}

fn default_agent_name() -> String {
    "默认".to_string()
}

fn default_agents() -> Vec<AgentConfig> {
    vec![AgentConfig::default()]
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            live_2d_model_path: default_model_path(),
            language: default_language(),
            gallery_dir: String::new(),
            gallery_enabled: false,
            gallery_interval: default_gallery_interval(),
            search_api_key: String::new(),
            active_agent: default_agent_id(),
            agents: default_agents(),
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

/// Create the memory dir + mandatory files for every configured agent. Called
/// after any settings write so a newly-added agent gets its `memory/<id>/`.
fn ensure_agent_dirs(settings: &AppSettings) {
    for agent in &settings.agents {
        let _ = crate::commands::memory::ensure_memory_files(&agent.id);
        let _ = crate::commands::heartbeat_file::ensure_heartbeat_file(&agent.id);
    }
}

/// Write raw YAML text to config.yaml, creating the parent dir if needed.
fn write_config_file(yaml: &str) -> Result<(), String> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create config dir: {}", e))?;
    }
    fs::write(&path, yaml).map_err(|e| format!("Failed to write config: {}", e))
}

/// Serialize `settings`, write it to config.yaml, then run the standard
/// post-write side effects: ensure every agent's memory dirs exist and emit
/// `settings-changed` so each window reloads its in-memory copy (the panel and
/// pet hold separate copies; without this the pet wouldn't pick up changes like
/// gallery mode until refocused). Use for any settings-mutating command.
fn write_settings(app: &tauri::AppHandle, settings: &AppSettings) -> Result<(), String> {
    let yaml = serde_yaml::to_string(settings)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;
    write_config_file(&yaml)?;
    ensure_agent_dirs(settings);
    use tauri::Emitter;
    let _ = app.emit("settings-changed", ());
    Ok(())
}

#[tauri::command]
pub fn save_settings(app: tauri::AppHandle, settings: AppSettings) -> Result<(), String> {
    write_settings(&app, &settings)
}

/// Switch the active agent (the one answering the desktop chat window) without
/// rewriting the whole settings object. Chat history is global, so this only
/// changes who responds next. Emits `settings-changed` so both windows reload.
#[tauri::command]
pub fn set_active_agent(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let mut settings = get_settings()?;
    if settings.agent(&id).is_none() {
        return Err(format!("Unknown agent: {}", id));
    }
    settings.active_agent = id;
    write_settings(&app, &settings)
}

/// Persist only the pet-window position into config.yaml (read-modify-write).
/// Unlike `save_settings` this does NOT emit `settings-changed` — a window move
/// shouldn't make every window reload its settings.
pub fn set_window_position(x: i32, y: i32) -> Result<(), String> {
    let mut settings = get_settings()?;
    settings.window = Some(WindowPosition { x, y });
    let yaml = serde_yaml::to_string(&settings)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;
    write_config_file(&yaml)
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
    // Validate YAML parses as AppSettings before saving. Write the user's exact
    // text (preserving comments/formatting) rather than re-serializing.
    let settings: AppSettings = serde_yaml::from_str(&content)
        .map_err(|e| format!("YAML 解析失败: {}", e))?;
    write_config_file(&content)?;
    ensure_agent_dirs(&settings);
    use tauri::Emitter;
    let _ = app.emit("settings-changed", ());
    Ok(())
}
