use serde::Serialize;
use std::sync::Arc;
use tauri::State;

use pet_core::logging::LogStore;
use pet_core::settings::get_settings;
use pet_core::turn::TurnRunner;
use crate::commands::chat::TurnStore;
use crate::telegram::bot::TelegramBot;
use crate::telegram::TelegramStore;

#[derive(Clone, Serialize)]
pub struct TelegramStatus {
    pub running: bool,
    pub error: Option<String>,
}

/// (Re)start Telegram bots for every agent whose telegram is enabled with a
/// token. Stops all currently-running bots first. Shared by app startup and the
/// `reconnect_telegram` command. Errors per agent are logged, not fatal.
pub async fn restart_all_bots(
    telegram_store: &TelegramStore,
    turns: Arc<TurnRunner>,
    log_store: LogStore,
) {
    {
        let mut guard = telegram_store.lock().await;
        for (_, bot) in guard.drain() {
            bot.stop();
        }
    }

    let settings = match get_settings() {
        Ok(s) => s,
        Err(_) => return,
    };

    for agent in &settings.agents {
        let tg = &agent.telegram;
        if !tg.enabled || tg.bot_token.is_empty() {
            continue;
        }
        match TelegramBot::start(
            agent.id.clone(),
            tg.clone(),
            turns.clone(),
            log_store.clone(),
        )
        .await
        {
            Ok(bot) => {
                telegram_store.lock().await.insert(agent.id.clone(), bot);
                eprintln!("Telegram bot started for agent {}", agent.id);
            }
            Err(e) => eprintln!("Failed to start Telegram bot for agent {}: {}", agent.id, e),
        }
    }
}

#[tauri::command]
pub async fn get_telegram_status(
    agent_id: String,
    telegram_store: State<'_, TelegramStore>,
) -> Result<TelegramStatus, String> {
    let guard = telegram_store.lock().await;
    Ok(TelegramStatus {
        running: guard.contains_key(&agent_id),
        error: None,
    })
}

#[tauri::command]
pub async fn reconnect_telegram(
    telegram_store: State<'_, TelegramStore>,
    turns: State<'_, TurnStore>,
    log_store: State<'_, LogStore>,
) -> Result<(), String> {
    restart_all_bots(
        telegram_store.inner(),
        turns.0.clone(),
        log_store.inner().clone(),
    )
    .await;
    Ok(())
}