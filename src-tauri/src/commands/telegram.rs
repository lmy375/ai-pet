use serde::Serialize;
use tauri::State;

use crate::commands::debug::LogStore;
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

/// (Re)start Telegram bots for every agent whose telegram is enabled with a
/// token. Stops all currently-running bots first. Shared by app startup and the
/// `reconnect_telegram` command. Errors per agent are logged, not fatal.
pub async fn restart_all_bots(
    telegram_store: &TelegramStore,
    mcp_store: McpManagerStore,
    log_store: LogStore,
    shell_store: ShellStore,
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
            mcp_store.clone(),
            LogStore(log_store.0.clone()),
            ShellStore(shell_store.0.clone()),
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
    mcp_store: State<'_, McpManagerStore>,
    log_store: State<'_, LogStore>,
    shell_store: State<'_, ShellStore>,
) -> Result<(), String> {
    restart_all_bots(
        telegram_store.inner(),
        mcp_store.inner().clone(),
        LogStore(log_store.0.clone()),
        ShellStore(shell_store.0.clone()),
    )
    .await;
    Ok(())
}
