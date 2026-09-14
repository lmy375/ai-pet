use genai::chat::ChatRequest;
use genai::resolver::AuthData;
use genai::{ModelIden, ServiceTarget};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

/// One MCP server, defined once globally (`mcp_servers` in config.yaml) and
/// referenced by name from `AgentConfig::mcp`. There is no per-server "enabled"
/// flag: an agent either lists a server or it doesn't, and the connection pool
/// only starts the servers somebody references.
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
    pub headers: BTreeMap<String, String>,
    /// Environment variables for the process (stdio transport)
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

impl Default for McpServerConfig {
    fn default() -> Self {
        Self {
            transport: default_transport(),
            command: String::new(),
            args: Vec::new(),
            url: String::new(),
            headers: BTreeMap::new(),
            env: BTreeMap::new(),
        }
    }
}

fn default_transport() -> String {
    "stdio".to_string()
}

/// One entry of the global model pool (`models` in config.yaml), keyed by a
/// user-chosen name. Everything needed to reach a model lives here — endpoint,
/// credentials, wire protocol, context window and reasoning effort — so an
/// agent only has to name one (`AgentConfig::model`), and switching models in
/// the chat switches the context window and thinking budget with it.
///
/// The same underlying model at two reasoning strengths is two entries
/// (`gpt-5.6-fast` / `gpt-5.6-deep`), which is the point: that choice belongs
/// to the model, not to the persona using it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    /// Which LLM wire protocol to speak: "" (auto) / "openai" / "openai_resp" /
    /// "anthropic" / "gemini" / "deepseek" / "xai" / "groq" / "ollama" /
    /// "openrouter" / "together" / "cohere" / "zai" / "moonshot" / "minimax".
    /// Auto asks genai to infer from the model name, which is a static prefix
    /// map (`gpt*`→OpenAI, `claude*`→Anthropic, `gemini*`→Gemini, …) that falls
    /// back to Ollama when nothing matches — so it guesses wrong for
    /// gateway-hosted or renamed models. Set it explicitly there.
    /// See `crate::provider`.
    ///
    /// Defaults to `openai` rather than auto: every model this app has ever
    /// talked to spoke OpenAI chat-completions by definition, and
    /// auto-detection would silently re-route them (a gateway-hosted
    /// `claude-sonnet-4-6` infers Anthropic, `GPT-5.5` matches nothing and
    /// falls back to Ollama). Auto stays available as an explicit choice.
    #[serde(default = "default_provider")]
    pub provider: String,
    #[serde(default = "default_api_base")]
    pub api_base: String,
    #[serde(default)]
    pub api_key: String,
    /// The model id sent on the wire (the map key is just the display name).
    #[serde(default = "default_model")]
    pub model: String,
    /// Context-window size (tokens), the denominator of the chat context-usage
    /// ring. Not exposed by the OpenAI API, so it's user-configured.
    #[serde(default = "default_context_window")]
    pub context_window: u32,
    /// How hard the model should think, as genai's single `ReasoningEffort`:
    /// `""` (send nothing, model default) / `none` / `minimal` / `low` /
    /// `medium` / `high` / `xhigh` / `max`, or a plain token count for an
    /// explicit thinking budget (e.g. `4096`).
    ///
    /// Each adapter renders this to its own wire format — `reasoning_effort`
    /// for OpenAI, `output_config.effort` or `thinking` for Anthropic,
    /// `thinkingConfig.thinkingBudget` for Gemini. A numeric budget is only
    /// native to Anthropic/Gemini; see `llm::chat_options` for how it reaches
    /// OpenAI-protocol gateways.
    #[serde(default)]
    pub reasoning: String,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            provider: default_provider(),
            api_base: default_api_base(),
            api_key: String::new(),
            model: default_model(),
            context_window: default_context_window(),
            reasoning: String::new(),
        }
    }
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

/// One configurable agent: a persona/memory (under `memory/<id>/`) plus
/// references into the global pools — which model it speaks through and which
/// MCP servers it may use — and its own Telegram bot and heartbeat schedule.
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
    /// Name of the `models` entry this agent talks through. Empty or unknown is
    /// a configuration error reported at chat time, not silently patched.
    #[serde(default)]
    pub model: String,
    /// Names of the `mcp_servers` entries this agent may call. Unknown names are
    /// ignored (see `AppSettings::mcp_for`).
    #[serde(default)]
    pub mcp: Vec<String>,
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
            model: String::new(),
            mcp: Vec::new(),
            telegram: TelegramConfig::default(),
            heartbeat_enabled: false,
            heartbeat_interval: default_heartbeat_interval(),
            heartbeat_context_turns: default_heartbeat_context_turns(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    /// The global model pool, keyed by display name. Agents reference an entry
    /// by name; the in-chat model switcher picks from these keys.
    #[serde(default)]
    pub models: BTreeMap<String, ModelConfig>,
    /// The global MCP server pool, keyed by name. Agents reference entries by
    /// name; one connection is shared by every agent that lists it.
    #[serde(default)]
    pub mcp_servers: BTreeMap<String, McpServerConfig>,
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
    /// Directory scanned for Agent Skills — every subdirectory holding a
    /// `SKILL.md` is one skill, offered to every agent. Empty = `~/.agents/skills`;
    /// a leading `~` is expanded (see `skills::resolve_skills_dir`).
    #[serde(default)]
    pub skills_dir: String,
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

    /// Look up a model-pool entry by name.
    pub fn model(&self, name: &str) -> Option<&ModelConfig> {
        self.models.get(name)
    }

    /// Resolve the model an agent speaks through. A missing or dangling
    /// reference is a real configuration error — reported here, in the one
    /// place that resolves it, rather than substituted with a default that
    /// would quietly talk to the wrong endpoint.
    pub fn model_for(&self, agent: &AgentConfig) -> Result<&ModelConfig, String> {
        if agent.model.trim().is_empty() {
            return Err(format!(
                "Agent \"{}\" 还没有选择模型。打开设置 → 全局 → 模型配置。",
                agent.name
            ));
        }
        self.model(&agent.model).ok_or_else(|| {
            format!(
                "Agent \"{}\" 引用了不存在的模型 \"{}\"。打开设置 → 全局 → 模型配置。",
                agent.name, agent.model
            )
        })
    }

    /// The MCP servers an agent may call, in the order it lists them. Names with
    /// no matching entry are skipped: a deleted server should cost that agent
    /// its tools, not its ability to chat.
    pub fn mcp_for<'a>(&'a self, agent: &'a AgentConfig) -> Vec<(&'a str, &'a McpServerConfig)> {
        agent
            .mcp
            .iter()
            .filter_map(|name| self.mcp_servers.get_key_value(name.as_str()))
            .map(|(name, config)| (name.as_str(), config))
            .collect()
    }

    /// Every MCP server name referenced by any agent — what the GUI connects at
    /// startup, so nothing spawns for a server nobody uses.
    pub fn referenced_mcp_servers(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .agents
            .iter()
            .flat_map(|a| a.mcp.iter())
            .filter(|name| self.mcp_servers.contains_key(name.as_str()))
            .cloned()
            .collect();
        names.sort();
        names.dedup();
        names
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

/// An agent's human-readable name by id, falling back to the default name when
/// settings can't be read or the id is unknown. Used to tell the agent its own
/// name in the system prompt.
pub fn agent_name(id: &str) -> String {
    get_settings()
        .ok()
        .and_then(|s| s.agent(id).map(|a| a.name.clone()))
        .unwrap_or_else(default_agent_name)
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

fn default_provider() -> String {
    "openai".to_string()
}

fn default_api_base() -> String {
    "https://api.openai.com/v1/".to_string()
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
            models: BTreeMap::new(),
            mcp_servers: BTreeMap::new(),
            live_2d_model_path: default_model_path(),
            language: default_language(),
            gallery_dir: String::new(),
            gallery_enabled: false,
            gallery_interval: default_gallery_interval(),
            search_api_key: String::new(),
            skills_dir: String::new(),
            active_agent: default_agent_id(),
            agents: default_agents(),
            window: None,
        }
    }
}

fn config_path() -> Result<PathBuf, String> {
    Ok(crate::common::config_dir()?.join("config.yaml"))
}

/// Fetch the available model ids for an agent's provider. Routed through genai
/// so the list comes from whichever protocol the agent actually speaks — an
/// Anthropic or Gemini endpoint has no OpenAI-style `/models` route, and asking
/// for one returned an error that looked like a bad API key.
pub async fn list_models(
    api_base: String,
    api_key: String,
    provider: String,
    model: String,
) -> Result<Vec<String>, String> {
    if api_base.trim().is_empty() {
        return Err("请先填写 API Base URL".to_string());
    }
    let kind = crate::provider::kind(&provider, &model);
    let config = (crate::llm::endpoint(&api_base), AuthData::from_single(api_key));
    let mut ids = crate::llm::client()
        .all_model_names(kind, config)
        .await
        .map_err(|e| format!("请求失败: {e}"))?;
    ids.sort();
    Ok(ids)
}

/// Send a minimal chat request to verify the model is reachable and usable.
/// Goes through the same genai path a real chat turn takes, so a green check
/// here means the configured provider/endpoint/model combination actually
/// works — testing over raw OpenAI chat-completions would pass against a
/// gateway while the agent's real Anthropic or Gemini requests still failed.
pub async fn test_model(
    api_base: String,
    api_key: String,
    model: String,
    provider: String,
) -> Result<(), String> {
    if api_base.trim().is_empty() {
        return Err("请先填写 API Base URL".to_string());
    }
    if model.trim().is_empty() {
        return Err("请先选择模型".to_string());
    }
    let target = ServiceTarget {
        endpoint: crate::llm::endpoint(&api_base),
        auth: AuthData::from_single(api_key),
        model: ModelIden::new(
            crate::provider::kind(&provider, &model),
            model.trim().to_string(),
        ),
    };
    crate::llm::client()
        .exec_chat(target, ChatRequest::from_user("ping"), None)
        .await
        .map_err(|e| format!("请求失败: {e}"))?;
    Ok(())
}

/// Ensure the config dir exists and return its path — for "open in file
/// manager" UI affordances, which live in the interface layer.
pub fn ensure_config_dir() -> Result<std::path::PathBuf, String> {
    let dir = crate::common::config_dir()?;
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create config dir: {}", e))?;
    Ok(dir)
}

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
        let _ = crate::memory::ensure_memory_files(&agent.id);
        let _ = crate::heartbeat_file::ensure_heartbeat_file(&agent.id);
    }
}

/// Write raw YAML text to config.yaml, creating the parent dir if needed.
fn write_config_file(yaml: &str) -> Result<(), String> {
    crate::common::write_text(&config_path()?, yaml)
}

/// Serialize `settings`, write it to config.yaml, then ensure every agent's
/// memory dirs exist. Interface layers that keep in-memory copies (the two GUI
/// windows) must broadcast their own change notification after calling any of
/// the settings-mutating functions below (the Tauri layer emits
/// `settings-changed`; the CLI reloads per turn and needs nothing).
pub fn save_settings(settings: &AppSettings) -> Result<(), String> {
    let yaml = serde_yaml::to_string(settings)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;
    write_config_file(&yaml)?;
    ensure_agent_dirs(settings);
    Ok(())
}

/// Switch the active agent (the one answering the chat) without rewriting the
/// whole settings object. Chat history is global, so this only changes who
/// responds next.
pub fn set_active_agent(id: &str) -> Result<(), String> {
    let mut settings = get_settings()?;
    if settings.agent(id).is_none() {
        return Err(format!("Unknown agent: {}", id));
    }
    settings.active_agent = id.to_string();
    save_settings(&settings)
}

/// Point one agent at a different entry of the global model pool, without
/// rewriting the whole settings object — used by the in-chat model switcher.
pub fn set_agent_model(id: &str, model: &str) -> Result<(), String> {
    let mut settings = get_settings()?;
    if !settings.models.contains_key(model) {
        return Err(format!("Unknown model: {}", model));
    }
    let agent = settings
        .agents
        .iter_mut()
        .find(|a| a.id == id)
        .ok_or_else(|| format!("Unknown agent: {}", id))?;
    agent.model = model.to_string();
    save_settings(&settings)
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

pub fn save_config_raw(content: &str) -> Result<(), String> {
    // Validate YAML parses as AppSettings before saving. Write the user's exact
    // text (preserving comments/formatting) rather than re-serializing.
    let settings: AppSettings = serde_yaml::from_str(content)
        .map_err(|e| format!("YAML 解析失败: {}", e))?;
    write_config_file(content)?;
    ensure_agent_dirs(&settings);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The on-disk contract: two global pools, agents that only reference them.
    /// Pinned as YAML because this is the file people hand-edit.
    const CONFIG: &str = r#"
models:
  fast:
    provider: openai
    api_base: https://litellm.example.com
    api_key: sk-test
    model: gpt-5.6-sol-sub2api
    context_window: 200000
    reasoning: medium
mcp_servers:
  fs:
    transport: stdio
    command: npx
agents:
  - id: default
    name: 小宠
    model: fast
    mcp: [fs, gone]
  - id: other
    name: 别的
    model: missing
"#;

    fn parse() -> AppSettings {
        serde_yaml::from_str(CONFIG).expect("config parses")
    }

    #[test]
    fn agent_resolves_its_model_through_the_global_pool() {
        let s = parse();
        let model = s.model_for(s.agent("default").unwrap()).unwrap();
        // The map key is the display name; `model` is what goes on the wire.
        assert_eq!(model.model, "gpt-5.6-sol-sub2api");
        assert_eq!(model.context_window, 200000);
        assert_eq!(model.reasoning, "medium");
    }

    #[test]
    fn dangling_or_empty_model_reference_is_an_error_not_a_default() {
        // Substituting a default here would silently talk to api.openai.com with
        // no key; the agent must refuse instead.
        let s = parse();
        assert!(s.model_for(s.agent("other").unwrap()).is_err());

        let mut s = s;
        s.agents[0].model = String::new();
        assert!(s.model_for(&s.agents[0]).is_err());
    }

    #[test]
    fn unknown_mcp_reference_is_skipped_not_fatal() {
        // A server deleted from the global pool should cost the agent its tools,
        // not its ability to chat.
        let s = parse();
        let servers = s.mcp_for(s.agent("default").unwrap());
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].0, "fs");
        assert_eq!(servers[0].1.command, "npx");
    }

    #[test]
    fn only_referenced_servers_are_started() {
        // What the GUI connects at boot: nothing spawns for a server no agent
        // lists, and a server two agents share is listed once.
        let mut s = parse();
        s.mcp_servers.insert("idle".to_string(), McpServerConfig::default());
        s.agents[1].mcp = vec!["fs".to_string()];
        assert_eq!(s.referenced_mcp_servers(), vec!["fs".to_string()]);
    }
}
