use serde::Serialize;
use tauri::{AppHandle, State};

use crate::commands::debug::{CacheCountersStore, LogStore};
use crate::commands::settings::get_settings;
use crate::commands::shell::ShellStore;
use crate::mcp::McpManagerStore;
use crate::telegram::bot::TelegramBot;
use crate::telegram::TelegramStore;

#[derive(Clone, Serialize)]
pub struct TelegramStatus {
    pub running: bool,
    pub error: Option<String>,
}

#[tauri::command]
pub async fn get_telegram_status(
    telegram_store: State<'_, TelegramStore>,
) -> Result<TelegramStatus, String> {
    let guard = telegram_store.lock().await;
    Ok(TelegramStatus {
        running: guard.is_some(),
        error: None,
    })
}

#[tauri::command]
pub async fn reconnect_telegram(
    app: AppHandle,
    telegram_store: State<'_, TelegramStore>,
    mcp_store: State<'_, McpManagerStore>,
    log_store: State<'_, LogStore>,
    shell_store: State<'_, ShellStore>,
    cache_counters: State<'_, CacheCountersStore>,
) -> Result<TelegramStatus, String> {
    // Stop existing bot
    {
        let mut guard = telegram_store.lock().await;
        if let Some(bot) = guard.take() {
            bot.stop();
        }
    }

    let settings = get_settings()?;
    let tg = &settings.telegram;

    if !tg.enabled || tg.bot_token.is_empty() {
        return Ok(TelegramStatus {
            running: false,
            error: None,
        });
    }

    let mcp = mcp_store.inner().clone();
    let logs = LogStore(log_store.0.clone());
    let shells = ShellStore(shell_store.0.clone());
    let counters = cache_counters.inner().clone();

    match TelegramBot::start(tg.clone(), mcp, logs, shells, counters, app).await {
        Ok(bot) => {
            *telegram_store.lock().await = Some(bot);
            Ok(TelegramStatus {
                running: true,
                error: None,
            })
        }
        Err(e) => Ok(TelegramStatus {
            running: false,
            error: Some(e),
        }),
    }
}
