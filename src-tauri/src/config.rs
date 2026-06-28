use crate::commands::settings::{get_settings, AgentConfig};

#[derive(Clone)]
pub struct AiConfig {
    /// Which agent this config belongs to. Threaded through the whole chat
    /// pipeline (via `ToolContext.config`) so memory, prompt, MCP routing and
    /// session ids all resolve to the right agent.
    pub agent_id: String,
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    /// Context-window size (tokens) used as the denominator for the context
    /// usage ring. The standard OpenAI API doesn't expose this, so it's a config
    /// value (`AgentConfig::context_window`).
    pub context_window: u32,
    /// Tavily API key for the `web_search` tool. Empty = web search disabled.
    pub search_api_key: String,
}

impl AiConfig {
    /// Build the config for the active agent (the one answering the desktop chat
    /// window). Used by the `chat` command.
    pub fn from_settings() -> Result<Self, String> {
        let settings = get_settings()?;
        let agent = settings
            .active_agent_config()
            .ok_or_else(|| "No agent configured. Open Settings to add one.".to_string())?;
        Self::build(agent, &settings.search_api_key)
    }

    /// Build the config for a specific agent. Used by the heartbeat scheduler and
    /// Telegram bots, which run per-agent regardless of which one is active. The
    /// `web_search` key is global, so it's read from settings here.
    pub fn from_agent(agent: &AgentConfig) -> Result<Self, String> {
        let search_api_key = get_settings().map(|s| s.search_api_key).unwrap_or_default();
        Self::build(agent, &search_api_key)
    }

    fn build(agent: &AgentConfig, search_api_key: &str) -> Result<Self, String> {
        if agent.api_key.is_empty() {
            return Err(format!(
                "Agent \"{}\" has no API Key. Open Settings to set it.",
                agent.name
            ));
        }
        Ok(Self {
            agent_id: agent.id.clone(),
            api_key: agent.api_key.clone(),
            base_url: agent.api_base.clone(),
            model: agent.model.clone(),
            context_window: agent.context_window,
            search_api_key: search_api_key.to_string(),
        })
    }
}
