use super::agent_tools::SpawnSubagentTool;
use super::chat_tool::ChatTool;
use super::file_tools::{EditFileTool, ReadFileTool, WriteFileTool};
use super::screenshot_tool::ScreenshotTool;
use super::shell_tools::{BashTool, CheckShellStatusTool};
use super::web_search_tool::WebSearchTool;
use super::tool::Tool;
use super::context::ToolContext;

/// Registry holding all available tools (built-in + MCP)
pub struct ToolRegistry {
    tools: Vec<Box<dyn Tool>>,
    /// MCP tool definitions in OpenAI function calling format
    mcp_definitions: Vec<serde_json::Value>,
    /// Names of MCP tools (for checking if a tool is MCP-managed)
    mcp_tool_names: Vec<String>,
}

impl ToolRegistry {
    /// Create registry with built-in tools and optional MCP tool definitions.
    ///
    /// `depth` is the sub-agent nesting level: `spawn_subagent` is only offered
    /// at depth 0, so a sub-agent can't spawn further sub-agents.
    ///
    /// `include_chat` adds the `chat` tool (proactively message the owner). It's
    /// offered only to scheduled heartbeat sessions, which run with no UI stream
    /// and otherwise have no way to reach the owner.
    ///
    /// `include_web_search` adds the `web_search` tool. It's withheld unless a
    /// Tavily API key is configured (the tool can't work without one), so the
    /// model isn't offered a capability that would always fail.
    pub fn new(
        mcp_definitions: Vec<serde_json::Value>,
        depth: usize,
        include_chat: bool,
        include_web_search: bool,
    ) -> Self {
        let mut tools: Vec<Box<dyn Tool>> = vec![
            Box::new(BashTool),
            Box::new(CheckShellStatusTool),
            Box::new(ReadFileTool),
            Box::new(WriteFileTool),
            Box::new(EditFileTool),
            Box::new(ScreenshotTool),
        ];
        if include_web_search {
            tools.push(Box::new(WebSearchTool));
        }
        if depth == 0 {
            tools.push(Box::new(SpawnSubagentTool));
        }
        if include_chat {
            tools.push(Box::new(ChatTool));
        }
        let mcp_tool_names: Vec<String> = mcp_definitions
            .iter()
            .filter_map(|d| d["function"]["name"].as_str().map(String::from))
            .collect();
        Self { tools, mcp_definitions, mcp_tool_names }
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

    /// Find and execute a built-in tool by name
    pub async fn execute(&self, name: &str, arguments: &str, ctx: &ToolContext) -> String {
        ctx.log(&format!("Tool call: {}({})", name, arguments));
        for tool in &self.tools {
            if tool.name() == name {
                return tool.execute(arguments, ctx).await;
            }
        }
        format!(r#"{{"error": "unknown tool: {}"}}"#, name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool_names(registry: &ToolRegistry) -> Vec<String> {
        registry
            .definitions()
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|d| d["function"]["name"].as_str().map(String::from))
            .collect()
    }

    #[test]
    fn spawn_subagent_offered_only_at_top_level() {
        // Depth 0 (the pet itself) can delegate; deeper sub-agents cannot, which
        // is what prevents runaway recursive spawning.
        assert!(tool_names(&ToolRegistry::new(vec![], 0, false, false)).contains(&"spawn_subagent".to_string()));
        assert!(!tool_names(&ToolRegistry::new(vec![], 1, false, false)).contains(&"spawn_subagent".to_string()));
    }

    #[test]
    fn screenshot_tool_always_offered() {
        // The pet can look at the user's screen at any depth and in any session.
        assert!(tool_names(&ToolRegistry::new(vec![], 0, false, false)).contains(&"screenshot".to_string()));
        assert!(tool_names(&ToolRegistry::new(vec![], 1, false, false)).contains(&"screenshot".to_string()));
    }

    #[test]
    fn web_search_offered_only_when_key_configured() {
        // The tool needs a Tavily key to work, so it's withheld without one and
        // offered (at any depth) once a key is present.
        assert!(!tool_names(&ToolRegistry::new(vec![], 0, false, false)).contains(&"web_search".to_string()));
        assert!(tool_names(&ToolRegistry::new(vec![], 0, false, true)).contains(&"web_search".to_string()));
        assert!(tool_names(&ToolRegistry::new(vec![], 1, false, true)).contains(&"web_search".to_string()));
    }

    #[test]
    fn chat_tool_offered_only_to_heartbeats() {
        // Normal sessions can't proactively message the owner; heartbeats can.
        assert!(!tool_names(&ToolRegistry::new(vec![], 0, false, false)).contains(&"chat".to_string()));
        assert!(tool_names(&ToolRegistry::new(vec![], 0, true, false)).contains(&"chat".to_string()));
    }
}
