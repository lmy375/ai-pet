use super::shell_tools::{CheckShellStatusTool, ExecuteShellTool};
use super::tool::Tool;
use super::context::ToolContext;

/// Registry holding all available tools
pub struct ToolRegistry {
    tools: Vec<Box<dyn Tool>>,
}

impl ToolRegistry {
    /// Create registry with all built-in tools
    pub fn new() -> Self {
        let tools: Vec<Box<dyn Tool>> = vec![
            Box::new(ExecuteShellTool),
            Box::new(CheckShellStatusTool),
        ];
        Self { tools }
    }

    /// Get all tool definitions for the LLM API request
    pub fn definitions(&self) -> serde_json::Value {
        serde_json::Value::Array(
            self.tools.iter().map(|t| t.definition()).collect(),
        )
    }

    /// Find and execute a tool by name
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
