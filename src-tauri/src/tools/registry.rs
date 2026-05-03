use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::sync::Mutex as TokioMutex;

use super::calendar_tool::GetUpcomingEventsTool;
use super::context::ToolContext;
use super::file_tools::{EditFileTool, ReadFileTool, WriteFileTool};
use super::memory_tools::{MemoryEditTool, MemoryListTool, MemorySearchTool};
use super::shell_tools::{BashTool, CheckShellStatusTool};
use super::system_tools::GetActiveWindowTool;
use super::tool::Tool;
use super::weather_tool::GetWeatherTool;

/// Tool names whose results are safe to cache within a single registry lifetime
/// (= one LLM turn). Read-only environment-awareness tools — same arguments produce the
/// same result for the duration of a turn, so when the model thinks twice about the same
/// query we save the IO / external API hit. **Never** add mutating tools here
/// (memory_edit, write_file, edit_file, bash...) or MCP tools whose semantics we don't
/// own — their freshness contract is theirs to define.
const CACHEABLE_TOOLS: &[&str] = &["get_active_window", "get_weather", "get_upcoming_events"];

/// Registry holding all available tools (built-in + MCP)
pub struct ToolRegistry {
    tools: Vec<Box<dyn Tool>>,
    /// MCP tool definitions in OpenAI function calling format
    mcp_definitions: Vec<serde_json::Value>,
    /// Names of MCP tools (for checking if a tool is MCP-managed)
    mcp_tool_names: Vec<String>,
    /// Per-turn cache for whitelisted read-only tools. Key = "tool_name|arguments".
    /// Populated lazily on first call, reused on repeats within the same registry's
    /// lifetime — naturally tick-scoped because the registry is rebuilt per LLM turn.
    cache: TokioMutex<HashMap<String, String>>,
    /// Counts of cache hits and misses across this registry's lifetime. Aggregated into a
    /// single summary log line by `log_cache_summary` so debug consumers can see effective
    /// hit ratio without parsing every per-call log.
    cache_hits: AtomicU64,
    cache_misses: AtomicU64,
    /// Names of tools the LLM has invoked at least once during this registry's lifetime.
    /// Surfaced via `called_tool_names` so the proactive dispatch can tag the decision log
    /// "Spoke" entry with which env-awareness tools the model actually used. Order is not
    /// preserved (only presence matters); duplicates are deduped on read.
    called_tools: TokioMutex<Vec<String>>,
}

impl ToolRegistry {
    /// Create registry with built-in tools and optional MCP tool definitions
    pub fn new(mcp_definitions: Vec<serde_json::Value>) -> Self {
        let tools: Vec<Box<dyn Tool>> = vec![
            Box::new(BashTool),
            Box::new(CheckShellStatusTool),
            Box::new(ReadFileTool),
            Box::new(WriteFileTool),
            Box::new(EditFileTool),
            Box::new(MemoryListTool),
            Box::new(MemorySearchTool),
            Box::new(MemoryEditTool),
            Box::new(GetActiveWindowTool),
            Box::new(GetWeatherTool),
            Box::new(GetUpcomingEventsTool),
        ];
        Self::with_tools(tools, mcp_definitions)
    }

    /// Internal constructor that accepts an arbitrary tool list — used by `new` for the
    /// canonical built-ins, and by tests to inject mocks.
    fn with_tools(tools: Vec<Box<dyn Tool>>, mcp_definitions: Vec<serde_json::Value>) -> Self {
        let mcp_tool_names: Vec<String> = mcp_definitions
            .iter()
            .filter_map(|d| d["function"]["name"].as_str().map(String::from))
            .collect();
        Self {
            tools,
            mcp_definitions,
            mcp_tool_names,
            cache: TokioMutex::new(HashMap::new()),
            cache_hits: AtomicU64::new(0),
            cache_misses: AtomicU64::new(0),
            called_tools: TokioMutex::new(Vec::new()),
        }
    }

    /// Get all tool definitions for the LLM API request (built-in + MCP)
    pub fn definitions(&self) -> serde_json::Value {
        let mut defs: Vec<serde_json::Value> = self.tools.iter().map(|t| t.definition()).collect();
        defs.extend(self.mcp_definitions.iter().cloned());
        serde_json::Value::Array(defs)
    }

    /// Check if a tool name belongs to an MCP server
    pub fn is_mcp_tool(&self, name: &str) -> bool {
        self.mcp_tool_names.contains(&name.to_string())
    }

    /// Find and execute a built-in tool by name. Whitelisted read-only tools serve from
    /// the per-registry cache when arguments match a previous call this turn.
    pub async fn execute(&self, name: &str, arguments: &str, ctx: &ToolContext) -> String {
        ctx.log(&format!("Tool call: {}({})", name, arguments));
        // Record the call regardless of cache hit/miss so the decision-log Spoke tag
        // reflects "the model asked about X this turn" — cache hits are still semantic
        // calls from the LLM's perspective even if no IO fired.
        self.called_tools.lock().await.push(name.to_string());
        let cache_key = if CACHEABLE_TOOLS.contains(&name) {
            Some(format!("{}|{}", name, arguments))
        } else {
            None
        };
        if let Some(ref key) = cache_key {
            let cache = self.cache.lock().await;
            if let Some(hit) = cache.get(key) {
                self.cache_hits.fetch_add(1, Ordering::Relaxed);
                ctx.log(&format!("Tool cache hit: {}", name));
                return hit.clone();
            }
        }
        for tool in &self.tools {
            if tool.name() == name {
                let result = tool.execute(arguments, ctx).await;
                if let Some(key) = cache_key {
                    self.cache.lock().await.insert(key, result.clone());
                    self.cache_misses.fetch_add(1, Ordering::Relaxed);
                }
                return result;
            }
        }
        format!(r#"{{"error": "unknown tool: {}"}}"#, name)
    }

    /// Snapshot of cache counters (hits, misses) for this registry's lifetime. Mainly
    /// used by `log_cache_summary` and tests; lock-free read of two atomics.
    pub fn cache_stats(&self) -> (u64, u64) {
        (
            self.cache_hits.load(Ordering::Relaxed),
            self.cache_misses.load(Ordering::Relaxed),
        )
    }

    /// Sorted unique list of tool names the LLM invoked during this registry's lifetime.
    /// Used by the proactive dispatch loop to tag the Spoke decision-log entry; small
    /// enough that sort+dedup on read is fine versus maintaining a HashSet on write.
    pub async fn called_tool_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.called_tools.lock().await.clone();
        names.sort();
        names.dedup();
        names
    }

    /// Emit a single summary log line and bump the process-wide cache counters. Caller
    /// invokes this after the LLM pipeline completes. Suppresses output (and counter
    /// increments) when no cacheable tool fired this turn — no point recording an empty
    /// turn for either humans grepping logs or the ratio renderer.
    pub fn log_cache_summary(&self, ctx: &ToolContext) {
        let (hits, misses) = self.cache_stats();
        let total = hits + misses;
        if total == 0 {
            return;
        }
        let pct = (hits as f64 / total as f64 * 100.0).round() as u64;
        ctx.log(&format!(
            "Tool cache summary: {}/{} hits ({}%)",
            hits, total, pct
        ));
        ctx.process_counters
            .cache
            .turns
            .fetch_add(1, Ordering::Relaxed);
        ctx.process_counters
            .cache
            .hits
            .fetch_add(hits, Ordering::Relaxed);
        ctx.process_counters
            .cache
            .calls
            .fetch_add(total, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::debug::LogStore;
    use crate::commands::shell::ShellStore;
    use std::collections::HashMap as StdHashMap;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Mutex as StdMutex};

    /// Test tool that records every execute() call so we can verify caching behavior.
    struct CountingTool {
        name: String,
        calls: Arc<AtomicU64>,
    }

    impl Tool for CountingTool {
        fn name(&self) -> &str {
            &self.name
        }
        fn definition(&self) -> serde_json::Value {
            serde_json::json!({"type": "function", "function": {"name": self.name}})
        }
        fn execute<'a>(
            &'a self,
            arguments: &'a str,
            _ctx: &'a ToolContext,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = String> + Send + 'a>> {
            let calls = self.calls.clone();
            Box::pin(async move {
                calls.fetch_add(1, Ordering::SeqCst);
                format!(r#"{{"echo": {:?}}}"#, arguments)
            })
        }
    }

    fn fresh_ctx() -> ToolContext {
        ToolContext::for_test(
            LogStore(Arc::new(StdMutex::new(Vec::new()))),
            ShellStore(Arc::new(StdMutex::new(StdHashMap::new()))),
        )
    }

    #[tokio::test]
    async fn cacheable_tool_called_once_for_same_args() {
        let calls = Arc::new(AtomicU64::new(0));
        let tool = Box::new(CountingTool {
            name: "get_weather".to_string(),
            calls: calls.clone(),
        });
        let reg = ToolRegistry::with_tools(vec![tool], vec![]);
        let ctx = fresh_ctx();

        let r1 = reg.execute("get_weather", "{}", &ctx).await;
        let r2 = reg.execute("get_weather", "{}", &ctx).await;
        assert_eq!(r1, r2, "second call should serve cached result verbatim");
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "underlying tool fired once"
        );
    }

    #[tokio::test]
    async fn cacheable_tool_different_args_re_executes() {
        let calls = Arc::new(AtomicU64::new(0));
        let tool = Box::new(CountingTool {
            name: "get_weather".to_string(),
            calls: calls.clone(),
        });
        let reg = ToolRegistry::with_tools(vec![tool], vec![]);
        let ctx = fresh_ctx();

        reg.execute("get_weather", r#"{"city":"Beijing"}"#, &ctx)
            .await;
        reg.execute("get_weather", r#"{"city":"Tokyo"}"#, &ctx)
            .await;
        assert_eq!(
            calls.load(Ordering::SeqCst),
            2,
            "different args = different cache keys"
        );
    }

    #[tokio::test]
    async fn non_cacheable_tool_always_executes() {
        let calls = Arc::new(AtomicU64::new(0));
        // memory_edit is intentionally NOT in CACHEABLE_TOOLS.
        let tool = Box::new(CountingTool {
            name: "memory_edit".to_string(),
            calls: calls.clone(),
        });
        let reg = ToolRegistry::with_tools(vec![tool], vec![]);
        let ctx = fresh_ctx();

        reg.execute("memory_edit", "{}", &ctx).await;
        reg.execute("memory_edit", "{}", &ctx).await;
        reg.execute("memory_edit", "{}", &ctx).await;
        assert_eq!(
            calls.load(Ordering::SeqCst),
            3,
            "mutating tool must not be cached"
        );
    }

    #[tokio::test]
    async fn cache_stats_track_hits_and_misses() {
        let calls = Arc::new(AtomicU64::new(0));
        let tool = Box::new(CountingTool {
            name: "get_weather".to_string(),
            calls: calls.clone(),
        });
        let reg = ToolRegistry::with_tools(vec![tool], vec![]);
        let ctx = fresh_ctx();

        // 1 miss, then 2 hits with same args = (2 hits, 1 miss).
        reg.execute("get_weather", "{}", &ctx).await;
        reg.execute("get_weather", "{}", &ctx).await;
        reg.execute("get_weather", "{}", &ctx).await;
        assert_eq!(reg.cache_stats(), (2, 1));
    }

    #[tokio::test]
    async fn called_tool_names_lists_each_invocation_once() {
        let calls_a = Arc::new(AtomicU64::new(0));
        let calls_b = Arc::new(AtomicU64::new(0));
        let reg = ToolRegistry::with_tools(
            vec![
                Box::new(CountingTool {
                    name: "get_weather".to_string(),
                    calls: calls_a,
                }),
                Box::new(CountingTool {
                    name: "memory_search".to_string(),
                    calls: calls_b,
                }),
            ],
            vec![],
        );
        let ctx = fresh_ctx();

        // Mix of cacheable + non-cacheable, with repeats — sorted dedup result regardless.
        reg.execute("memory_search", r#"{"q":"a"}"#, &ctx).await;
        reg.execute("get_weather", "{}", &ctx).await;
        reg.execute("get_weather", "{}", &ctx).await;
        reg.execute("memory_search", r#"{"q":"b"}"#, &ctx).await;
        let names = reg.called_tool_names().await;
        assert_eq!(
            names,
            vec!["get_weather".to_string(), "memory_search".to_string()]
        );
    }

    #[tokio::test]
    async fn called_tool_names_starts_empty() {
        let reg = ToolRegistry::with_tools(vec![], vec![]);
        assert!(reg.called_tool_names().await.is_empty());
    }

    #[tokio::test]
    async fn cache_stats_ignore_non_cacheable_tools() {
        let calls = Arc::new(AtomicU64::new(0));
        let tool = Box::new(CountingTool {
            name: "memory_edit".to_string(),
            calls: calls.clone(),
        });
        let reg = ToolRegistry::with_tools(vec![tool], vec![]);
        let ctx = fresh_ctx();

        reg.execute("memory_edit", "{}", &ctx).await;
        reg.execute("memory_edit", "{}", &ctx).await;
        // memory_edit is not on the whitelist — neither hit nor miss is recorded.
        assert_eq!(reg.cache_stats(), (0, 0));
    }

    #[tokio::test]
    async fn log_cache_summary_bumps_atomic_counters() {
        let calls = Arc::new(AtomicU64::new(0));
        let tool = Box::new(CountingTool {
            name: "get_weather".to_string(),
            calls: calls.clone(),
        });
        let reg = ToolRegistry::with_tools(vec![tool], vec![]);
        let ctx = fresh_ctx();

        // Same args 3 times → 1 miss + 2 hits.
        reg.execute("get_weather", "{}", &ctx).await;
        reg.execute("get_weather", "{}", &ctx).await;
        reg.execute("get_weather", "{}", &ctx).await;
        reg.log_cache_summary(&ctx);
        assert_eq!(ctx.process_counters.cache.turns.load(Ordering::SeqCst), 1);
        assert_eq!(ctx.process_counters.cache.hits.load(Ordering::SeqCst), 2);
        assert_eq!(ctx.process_counters.cache.calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn log_cache_summary_skips_when_no_cacheable_calls() {
        // No execute() calls at all — summary must not bump counters.
        let reg = ToolRegistry::with_tools(vec![], vec![]);
        let ctx = fresh_ctx();
        reg.log_cache_summary(&ctx);
        assert_eq!(ctx.process_counters.cache.turns.load(Ordering::SeqCst), 0);
        assert_eq!(ctx.process_counters.cache.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn unknown_tool_returns_error_and_does_not_cache() {
        let reg = ToolRegistry::with_tools(vec![], vec![]);
        let ctx = fresh_ctx();
        let out = reg.execute("nonexistent", "{}", &ctx).await;
        assert!(out.contains("unknown tool"));
        // A second call should still error rather than serve a cached "unknown" string.
        let out2 = reg.execute("nonexistent", "{}", &ctx).await;
        assert!(out2.contains("unknown tool"));
    }
}
