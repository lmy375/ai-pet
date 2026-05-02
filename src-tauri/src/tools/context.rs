use crate::commands::debug::{write_log, LogStore, ProcessCountersStore};
use crate::commands::shell::ShellStore;

/// Shared context passed to all tools during execution
pub struct ToolContext {
    pub shell_store: ShellStore,
    pub log_store: LogStore,
    /// Bundle of process-wide counter groups (cache hit ratio, mood-tag adherence...).
    /// Adding a new metric is now one new field on `ProcessCounters` plus one Tauri
    /// command — no changes to ToolContext signatures or the 5+ callers.
    pub process_counters: ProcessCountersStore,
}

impl ToolContext {
    pub fn new(
        log_store: LogStore,
        shell_store: ShellStore,
        process_counters: ProcessCountersStore,
    ) -> Self {
        Self {
            shell_store,
            log_store,
            process_counters,
        }
    }

    pub fn from_states(
        log_store: &tauri::State<'_, LogStore>,
        shell_store: &tauri::State<'_, ShellStore>,
        process_counters: &tauri::State<'_, ProcessCountersStore>,
    ) -> Self {
        Self {
            shell_store: ShellStore(shell_store.0.clone()),
            log_store: LogStore(log_store.0.clone()),
            process_counters: process_counters.inner().clone(),
        }
    }

    /// Constructor for unit tests that don't go through Tauri State. Builds fresh empty
    /// counters so each test gets isolated state.
    #[cfg(test)]
    pub fn for_test(log_store: LogStore, shell_store: ShellStore) -> Self {
        Self::new(
            log_store,
            shell_store,
            crate::commands::debug::new_process_counters(),
        )
    }

    pub fn log(&self, msg: &str) {
        write_log(&self.log_store.0, msg);
    }
}
