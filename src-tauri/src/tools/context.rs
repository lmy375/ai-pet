use crate::commands::debug::{write_log, LogStore};
use crate::commands::shell::ShellStore;

/// Shared context passed to all tools during execution
pub struct ToolContext {
    pub shell_store: ShellStore,
    pub log_store: LogStore,
}

impl ToolContext {
    pub fn new(log_store: LogStore, shell_store: ShellStore) -> Self {
        Self { shell_store, log_store }
    }

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
        write_log(&self.log_store.0, msg);
    }
}
