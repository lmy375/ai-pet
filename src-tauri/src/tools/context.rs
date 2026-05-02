use crate::commands::debug::{write_log, CacheCountersStore, LogStore};
use crate::commands::shell::ShellStore;

/// Shared context passed to all tools during execution
pub struct ToolContext {
    pub shell_store: ShellStore,
    pub log_store: LogStore,
    /// Process-wide cumulative cache counters. Populated by ToolRegistry::log_cache_summary
    /// at the end of each LLM turn so the panel UI can render an honest cumulative hit
    /// ratio across the entire pet session, independent of in-memory log truncation.
    pub cache_counters: CacheCountersStore,
}

impl ToolContext {
    pub fn new(
        log_store: LogStore,
        shell_store: ShellStore,
        cache_counters: CacheCountersStore,
    ) -> Self {
        Self {
            shell_store,
            log_store,
            cache_counters,
        }
    }

    pub fn from_states(
        log_store: &tauri::State<'_, LogStore>,
        shell_store: &tauri::State<'_, ShellStore>,
        cache_counters: &tauri::State<'_, CacheCountersStore>,
    ) -> Self {
        Self {
            shell_store: ShellStore(shell_store.0.clone()),
            log_store: LogStore(log_store.0.clone()),
            cache_counters: cache_counters.inner().clone(),
        }
    }

    /// Constructor for unit tests that don't go through Tauri State. Builds fresh empty
    /// stores so each test gets isolated counters.
    #[cfg(test)]
    pub fn for_test(log_store: LogStore, shell_store: ShellStore) -> Self {
        Self::new(
            log_store,
            shell_store,
            crate::commands::debug::new_cache_counters(),
        )
    }

    pub fn log(&self, msg: &str) {
        write_log(&self.log_store.0, msg);
    }
}
