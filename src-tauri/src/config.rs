use std::env;

pub struct AiConfig {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
}

impl AiConfig {
    pub fn from_env() -> Result<Self, String> {
        // Try loading .env from project root (parent of src-tauri)
        let _ = dotenvy::from_filename("../.env").or_else(|_| dotenvy::dotenv());
        Ok(Self {
            api_key: env::var("OPENAI_API_KEY")
                .map_err(|_| "OPENAI_API_KEY environment variable not set".to_string())?,
            base_url: env::var("OPENAI_BASE_URL")
                .unwrap_or_else(|_| "https://api.openai.com/v1".into()),
            model: env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o-mini".into()),
        })
    }
}
