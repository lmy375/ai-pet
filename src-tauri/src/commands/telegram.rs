use serde::Serialize;
use tauri::{AppHandle, State};

use crate::commands::debug::{LogStore, ProcessCountersStore};
use crate::commands::settings::get_settings;
use crate::commands::shell::ShellStore;
use crate::mcp::McpManagerStore;
use crate::telegram::bot::TelegramBot;
use crate::telegram::warnings::{TgStartupWarning, TgStartupWarningStore};
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
    process_counters: State<'_, ProcessCountersStore>,
    warnings: State<'_, TgStartupWarningStore>,
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
    let counters = process_counters.inner().clone();
    let warnings_inner = warnings.inner().clone();

    match TelegramBot::start(
        tg.clone(),
        mcp,
        logs,
        shells,
        counters,
        app,
        warnings_inner,
    )
    .await
    {
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

/// 取启动期非 fatal 告警的快照（含 set_my_commands 失败 / bot_start 失
/// 败等）。前端 PanelDebug 5s 拉一次，若非空则展示一条橙色 banner。
#[tauri::command]
pub fn get_tg_startup_warnings(
    warnings: State<'_, TgStartupWarningStore>,
) -> Vec<TgStartupWarning> {
    crate::telegram::warnings::snapshot(warnings.inner())
}

/// 清空 TG 客户端的命令补全表。用户重命名 / 删除某条命令时旧名不会自动
/// 消失（覆盖型 set_my_commands 只在 reconnect 重注册时才覆盖），先调本
/// 命令清空 → 重连即拿到全新补全。
///
/// 实现走临时 Bot：从 settings 读 token 新建 teloxide Bot 调
/// `set_my_commands(vec![])`。不经 TelegramStore —— 那个 store 持的是
/// dispatcher shutdown token，启动后 Bot 已 move 进 dispatcher 拿不回；
/// 而 set_my_commands 是 HTTP API idempotent 操作，同 token 即认证同 bot，
/// 与 dispatcher 状态无关。
#[tauri::command]
pub async fn reset_tg_commands() -> Result<(), String> {
    let settings = get_settings()?;
    let token = settings.telegram.bot_token.trim();
    if token.is_empty() {
        return Err("Telegram bot_token 未配置".to_string());
    }
    use teloxide::prelude::Requester;
    let bot = teloxide::Bot::new(token);
    bot.set_my_commands(Vec::<teloxide::types::BotCommand>::new())
        .await
        .map_err(|e| format!("set_my_commands 清空失败: {}", e))?;
    Ok(())
}
