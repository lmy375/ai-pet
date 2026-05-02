use crate::commands::debug::{
    write_log, CacheCountersStore, LogStore, MoodTagCountersStore,
};
use crate::commands::shell::ShellStore;

/// Shared context passed to all tools during execution
pub struct ToolContext {
    pub shell_store: ShellStore,
    pub log_store: LogStore,
    /// Process-wide cumulative cache counters. Populated by ToolRegistry::log_cache_summary
    /// at the end of each LLM turn so the panel UI can render an honest cumulative hit
    /// ratio across the entire pet session, independent of in-memory log truncation.
    pub cache_counters: CacheCountersStore,
    /// Process-wide counters tracking how often the LLM included the `[motion: X]` prefix
    /// in its mood updates. Bumped from `mood::read_mood_for_event` so all four LLM entry
    /// points contribute to the same compliance ratio.
    pub mood_tag_counters: MoodTagCountersStore,
}

impl ToolContext {
    pub fn new(
        log_store: LogStore,
        shell_store: ShellStore,
        cache_counters: CacheCountersStore,
        mood_tag_counters: MoodTagCountersStore,
    ) -> Self {
        Self {
            shell_store,
            log_store,
            cache_counters,
            mood_tag_counters,
        }
    }

    pub fn from_states(
        log_store: &tauri::State<'_, LogStore>,
        shell_store: &tauri::State<'_, ShellStore>,
        cache_counters: &tauri::State<'_, CacheCountersStore>,
        mood_tag_counters: &tauri::State<'_, MoodTagCountersStore>,
    ) -> Self {
        Self {
            shell_store: ShellStore(shell_store.0.clone()),
            log_store: LogStore(log_store.0.clone()),
            cache_counters: cache_counters.inner().clone(),
            mood_tag_counters: mood_tag_counters.inner().clone(),
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
            crate::commands::debug::new_mood_tag_counters(),
        )
    }

    pub fn log(&self, msg: &str) {
        write_log(&self.log_store.0, msg);
    }
}
