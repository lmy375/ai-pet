mod commands;
mod config;

use commands::debug::LogStore;
use std::sync::Mutex;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(LogStore(Mutex::new(Vec::new())))
        .invoke_handler(tauri::generate_handler![
            commands::chat::chat,
            commands::settings::get_settings,
            commands::settings::save_settings,
            commands::settings::get_soul,
            commands::settings::save_soul,
            commands::window::open_panel,
            commands::debug::get_logs,
            commands::debug::append_log,
            commands::debug::clear_logs,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
