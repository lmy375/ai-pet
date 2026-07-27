use super::ToolContext;
use std::fmt::Display;

/// Parse a tool's JSON arguments string, falling back to `Null` on bad input.
pub fn parse_args(arguments: &str) -> serde_json::Value {
    serde_json::from_str(arguments).unwrap_or_default()
}

/// The standard `{"error": "<msg>"}` string a tool returns on failure. Routed
/// through serde so quotes/newlines in `msg` are escaped properly — a
/// hand-rolled `format!(r#"{{"error":"{}"}}"#, e)` produces invalid JSON when
/// `e` contains a `"`.
pub fn tool_error(msg: impl Display) -> String {
    serde_json::json!({ "error": msg.to_string() }).to_string()
}

/// Pull a required, non-empty string field out of parsed args. On the `Err`
/// path returns a ready-to-send `{"error": "missing '<field>' parameter"}`, so
/// callers can `match`/`?` straight into a tool result.
pub fn required_str(args: &serde_json::Value, field: &str) -> Result<String, String> {
    match args[field].as_str() {
        Some(s) if !s.is_empty() => Ok(s.to_string()),
        _ => Err(tool_error(format!("missing '{}' parameter", field))),
    }
}

/// Implement `Tool::execute` by delegating to an async `*_impl(arguments, ctx)`
/// function. Every tool's `execute` is the same `Box::pin` trampoline; this
/// keeps each tool to just `name`, `definition`, and its async impl.
#[macro_export]
macro_rules! impl_execute {
    ($impl_fn:ident) => {
        fn execute<'a>(
            &'a self,
            arguments: &'a str,
            ctx: &'a $crate::tools::ToolContext,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = String> + Send + 'a>> {
            Box::pin($impl_fn(arguments, ctx))
        }
    };
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
