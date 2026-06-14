use super::ToolContext;

/// Parse a tool's JSON arguments string, falling back to `Null` on bad input.
pub fn parse_args(arguments: &str) -> serde_json::Value {
    serde_json::from_str(arguments).unwrap_or_default()
}

/// Trait that every tool must implement
pub trait Tool: Send + Sync {
    /// Tool name (matches function.name in the API)
    fn name(&self) -> &str;

    /// OpenAI function calling definition (the object inside "tools" array)
    fn definition(&self) -> serde_json::Value;

    /// Execute the tool with given JSON arguments string, return result as string
    fn execute<'a>(
        &'a self,
        arguments: &'a str,
        ctx: &'a ToolContext,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = String> + Send + 'a>>;
}
