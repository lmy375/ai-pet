mod commands;
mod common;
mod config;
mod mcp;
mod telegram;
mod tools;

use commands::debug::{log_dir, LogStore};
use commands::shell::ShellStore;
use std::sync::Arc;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Ensure log directory exists
    let _ = std::fs::create_dir_all(log_dir());

    // Ensure each configured agent's memory dir + mandatory files exist
    // (`memory/<id>/{SOUL,USER,MEMORY,HEARTBEAT}.md`).
    if let Ok(settings) = commands::settings::get_settings() {
        for agent in &settings.agents {
            let _ = commands::memory::ensure_memory_files(&agent.id);
            let _ = commands::heartbeat_file::ensure_heartbeat_file(&agent.id);
        }
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .manage(LogStore(Arc::new(std::sync::Mutex::new(Vec::new()))))
        .manage(ShellStore(Arc::new(std::sync::Mutex::new(
            commands::shell::load_persisted_tasks(),
        ))))
        .manage(mcp::new_mcp_store())
        .manage(telegram::new_telegram_store())
        .manage(commands::window::ActiveWindow(std::sync::Mutex::new("main".to_string())))
        .setup(|app| {
            // Restore the pet window to its last position (and show it — it starts
            // hidden so it's positioned before appearing, avoiding a center flash).
            commands::window::restore_main_window(app.handle());

            // Initialize MCP servers from config on app start
            let mcp_store = app.state::<mcp::McpManagerStore>().inner().clone();
            let telegram_store = app.state::<telegram::TelegramStore>().inner().clone();
            let log_store = app.state::<LogStore>().inner().clone();
            let shell_store = app.state::<ShellStore>().inner().clone();
            tauri::async_runtime::spawn(async move {
                let settings = commands::settings::get_settings().unwrap_or_default();

                // Initialize each agent's MCP servers into its own manager.
                {
                    let mut managers = mcp_store.lock().await;
                    for agent in &settings.agents {
                        if agent.mcp_servers.is_empty() {
                            continue;
                        }
                        let manager = mcp::McpManager::start_from_agent(agent).await;
                        managers.insert(agent.id.clone(), manager);
                    }
                    if !managers.is_empty() {
                        eprintln!("MCP servers initialized");
                    }
                }

                // Start a Telegram bot for every agent with telegram enabled.
                commands::telegram::restart_all_bots(
                    &telegram_store,
                    mcp_store.clone(),
                    log_store,
                    shell_store,
                )
                .await;
            });

            // Start the scheduled-heartbeat loop (it checks settings each tick, so
            // it's a no-op until the owner enables it in Settings).
            commands::heartbeat::start_scheduler(
                app.handle().clone(),
                app.state::<LogStore>().inner().clone(),
                app.state::<ShellStore>().inner().clone(),
                app.state::<mcp::McpManagerStore>().inner().clone(),
            );
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::chat::chat,
            commands::settings::get_settings,
            commands::settings::save_settings,
            commands::settings::get_config_raw,
            commands::settings::save_config_raw,
            commands::settings::set_active_agent,
            commands::memory::get_soul,
            commands::memory::save_soul,
            commands::memory::get_user,
            commands::memory::save_user,
            commands::memory::get_memory,
            commands::memory::save_memory,
            commands::memory::open_memory_dir,
            commands::heartbeat_file::get_heartbeat,
            commands::heartbeat_file::save_heartbeat,
            commands::settings::open_config_dir,
            commands::settings::open_path,
            commands::settings::list_models,
            commands::settings::test_model,
            commands::gallery::default_gallery_dir,
            commands::gallery::list_gallery_media,
            commands::window::open_panel,
            commands::window::open_debug,
            commands::window::open_devtools,
            commands::window::save_window_position,
            commands::window::set_active_window,
            commands::debug::get_logs,
            commands::debug::clear_logs,
            commands::debug::get_llm_logs,
            commands::shell::check_task_status,
            commands::shell::list_tasks,
            commands::shell::kill_task,
            commands::mcp::get_mcp_status,
            commands::mcp::reconnect_mcp,
            commands::mcp::list_available_tools,
            commands::session::list_sessions,
            commands::session::set_active_session,
            commands::session::load_session,
            commands::session::save_session,
            commands::session::create_session,
            commands::session::rename_session,
            commands::session::delete_session,
            commands::telegram::get_telegram_status,
            commands::telegram::reconnect_telegram,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
