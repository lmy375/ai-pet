use crate::commands::settings::get_settings;

#[derive(Clone)]
pub struct AiConfig {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    /// Context-window size (tokens) used as the denominator for the context
    /// usage ring. The standard OpenAI API doesn't expose this, so it's a config
    /// value (`AppSettings::context_window`).
    pub context_window: u32,
    /// Tavily API key for the `web_search` tool. Empty = web search disabled.
    pub search_api_key: String,
}

impl AiConfig {
    pub fn from_settings() -> Result<Self, String> {
        let settings = get_settings()?;
        if settings.api_key.is_empty() {
            return Err("API Key not configured. Open Settings to set it.".to_string());
        }
        Ok(Self {
            api_key: settings.api_key,
            base_url: settings.api_base,
            model: settings.model,
            context_window: settings.context_window,
            search_api_key: settings.search_api_key,
        })
    }
}
