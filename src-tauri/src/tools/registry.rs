use std::collections::HashMap;

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
        }
    }

    /// Get all tool definitions for the LLM API request (built-in + MCP)
    pub fn definitions(&self) -> serde_json::Value {
        let mut defs: Vec<serde_json::Value> =
            self.tools.iter().map(|t| t.definition()).collect();
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
        let cache_key = if CACHEABLE_TOOLS.contains(&name) {
            Some(format!("{}|{}", name, arguments))
        } else {
            None
        };
        if let Some(ref key) = cache_key {
            let cache = self.cache.lock().await;
            if let Some(hit) = cache.get(key) {
                ctx.log(&format!("Tool cache hit: {}", name));
                return hit.clone();
            }
        }
        for tool in &self.tools {
            if tool.name() == name {
                let result = tool.execute(arguments, ctx).await;
                if let Some(key) = cache_key {
                    self.cache.lock().await.insert(key, result.clone());
                }
                return result;
            }
        }
        format!(r#"{{"error": "unknown tool: {}"}}"#, name)
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
        ToolContext::new(
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
        assert_eq!(calls.load(Ordering::SeqCst), 1, "underlying tool fired once");
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

        reg.execute("get_weather", r#"{"city":"Beijing"}"#, &ctx).await;
        reg.execute("get_weather", r#"{"city":"Tokyo"}"#, &ctx).await;
        assert_eq!(calls.load(Ordering::SeqCst), 2, "different args = different cache keys");
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
        assert_eq!(calls.load(Ordering::SeqCst), 3, "mutating tool must not be cached");
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
