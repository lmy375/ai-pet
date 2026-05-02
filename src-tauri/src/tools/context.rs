use std::sync::{Arc, Mutex};

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
    /// Optional sink for "which tool names did the LLM end up calling this turn?".
    /// `run_chat_pipeline` pushes the registry's `called_tool_names` here at the end so
    /// callers like `run_proactive_turn` can tag the decision log without changing the
    /// pipeline's `Result<String, _>` return type. Stays `None` for callers that don't
    /// care (consolidate, telegram, generic chat command).
    pub tools_used: Option<Arc<Mutex<Vec<String>>>>,
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
            tools_used: None,
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
            tools_used: None,
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

    /// Builder method — attach a `tools_used` collector. Caller keeps a clone of the Arc
    /// so it can read the populated names after the pipeline returns.
    pub fn with_tools_used_collector(mut self, collector: Arc<Mutex<Vec<String>>>) -> Self {
        self.tools_used = Some(collector);
        self
    }

    pub fn log(&self, msg: &str) {
        write_log(&self.log_store.0, msg);
    }
}
