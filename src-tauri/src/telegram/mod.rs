pub mod bot;

use bot::TelegramBot;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// One running Telegram bot per agent (keyed by agent id). Each agent has its own
/// bot token, persona/memory and session, so all enabled agents' bots run at once.
pub type TelegramStore = Arc<Mutex<HashMap<String, TelegramBot>>>;

pub fn new_telegram_store() -> TelegramStore {
    Arc::new(Mutex::new(HashMap::new()))
}
