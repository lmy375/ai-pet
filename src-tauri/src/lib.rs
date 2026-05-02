mod commands;
mod config;
mod consolidate;
mod decision_log;
mod focus_mode;
mod focus_tracker;
mod input_idle;
mod log_rotation;
mod mcp;
mod mood;
mod proactive;
mod speech_history;
mod telegram;
mod tools;
mod wake_detector;

use commands::debug::{log_dir, new_process_counters, LogStore};
use commands::shell::ShellStore;
use std::collections::HashMap;
use std::sync::Arc;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Ensure log directory exists
    let _ = std::fs::create_dir_all(log_dir());

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(LogStore(Arc::new(std::sync::Mutex::new(Vec::new()))))
        .manage(ShellStore(Arc::new(std::sync::Mutex::new(HashMap::new()))))
        .manage(new_process_counters())
        .manage(decision_log::new_decision_log())
        .manage(wake_detector::new_wake_detector())
        .manage(mcp::new_mcp_store())
        .manage(telegram::new_telegram_store())
        .manage(proactive::new_interaction_clock())
        .setup(|app| {
            // Initialize MCP servers from config on app start
            let mcp_store = app.state::<mcp::McpManagerStore>().inner().clone();
            let telegram_store = app.state::<telegram::TelegramStore>().inner().clone();
            let log_store = app.state::<LogStore>().inner().clone();
            let shell_store = app.state::<ShellStore>().inner().clone();
            let process_counters = app
                .state::<commands::debug::ProcessCountersStore>()
                .inner()
                .clone();
            let app_handle_for_tg = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let settings = commands::settings::get_settings().unwrap_or_default();

                // Initialize MCP servers
                if !settings.mcp_servers.is_empty() {
                    let manager = mcp::McpManager::start_from_settings(&settings).await;
                    *mcp_store.lock().await = manager;
                    eprintln!("MCP servers initialized");
                }

                // Initialize Telegram bot
                let tg = &settings.telegram;
                if tg.enabled && !tg.bot_token.is_empty() {
                    let mcp_clone = mcp_store.clone();
                    match telegram::bot::TelegramBot::start(
                        tg.clone(),
                        mcp_clone,
                        log_store,
                        shell_store,
                        process_counters,
                        app_handle_for_tg,
                    )
                    .await
                    {
                        Ok(bot) => {
                            *telegram_store.lock().await = Some(bot);
                            eprintln!("Telegram bot started");
                        }
                        Err(e) => eprintln!("Failed to start Telegram bot: {}", e),
                    }
                }
            });

            // Start proactive engagement loop (reads settings each tick).
            proactive::spawn(app.handle().clone());

            // Start memory consolidation loop (long-period, opt-in).
            consolidate::spawn(app.handle().clone());

            // Track Focus mode transitions to disk for long-term insights.
            focus_tracker::spawn(app.handle().clone());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::chat::chat,
            commands::settings::get_settings,
            commands::settings::save_settings,
            commands::settings::get_config_raw,
            commands::settings::save_config_raw,
            commands::settings::get_soul,
            commands::settings::save_soul,
            commands::window::open_panel,
            commands::window::open_debug,
            commands::debug::get_logs,
            commands::debug::append_log,
            commands::debug::clear_logs,
            commands::debug::get_llm_logs,
            commands::debug::get_cache_stats,
            commands::debug::reset_cache_stats,
            commands::debug::get_mood_tag_stats,
            commands::debug::reset_mood_tag_stats,
            decision_log::get_proactive_decisions,
            speech_history::get_recent_speeches,
            proactive::get_tone_snapshot,
            proactive::get_pending_reminders,
            consolidate::trigger_consolidate,
            proactive::trigger_proactive_turn,
            commands::shell::check_shell_status,
            commands::mcp::get_mcp_status,
            commands::mcp::reconnect_mcp,
            commands::session::list_sessions,
            commands::session::load_session,
            commands::session::save_session,
            commands::session::create_session,
            commands::session::delete_session,
            commands::telegram::get_telegram_status,
            commands::telegram::reconnect_telegram,
            commands::memory::memory_list,
            commands::memory::memory_search,
            commands::memory::memory_edit,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
