use crate::commands::settings::get_settings;

pub struct AiConfig {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
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
        })
    }
}
