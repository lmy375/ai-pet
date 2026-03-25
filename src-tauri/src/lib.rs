mod commands;
mod config;
mod tools;

use commands::debug::LogStore;
use commands::shell::ShellStore;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(LogStore(Arc::new(Mutex::new(Vec::new()))))
        .manage(ShellStore(Arc::new(Mutex::new(HashMap::new()))))
        .invoke_handler(tauri::generate_handler![
            commands::chat::chat,
            commands::settings::get_settings,
            commands::settings::save_settings,
            commands::settings::get_soul,
            commands::settings::save_soul,
            commands::window::open_panel,
            commands::window::open_debug,
            commands::debug::get_logs,
            commands::debug::append_log,
            commands::debug::clear_logs,
            commands::shell::execute_shell,
            commands::shell::check_shell_status,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
