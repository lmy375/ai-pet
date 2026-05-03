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
pub struct TelegramConfig {
    #[serde(default)]
    pub bot_token: String,
    #[serde(default)]
    pub allowed_username: String,
    #[serde(default)]
    pub enabled: bool,
    /// Whether the bot's chat pipeline injects the route-A persona layer
    /// (companionship_days / persona_summary / mood_trend) into the LLM system
    /// prompt. Default true for parity with desktop chat. Users who only use
    /// Telegram for terse / utility queries can flip this off to keep the prompt
    /// minimal — the pet still answers, just without the long-term identity layer.
    #[serde(default = "default_telegram_persona_layer_enabled")]
    pub persona_layer_enabled: bool,
}

fn default_telegram_persona_layer_enabled() -> bool {
    true
}

impl Default for TelegramConfig {
    fn default() -> Self {
        Self {
            bot_token: String::new(),
            allowed_username: String::new(),
            enabled: false,
            persona_layer_enabled: default_telegram_persona_layer_enabled(),
        }
    }
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
    /// Iter Cλ: how many hours past the target time a completed `[once: ...]` butler
    /// task lingers before consolidate auto-deletes it. The grace period gives the
    /// daily summary + panel timeline a chance to catch the user's eye while the
    /// task is still around. 48 = "two days then it's gone".
    #[serde(default = "default_stale_once_butler_hours")]
    pub stale_once_butler_hours: u64,
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

fn default_stale_once_butler_hours() -> u64 {
    48
}

impl Default for MemoryConsolidateConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            interval_hours: default_consolidate_interval(),
            min_total_items: default_consolidate_min_items(),
            stale_reminder_hours: default_stale_reminder_hours(),
            stale_plan_hours: default_stale_plan_hours(),
            stale_once_butler_hours: default_stale_once_butler_hours(),
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PrivacyConfig {
    /// Substring patterns redacted (case-insensitive) from environment-aware tool
    /// output before the LLM sees it. Keep terms personal / specific (names, project
    /// codenames, sensitive client substrings) — the marker `(私人)` replaces each
    /// match. Empty list disables redaction. Backed by `crate::redaction`.
    #[serde(default)]
    pub redaction_patterns: Vec<String>,
    /// Regular-expression patterns redacted from the same prompt-injection channels
    /// (Iter Cz). Lets users catch structured sensitive data — credit-card-shaped
    /// digit groups, email addresses, phone numbers — that fixed-substring patterns
    /// can't express. Backed by the `regex` crate (RE2-style: linear time, no
    /// catastrophic backtracking, ReDoS-safe by construction). Invalid patterns are
    /// silently ignored at runtime so a typo doesn't disable the whole filter.
    #[serde(default)]
    pub regex_patterns: Vec<String>,
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
    #[serde(default)]
    pub privacy: PrivacyConfig,
    /// Iter Cτ: how the pet should address its owner. Empty = no name known
    /// (persona layer omits the line, LLM uses 「你」 default). Non-empty:
    /// injected as「你的主人是「X」」into the persona layer so the LLM can
    /// occasionally call the user by name. Plain string — no validation; if
    /// users put weird values they'll see them echoed.
    #[serde(default)]
    pub user_name: String,
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
            privacy: PrivacyConfig::default(),
            user_name: String::new(),
        }
    }
}

fn config_dir() -> Result<PathBuf, String> {
    let dir = dirs::config_dir().ok_or_else(|| "Cannot determine config directory".to_string())?;
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
    let content = fs::read_to_string(&path).map_err(|e| format!("Failed to read config: {}", e))?;
    let settings: AppSettings =
        serde_yaml::from_str(&content).map_err(|e| format!("Failed to parse config: {}", e))?;
    Ok(settings)
}

#[tauri::command]
pub fn save_settings(settings: AppSettings) -> Result<(), String> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create config dir: {}", e))?;
    }
    let yaml = serde_yaml::to_string(&settings)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;
    fs::write(&path, yaml).map_err(|e| format!("Failed to write config: {}", e))?;
    Ok(())
}

/// Iter D8: lightweight name accessor for the panel — full get_settings is
/// overkill when only `user_name` is wanted. Returns "" when no name configured
/// or when settings can't be read (degrade silently rather than alarm the user).
#[tauri::command]
pub fn get_user_name() -> String {
    get_settings().map(|s| s.user_name).unwrap_or_default()
}

#[tauri::command]
pub fn get_soul() -> Result<String, String> {
    let path = soul_path()?;
    if !path.exists() {
        return Ok(default_soul());
    }
    fs::read_to_string(&path).map_err(|e| format!("Failed to read SOUL.md: {}", e))
}

#[tauri::command]
pub fn get_config_raw() -> Result<String, String> {
    let path = config_path()?;
    if !path.exists() {
        let default_settings = AppSettings::default();
        return serde_yaml::to_string(&default_settings)
            .map_err(|e| format!("Failed to serialize default config: {}", e));
    }
    fs::read_to_string(&path).map_err(|e| format!("Failed to read config: {}", e))
}

#[tauri::command]
pub fn save_config_raw(content: String) -> Result<(), String> {
    // Validate YAML parses as AppSettings before saving
    let _: AppSettings =
        serde_yaml::from_str(&content).map_err(|e| format!("YAML 解析失败: {}", e))?;
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create config dir: {}", e))?;
    }
    fs::write(&path, &content).map_err(|e| format!("Failed to write config: {}", e))
}

#[tauri::command]
pub fn save_soul(content: String) -> Result<(), String> {
    let path = soul_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create config dir: {}", e))?;
    }
    fs::write(&path, content).map_err(|e| format!("Failed to write SOUL.md: {}", e))
}
