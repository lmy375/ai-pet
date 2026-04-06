use super::file_tools::{EditFileTool, ReadFileTool, WriteFileTool};
use super::memory_tools::{MemoryEditTool, MemoryListTool, MemorySearchTool};
use super::shell_tools::{BashTool, CheckShellStatusTool};
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
        ];
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
