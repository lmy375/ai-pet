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
        }
    }
}

fn config_dir() -> Result<PathBuf, String> {
    let dir = dirs::config_dir()
        .ok_or_else(|| "Cannot determine config directory".to_string())?;
    Ok(dir.join("pet"))
}

fn config_path() -> Result<PathBuf, String> {
    Ok(config_dir()?.join("config.yaml"))
}

fn soul_path() -> Result<PathBuf, String> {
    Ok(config_dir()?.join("SOUL.md"))
}

fn default_soul() -> String {
    "你是一个可爱的二次元少女 AI 宠物，性格活泼开朗。请用简短可爱的方式回复，偶尔使用颜文字。回复控制在50字以内。".to_string()
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
pub fn save_soul(content: String) -> Result<(), String> {
    let path = soul_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create config dir: {}", e))?;
    }
    fs::write(&path, content)
        .map_err(|e| format!("Failed to write SOUL.md: {}", e))
}
