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
    /// 用户自定义的 TG 命令名 + 描述。bot 启动时与硬编码 5 条合并后调
    /// `set_my_commands`，让用户在 TG 客户端打 `/` 时看到。调用时不走
    /// command dispatch，**直接 fall through 到 chat pipeline** —— LLM
    /// 把 `/name <args>` 当文本看待 + 自由选 tool；不绑定具体 tool 映射。
    #[serde(default)]
    pub custom_commands: Vec<TgCustomCommand>,
    /// TG 客户端补全表里 hardcoded 命令的描述语种：`"zh"`（默认） / `"en"`。
    /// 自定义命令的描述用户自填，**不**翻译。其它运行时反馈（成功 /
    /// 失败 / 未知命令文案）当前都还中文，未走本字段。
    #[serde(default = "default_telegram_command_lang")]
    pub command_lang: String,
}

fn default_telegram_command_lang() -> String {
    "zh".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct TgCustomCommand {
    /// 命令名（不带 `/`）。lowercase ASCII / 数字 / `_`，与 TG API 约束一致。
    pub name: String,
    /// TG 客户端补全弹窗显示的描述。≤ 256 字。
    pub description: String,
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
            custom_commands: Vec::new(),
            command_lang: default_telegram_command_lang(),
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
    /// Iter R13: high-level companion temperament preset. One of `"balanced"`
    /// (default — honor the explicit cooldown / chatty values above),
    /// `"chatty"` (cooldown × 0.5, chatty threshold × 2), or `"quiet"`
    /// (cooldown × 2.0, chatty threshold × 0.5). Lets users dial overall
    /// "how much should the pet speak today?" without tuning two numbers.
    /// Unknown values are treated as `"balanced"`.
    #[serde(default = "default_companion_mode")]
    pub companion_mode: String,
    /// 长任务心跳阈值（分钟）。pending 的 `butler_tasks` 条目若被宠物
    /// 触碰过（updated_at > created_at + 5s）且距离上次更新 ≥ 该阈值，
    /// 在下次 proactive prompt 里以「[心跳]」段提醒 LLM 写进展或
    /// 标 done / error。0 = 关闭心跳。默认 30。
    #[serde(default = "default_task_heartbeat_minutes")]
    pub task_heartbeat_minutes: u32,
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

fn default_companion_mode() -> String {
    "balanced".to_string()
}

fn default_task_heartbeat_minutes() -> u32 {
    30
}

/// Iter R13: high-level companion-mode coefficients. Pure helper — given a
/// mode string and the user's base cooldown_seconds + chatty_day_threshold,
/// returns the effective values the gate should honor. Unknown modes
/// degrade to `"balanced"` (return base unchanged) so a typo / missing
/// field doesn't break the loop.
///
/// Returns `(effective_cooldown, effective_chatty)`. Math is integer
/// (`base × num / den`) so 0 stays 0 — preserves user's explicit opt-out
/// of either gate, same way `adapted_cooldown_seconds` does.
pub fn apply_companion_mode(mode: &str, base_cooldown: u64, base_chatty: u64) -> (u64, u64) {
    match mode {
        "chatty" => (base_cooldown / 2, base_chatty.saturating_mul(2)),
        "quiet" => (base_cooldown.saturating_mul(2), base_chatty / 2),
        _ => (base_cooldown, base_chatty), // balanced or unknown
    }
}

/// Iter R64: companion-mode dial for the R62 deep-focus hard-block
/// threshold. Chatty users want pet to keep trying past 90min; quiet
/// users want it to back off earlier (60 = matches R27 directive bound).
/// - chatty: base × 3/2 (e.g. 90 → 135)
/// - quiet: base × 2/3 (e.g. 90 → 60)
/// - balanced / unknown: base unchanged (90)
///
/// Integer math, saturating, so 0 stays 0 (lets a future "no-block"
/// opt-out cleanly). Pure / unit-testable.
pub fn apply_companion_mode_hard_block(mode: &str, base: u64) -> u64 {
    match mode {
        "chatty" => base.saturating_mul(3) / 2,
        "quiet" => base.saturating_mul(2) / 3,
        _ => base, // balanced or unknown
    }
}

impl ProactiveConfig {
    /// Iter R13 convenience: chatty-day threshold after applying
    /// companion_mode. Use this anywhere the prompt / gate / panel
    /// compares today_speech_count against the threshold so all four
    /// surfaces honor the user's mode choice consistently.
    pub fn effective_chatty_threshold(&self) -> u64 {
        apply_companion_mode(
            &self.companion_mode,
            self.cooldown_seconds,
            self.chatty_day_threshold,
        )
        .1
    }

    /// Iter R13 convenience: cooldown after applying companion_mode but
    /// *before* R7's ratio-driven adaptation. The gate path layers
    /// `adapted_cooldown_seconds` on top of this; other readers (panel
    /// chip, snapshot tooltips) can use this directly when they want the
    /// "base after user dial" without the feedback-ratio fine-tune.
    pub fn effective_cooldown_base(&self) -> u64 {
        apply_companion_mode(
            &self.companion_mode,
            self.cooldown_seconds,
            self.chatty_day_threshold,
        )
        .0
    }

    /// Iter R64: hard-block threshold (R62 const, default 90) after
    /// applying companion_mode. Caller passes `base` (typically
    /// `proactive::active_app::HARD_FOCUS_BLOCK_MINUTES`) so this stays
    /// decoupled from the proactive module's const layout.
    pub fn effective_hard_block_minutes(&self, base: u64) -> u64 {
        apply_companion_mode_hard_block(&self.companion_mode, base)
    }
}

/// 早安简报：由 `proactive::morning_briefing` 在每日固定时刻触发的"主动开
/// 口"。开关 + 触发时刻独立于 `proactive` 的常规节奏 — 用户可能关闭常规
/// 主动发言但仍想保留每日早安，所以两个 enabled 字段必须能各自取舍。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MorningBriefingConfig {
    /// 默认打开 — 这是产品亮点，理想路径就是用户能感知到。
    #[serde(default = "default_morning_briefing_enabled")]
    pub enabled: bool,
    /// 24h 制小时，0..=23。无效值（≥24）由门控函数静默拒绝，不 panic。
    #[serde(default = "default_morning_briefing_hour")]
    pub hour: u8,
    /// 0..=59。同上，越界静默拒绝。
    #[serde(default = "default_morning_briefing_minute")]
    pub minute: u8,
}

fn default_morning_briefing_enabled() -> bool {
    true
}

fn default_morning_briefing_hour() -> u8 {
    crate::proactive::MORNING_BRIEFING_DEFAULT_HOUR
}

fn default_morning_briefing_minute() -> u8 {
    crate::proactive::MORNING_BRIEFING_DEFAULT_MINUTE
}

impl Default for MorningBriefingConfig {
    fn default() -> Self {
        Self {
            enabled: default_morning_briefing_enabled(),
            hour: default_morning_briefing_hour(),
            minute: default_morning_briefing_minute(),
        }
    }
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
    /// Iter R17: how many days a `daily_review_YYYY-MM-DD` entry lingers before
    /// consolidate prunes it. R12 writes one per day; without pruning the
    /// ai_insights category grows unboundedly (1 entry / day → 365 / year).
    /// 30 = "last month of daily journals are kept; older ones trimmed". Set
    /// higher (90, 365) if you want to scroll back further; 0 disables pruning.
    #[serde(default = "default_stale_daily_review_days")]
    pub stale_daily_review_days: u32,
    /// 周报合成的"周日 closing 时刻"（本地时间小时 0-23）。在该时刻之后
    /// （含整点）的下一次 consolidate loop 唤醒会触发周报合成 → 写入
    /// `ai_insights/weekly_summary_YYYY-Www`。0 = 关闭周报。默认 20。
    /// 与 `enabled` 解耦：周报独立运行，即便 LLM 整理被禁用仍按时合成。
    #[serde(default = "default_weekly_summary_closing_hour")]
    pub weekly_summary_closing_hour: u8,
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

fn default_stale_daily_review_days() -> u32 {
    30
}

fn default_weekly_summary_closing_hour() -> u8 {
    crate::weekly_summary::DEFAULT_CLOSING_HOUR
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
            stale_daily_review_days: default_stale_daily_review_days(),
            weekly_summary_closing_hour: default_weekly_summary_closing_hour(),
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
            companion_mode: default_companion_mode(),
            task_heartbeat_minutes: default_task_heartbeat_minutes(),
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
    pub morning_briefing: MorningBriefingConfig,
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
    /// 工具审核覆盖：键是工具名，值是 `auto` / `always_review` / `always_approve`。
    /// 未列出的工具按 `auto`（跟分类器走）。值字符串而非 enum，让前向兼容
    /// 自然成立 — 见 `tool_review_policy::parse_mode`，不识别值默认退回 `auto`。
    #[serde(default)]
    pub tool_review_overrides: HashMap<String, String>,
    /// Live2D motion 自定义映射：把语义键（Tap / Flick / Flick3 / Idle）映射
    /// 到当前模型的实际 motion group 名。空 / 缺省 = 用语义键当 group 名
    /// （与内置 miku 模型行为一致）。键不在此 map 里时也走 fallback。
    /// 仅前端读 —— LLM 协议仍 emit Tap/Flick/Flick3/Idle 这 4 个语义键。
    #[serde(default)]
    pub motion_mapping: HashMap<String, String>,
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
            morning_briefing: MorningBriefingConfig::default(),
            memory_consolidate: MemoryConsolidateConfig::default(),
            chat: ChatConfig::default(),
            privacy: PrivacyConfig::default(),
            user_name: String::new(),
            tool_review_overrides: HashMap::new(),
            motion_mapping: HashMap::new(),
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

#[cfg(test)]
mod tests {
    use super::*;

    // -- Iter R13: apply_companion_mode --------------------------------------

    #[test]
    fn apply_companion_mode_balanced_returns_base_unchanged() {
        assert_eq!(apply_companion_mode("balanced", 1800, 5), (1800, 5));
    }

    #[test]
    fn apply_companion_mode_chatty_halves_cooldown_and_doubles_chatty() {
        // Chatty mode = "speak more freely": shorter gap between turns,
        // higher threshold before "chatty today" rule kicks in.
        assert_eq!(apply_companion_mode("chatty", 1800, 5), (900, 10));
    }

    #[test]
    fn apply_companion_mode_quiet_doubles_cooldown_and_halves_chatty() {
        // Quiet mode = "speak less": longer gap, lower threshold (chatty
        // rule fires sooner so the pet self-restrains earlier).
        assert_eq!(apply_companion_mode("quiet", 1800, 5), (3600, 2));
    }

    #[test]
    fn apply_companion_mode_unknown_falls_back_to_balanced() {
        // Typo / missing field — don't punish the user with surprise behavior.
        assert_eq!(apply_companion_mode("typo", 1800, 5), (1800, 5));
        assert_eq!(apply_companion_mode("", 1800, 5), (1800, 5));
    }

    // -- Iter R64: apply_companion_mode_hard_block ---------------------------

    #[test]
    fn apply_companion_mode_hard_block_balanced_returns_base() {
        assert_eq!(apply_companion_mode_hard_block("balanced", 90), 90);
    }

    #[test]
    fn apply_companion_mode_hard_block_chatty_extends_threshold() {
        // chatty users want pet to keep engaging past the default 90min;
        // 1.5x = 135min lets pet still try mid-Pomodoro pairs.
        assert_eq!(apply_companion_mode_hard_block("chatty", 90), 135);
        assert_eq!(apply_companion_mode_hard_block("chatty", 60), 90);
    }

    #[test]
    fn apply_companion_mode_hard_block_quiet_pulls_in_threshold() {
        // quiet users want pet to back off sooner; 2/3x = 60min matches
        // R27's existing directive boundary so soft + hard transitions
        // coincide for quiet-mode users.
        assert_eq!(apply_companion_mode_hard_block("quiet", 90), 60);
        assert_eq!(apply_companion_mode_hard_block("quiet", 120), 80);
    }

    #[test]
    fn apply_companion_mode_hard_block_unknown_falls_back_to_balanced() {
        assert_eq!(apply_companion_mode_hard_block("typo", 90), 90);
        assert_eq!(apply_companion_mode_hard_block("", 90), 90);
    }

    #[test]
    fn apply_companion_mode_hard_block_zero_base_stays_zero() {
        // 0 base = "no hard block" opt-out; multipliers preserve that
        // (consistent with apply_companion_mode integer math semantics).
        assert_eq!(apply_companion_mode_hard_block("chatty", 0), 0);
        assert_eq!(apply_companion_mode_hard_block("quiet", 0), 0);
        assert_eq!(apply_companion_mode_hard_block("balanced", 0), 0);
    }

    #[test]
    fn apply_companion_mode_zero_base_stays_zero() {
        // User-explicit cooldown=0 (no gate) must NOT be re-enabled by mode.
        // Same invariant as feedback_history::adapted_cooldown_seconds.
        assert_eq!(apply_companion_mode("chatty", 0, 0), (0, 0));
        assert_eq!(apply_companion_mode("quiet", 0, 0), (0, 0));
    }

    #[test]
    fn apply_companion_mode_quiet_overflow_clamps_via_saturating() {
        // Quiet mode multiplies cooldown by 2; saturating_mul ensures we
        // don't wrap at u64::MAX. (Not realistic in production but the
        // contract should be safe.)
        let (cooldown, _) = apply_companion_mode("quiet", u64::MAX, 5);
        assert_eq!(cooldown, u64::MAX, "saturating_mul keeps it pinned");
    }
}
