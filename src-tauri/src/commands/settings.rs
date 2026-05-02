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
pub struct ProactiveConfig {
    #[serde(default)]
    pub enabled: bool,
    /// How often the background loop wakes up to consider speaking, in seconds.
    #[serde(default = "default_proactive_interval")]
    pub interval_seconds: u64,
    /// Required idle time since last interaction before pet may speak, in seconds.
    #[serde(default = "default_proactive_idle_threshold")]
    pub idle_threshold_seconds: u64,
    /// Required idle time since the last keyboard/mouse event, in seconds.
    /// Prevents the pet from interrupting while the user is actively typing.
    /// Set to 0 to disable the input-idle gate.
    #[serde(default = "default_proactive_input_idle")]
    pub input_idle_seconds: u64,
    /// Minimum seconds between two proactive utterances, regardless of idle. Prevents the
    /// pet from speaking again right after just speaking. Set to 0 to disable.
    #[serde(default = "default_proactive_cooldown")]
    pub cooldown_seconds: u64,
    /// Start of the daily quiet window, in 24-hour local time (0–23). Pet stays silent
    /// during this window. Set start == end to disable the gate.
    #[serde(default = "default_quiet_hours_start")]
    pub quiet_hours_start: u8,
    /// End of the daily quiet window (exclusive), in 24-hour local time (0–23).
    #[serde(default = "default_quiet_hours_end")]
    pub quiet_hours_end: u8,
    /// When true, the pet stays silent while macOS Focus / Do-Not-Disturb is engaged.
    /// On non-macOS or when the Focus state file is unreadable, this gate is a no-op.
    #[serde(default = "default_respect_focus_mode")]
    pub respect_focus_mode: bool,
    /// Threshold at which today_speech_count starts injecting the "today you've already
    /// said a lot, prefer to stay quiet" rule into the proactive prompt. Lower = the pet
    /// becomes selective sooner; raise it on quiet days when you'd like more company.
    /// 0 disables the rule entirely (pet never gets a chatty-day nudge).
    #[serde(default = "default_chatty_day_threshold")]
    pub chatty_day_threshold: u64,
}

fn default_proactive_interval() -> u64 {
    300
}

fn default_proactive_idle_threshold() -> u64 {
    900
}

fn default_proactive_input_idle() -> u64 {
    60
}

fn default_proactive_cooldown() -> u64 {
    1800
}

fn default_quiet_hours_start() -> u8 {
    23
}

fn default_quiet_hours_end() -> u8 {
    7
}

fn default_respect_focus_mode() -> bool {
    true
}

fn default_chatty_day_threshold() -> u64 {
    5
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConsolidateConfig {
    #[serde(default)]
    pub enabled: bool,
    /// How often the consolidation loop runs, in hours.
    #[serde(default = "default_consolidate_interval")]
    pub interval_hours: u64,
    /// Skip consolidation if total memory items across all categories is below this. Avoids
    /// burning tokens to "tidy up" an empty index.
    #[serde(default = "default_consolidate_min_items")]
    pub min_total_items: usize,
    /// Sweep an Absolute-form reminder that's been past its target by this many hours.
    /// 24 is conservative for daily-use; raise it for devices that suspend overnight,
    /// lower it for always-on setups that want todos to clear out faster.
    #[serde(default = "default_stale_reminder_hours")]
    pub stale_reminder_hours: u64,
    /// Sweep the pet's `ai_insights/daily_plan` entry when its updated_at is older than
    /// this many hours. Plans are short-term intent; lingering plans bias every prompt
    /// after they were relevant. 24 = "today's plan ages out tomorrow".
    #[serde(default = "default_stale_plan_hours")]
    pub stale_plan_hours: u64,
}

fn default_consolidate_interval() -> u64 {
    6
}

fn default_consolidate_min_items() -> usize {
    12
}

fn default_stale_reminder_hours() -> u64 {
    24
}

fn default_stale_plan_hours() -> u64 {
    24
}

impl Default for MemoryConsolidateConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            interval_hours: default_consolidate_interval(),
            min_total_items: default_consolidate_min_items(),
            stale_reminder_hours: default_stale_reminder_hours(),
            stale_plan_hours: default_stale_plan_hours(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatConfig {
    /// Maximum number of trailing user/assistant messages sent to the LLM. Leading system
    /// messages (SOUL.md, mood note) are always preserved. Caps token cost on long
    /// conversations without dropping persona context. Set to 0 to disable trimming.
    #[serde(default = "default_chat_max_context")]
    pub max_context_messages: usize,
}

fn default_chat_max_context() -> usize {
    50
}

impl Default for ChatConfig {
    fn default() -> Self {
        Self {
            max_context_messages: default_chat_max_context(),
        }
    }
}

impl Default for ProactiveConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            interval_seconds: default_proactive_interval(),
            idle_threshold_seconds: default_proactive_idle_threshold(),
            input_idle_seconds: default_proactive_input_idle(),
            cooldown_seconds: default_proactive_cooldown(),
            quiet_hours_start: default_quiet_hours_start(),
            quiet_hours_end: default_quiet_hours_end(),
            respect_focus_mode: default_respect_focus_mode(),
            chatty_day_threshold: default_chatty_day_threshold(),
        }
    }
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
    #[serde(default)]
    pub proactive: ProactiveConfig,
    #[serde(default)]
    pub memory_consolidate: MemoryConsolidateConfig,
    #[serde(default)]
    pub chat: ChatConfig,
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
            proactive: ProactiveConfig::default(),
            memory_consolidate: MemoryConsolidateConfig::default(),
            chat: ChatConfig::default(),
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
