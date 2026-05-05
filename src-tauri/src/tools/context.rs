use std::sync::{Arc, Mutex};

use crate::commands::debug::{write_log, LogStore, ProcessCountersStore};
use crate::commands::shell::ShellStore;
use crate::tool_review::ToolReviewRegistryStore;

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
    /// 调试器收集器：每次 tool 执行结果计算完后追加一条
    /// `(name, arguments, result)` 完整记录，给 proactive 调试器在 modal 里
    /// 把 in/out 链路一并展示。与 `tools_used` 完全对称（一个收 names，一个
    /// 收 full records）。`None` 时 chat pipeline 完全跳过，零开销。
    pub tool_calls: Option<Arc<Mutex<Vec<crate::proactive::ToolCallEntry>>>>,
    /// Iter TR3: optional registry for human-review of high-risk tool calls.
    /// `Some` for the desktop chat path (where the panel can render the modal);
    /// `None` for telegram / consolidate / autonomous flows that have no UX
    /// surface — those paths skip review and execute high-risk tools directly.
    pub tool_review: Option<ToolReviewRegistryStore>,
    /// Iter R2: optional decision-log handle so tool-review outcomes (approve /
    /// deny / timeout) land alongside the proactive Spoke / Silent / Skip
    /// entries the panel already shows. None for paths that don't surface to
    /// the panel.
    pub decision_log: Option<crate::decision_log::DecisionLogStore>,
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
            tool_calls: None,
            tool_review: None,
            decision_log: None,
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
            tool_calls: None,
            tool_review: None,
            decision_log: None,
        }
    }

    /// Builder method — attach a tool-review registry (Iter TR3). The desktop
    /// `chat` Tauri command wires this so high-risk tool calls can park here
    /// for user approve/deny before execution.
    pub fn with_tool_review(mut self, registry: ToolReviewRegistryStore) -> Self {
        self.tool_review = Some(registry);
        self
    }

    /// Iter R2: attach a decision-log handle so the chat pipeline can push
    /// tool-review outcomes onto the same log the proactive loop uses.
    pub fn with_decision_log(mut self, log: crate::decision_log::DecisionLogStore) -> Self {
        self.decision_log = Some(log);
        self
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

    /// Builder method — attach a `tool_calls` collector. Caller keeps a clone of
    /// the Arc 以便管线跑完后读出完整记录（name/args/result）。pure parallel
    /// 与 `with_tools_used_collector`，只对开了调试器的调用方（proactive
    /// 路径）有意义；其它路径不调即零开销。
    pub fn with_tool_calls_collector(
        mut self,
        collector: Arc<Mutex<Vec<crate::proactive::ToolCallEntry>>>,
    ) -> Self {
        self.tool_calls = Some(collector);
        self
    }

    pub fn log(&self, msg: &str) {
        write_log(&self.log_store.0, msg);
    }
}
