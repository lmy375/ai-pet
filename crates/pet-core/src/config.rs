use crate::settings::{get_settings, AgentConfig, AppSettings};

#[derive(Clone)]
pub struct AiConfig {
    /// Which agent this config belongs to. Threaded through the whole chat
    /// pipeline (via `ToolContext.config`) so memory, prompt, MCP routing and
    /// session ids all resolve to the right agent.
    pub agent_id: String,
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    /// Wire protocol to speak, from the model-pool entry. Empty = infer from
    /// the model name. See `crate::provider::kind`.
    pub provider: String,
    /// Context-window size (tokens) used as the denominator for the context
    /// usage ring. The standard OpenAI API doesn't expose this, so it's a config
    /// value (`ModelConfig::context_window`).
    pub context_window: u32,
    /// Tavily API key for the `web_search` tool. Empty = web search disabled.
    pub search_api_key: String,
    /// Names of the global MCP servers this agent may call, in its configured
    /// order. Resolved here so the chat loop can ask the connection pool for
    /// this agent's tools without re-reading settings mid-turn.
    pub mcp_servers: Vec<String>,
    /// Reasoning control, from the model-pool entry. See
    /// `ModelConfig::reasoning`.
    pub reasoning: String,
}

impl AiConfig {
    /// Build the config for the active agent (the one answering the desktop chat
    /// window). Used by the `chat` command.
    pub fn from_settings() -> Result<Self, String> {
        let settings = get_settings()?;
        let agent = settings
            .active_agent_config()
            .ok_or_else(|| "No agent configured. Open Settings to add one.".to_string())?;
        Self::build(&settings, agent)
    }

    /// Build the config for a specific agent. Used by the heartbeat scheduler and
    /// Telegram bots, which run per-agent regardless of which one is active. The
    /// model pool and the `web_search` key are global, so settings are re-read
    /// here rather than passed in.
    pub fn from_agent(agent: &AgentConfig) -> Result<Self, String> {
        let settings = get_settings()?;
        Self::build(&settings, agent)
    }

    /// Resolve an agent's `model` reference against the global pool. This is the
    /// single place that resolution happens — everything downstream sees the
    /// flattened result and never knows the pool exists.
    fn build(settings: &AppSettings, agent: &AgentConfig) -> Result<Self, String> {
        let model = settings.model_for(agent)?;
        if model.api_key.is_empty() {
            return Err(format!(
                "模型 \"{}\" 还没有填 API Key。打开设置 → 全局 → 模型配置。",
                agent.model
            ));
        }
        Ok(Self {
            agent_id: agent.id.clone(),
            api_key: model.api_key.clone(),
            base_url: model.api_base.clone(),
            model: model.model.clone(),
            provider: model.provider.clone(),
            context_window: model.context_window,
            search_api_key: settings.search_api_key.clone(),
            mcp_servers: settings
                .mcp_for(agent)
                .into_iter()
                .map(|(name, _)| name.to_string())
                .collect(),
            reasoning: model.reasoning.clone(),
        })
    }
}
