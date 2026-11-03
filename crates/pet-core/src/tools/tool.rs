use super::ToolContext;
use serde::de::DeserializeOwned;
use std::fmt::Display;

/// Parse a tool's JSON arguments into its argument struct. A malformed call or a
/// field of the wrong type is reported back to the model as an error — never
/// silently replaced by a default, which used to turn `"timeout": "300000"`
/// into 120s and `"progress": "50"` into 0.
pub fn parse_args<T: DeserializeOwned>(arguments: &str) -> Result<T, String> {
    let text = if arguments.trim().is_empty() { "{}" } else { arguments };
    serde_json::from_str(text).map_err(|e| tool_error(format!("invalid arguments: {}", e)))
}

/// The standard `{"error": "<msg>"}` string a tool returns on failure. Routed
/// through serde so quotes/newlines in `msg` are escaped properly — a
/// hand-rolled `format!(r#"{{"error":"{}"}}"#, e)` produces invalid JSON when
/// `e` contains a `"`.
pub fn tool_error(msg: impl Display) -> String {
    serde_json::json!({ "error": msg.to_string() }).to_string()
}

/// Implement `Tool::execute` by delegating to an async `*_impl(arguments, ctx)`
/// function. Every tool's `execute` is the same `Box::pin` trampoline; this
/// keeps each tool to just its spec and its async impl.
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

/// A tool's static description: everything the model is told about it. The
/// provider envelope (`{"type":"function","function":…}`) is assembled once by
/// `Tool::definition`, so a tool can't drift between its displayed name and the
/// name the registry dispatches on.
pub struct ToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub schema: serde_json::Value,
}

/// Trait that every tool must implement
pub trait Tool: Send + Sync {
    /// The tool's name, prose and parameter schema.
    fn spec(&self) -> ToolSpec;

    /// OpenAI function calling definition (the object inside "tools" array).
    fn definition(&self) -> serde_json::Value {
        let spec = self.spec();
        serde_json::json!({
            "type": "function",
            "function": {
                "name": spec.name,
                "description": spec.description,
                "parameters": spec.schema,
            }
        })
    }

    /// Execute the tool with given JSON arguments string, return result as string
    fn execute<'a>(
        &'a self,
        arguments: &'a str,
        ctx: &'a ToolContext,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = String> + Send + 'a>>;
}
#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Deserialize)]
    struct Args {
        count: u64,
    }

    #[test]
    fn a_wrong_type_is_reported_instead_of_defaulted() {
        // Models sometimes send numbers as strings. The old `as_u64().unwrap_or`
        // path quietly fell back to a default; now the call fails loudly.
        let error = parse_args::<Args>(r#"{"count": "50"}"#).err().unwrap();
        assert!(error.contains("invalid arguments"), "{error}");
        assert_eq!(parse_args::<Args>(r#"{"count": 50}"#).unwrap().count, 50);
    }
}
