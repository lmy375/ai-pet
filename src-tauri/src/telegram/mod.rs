pub mod bot;
pub mod bulk_silent;
pub mod commands;
pub mod photo;
pub mod warnings;

use bot::TelegramBot;
use std::sync::Arc;
use tokio::sync::Mutex;

pub type TelegramStore = Arc<Mutex<Option<TelegramBot>>>;

pub fn new_telegram_store() -> TelegramStore {
    Arc::new(Mutex::new(None))
}
