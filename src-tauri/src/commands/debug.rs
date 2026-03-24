use std::sync::Mutex;
use tauri::State;

pub struct LogStore(pub Mutex<Vec<String>>);

#[tauri::command]
pub fn get_logs(store: State<'_, LogStore>) -> Vec<String> {
    store.0.lock().unwrap().clone()
}

#[tauri::command]
pub fn append_log(store: State<'_, LogStore>, message: String) {
    let mut logs = store.0.lock().unwrap();
    let ts = chrono::Local::now().format("%H:%M:%S%.3f").to_string();
    logs.push(format!("[{}] {}", ts, message));
    // Keep last 500 entries
    if logs.len() > 500 {
        let drain = logs.len() - 500;
        logs.drain(0..drain);
    }
}

#[tauri::command]
pub fn clear_logs(store: State<'_, LogStore>) {
    store.0.lock().unwrap().clear();
}
