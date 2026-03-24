use crate::commands::debug::LogStore;
use crate::commands::shell::ShellStore;

/// Shared context passed to all tools during execution
pub struct ToolContext {
    pub shell_store: ShellStore,
    pub log_store: LogStore,
}

impl ToolContext {
    pub fn from_states(
        log_store: &tauri::State<'_, LogStore>,
        shell_store: &tauri::State<'_, ShellStore>,
    ) -> Self {
        Self {
            shell_store: ShellStore(shell_store.0.clone()),
            log_store: LogStore(log_store.0.clone()),
        }
    }

    pub fn log(&self, msg: &str) {
        let mut logs = self.log_store.0.lock().unwrap();
        let ts = chrono::Local::now().format("%H:%M:%S%.3f").to_string();
        logs.push(format!("[{}] {}", ts, msg));
        if logs.len() > 500 {
            let drain = logs.len() - 500;
            logs.drain(0..drain);
        }
    }
}
