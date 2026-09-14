use std::collections::BTreeMap;

use super::agent_tools::SpawnSubagentTool;
use super::chat_tool::ChatTool;
use super::context::ToolContext;
use super::file_tools::{EditFileTool, ReadFileTool, WriteFileTool};
use super::group_tool::GroupChatTool;
use super::screenshot_tool::ScreenshotTool;
use super::shell_tools::{BashTool, CheckShellStatusTool, WriteStdinTool};
use super::tool::Tool;
use super::web_search_tool::WebSearchTool;

/// The context gate a built-in tool sits behind — one table instead of a chain
/// of `if`s, so "which tools does this kind of run get" is readable in one place
/// and can also be shown in the settings UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolScope {
    /// Offered to every run.
    Always,
    /// Only at depth 0 — a sub-agent that could spawn sub-agents would recurse.
    TopLevel,
    /// Only to scheduled heartbeat runs, which have no UI stream and otherwise
    /// no way to reach the owner.
    Heartbeat,
    /// Only to group-page agent runs, the one place an agent speaks to a room.
    Group,
    /// Only when a Tavily key is configured — the tool cannot work without one.
    WebSearch,
}

impl ToolScope {
    fn offered(self, policy: &ToolPolicy) -> bool {
        match self {
            ToolScope::Always => true,
            ToolScope::TopLevel => policy.depth == 0,
            ToolScope::Heartbeat => policy.include_chat,
            ToolScope::Group => policy.include_group,
            ToolScope::WebSearch => policy.include_web_search,
        }
    }
}

/// Which tools a run may offer: the context gates above, plus the owner's own
/// configuration (tools switched off, descriptions rewritten).
///
/// The owner's part can only ever *subtract*. Switching `chat` on in config
/// doesn't hand it to a normal session, and `spawn_subagent` stays hidden inside
/// a sub-agent: those gates guard runaway recursion and calls that would always
/// fail, which is not a preference.
#[derive(Debug, Clone, Default)]
pub struct ToolPolicy {
    /// Sub-agent nesting level (0 = the pet itself).
    pub depth: usize,
    pub include_chat: bool,
    pub include_web_search: bool,
    pub include_group: bool,
    /// Tool names the owner switched off (`tools.disabled` in config.yaml).
    /// Applies to MCP tools too — they go through the same registry.
    pub disabled: Vec<String>,
    /// Tool name → owner-rewritten `description` (`prompts/tools/<name>.md`).
    pub descriptions: BTreeMap<String, String>,
}

impl ToolPolicy {
    /// The owner-configured half, read from disk: the disabled list from
    /// settings and the description overrides from `prompts/tools/`. Callers
    /// fill in the context gates (`depth`, `include_*`) with struct update
    /// syntax.
    pub fn from_config() -> Self {
        Self {
            disabled: crate::settings::get_settings()
                .map(|s| s.tools.disabled)
                .unwrap_or_default(),
            descriptions: crate::prompts::tool_descriptions(),
            ..Self::default()
        }
    }

    fn allows(&self, name: &str) -> bool {
        !self.disabled.iter().any(|d| d == name)
    }
}

/// Every built-in tool, with the gate it sits behind. The single source of
/// truth for what "built-in tools" means — the registry filters this list, and
/// the settings UI lists it.
fn builtins() -> Vec<(Box<dyn Tool>, ToolScope)> {
    vec![
        (Box::new(BashTool), ToolScope::Always),
        (Box::new(CheckShellStatusTool), ToolScope::Always),
        (Box::new(WriteStdinTool), ToolScope::Always),
        (Box::new(ReadFileTool), ToolScope::Always),
        (Box::new(WriteFileTool), ToolScope::Always),
        (Box::new(EditFileTool), ToolScope::Always),
        (Box::new(ScreenshotTool), ToolScope::Always),
        (Box::new(WebSearchTool), ToolScope::WebSearch),
        (Box::new(SpawnSubagentTool), ToolScope::TopLevel),
        (Box::new(ChatTool), ToolScope::Heartbeat),
        (Box::new(GroupChatTool), ToolScope::Group),
    ]
}

/// One built-in tool as the settings UI sees it: its name, the description that
/// would be sent today, and the gate it sits behind.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BuiltinTool {
    pub name: String,
    pub description: String,
    pub scope: ToolScope,
}

/// Every built-in tool with its *default* description — including the ones a
/// given run wouldn't be offered. This is the catalog the settings page lists;
/// it deliberately ignores both the context gates and the owner's overrides, so
/// "restore default" has something to restore to.
pub fn builtin_catalog() -> Vec<BuiltinTool> {
    builtins()
        .into_iter()
        .map(|(tool, scope)| BuiltinTool {
            name: tool.name().to_string(),
            description: tool.definition()["function"]["description"]
                .as_str()
                .unwrap_or("")
                .to_string(),
            scope,
        })
        .collect()
}

/// Registry holding all available tools (built-in + MCP)
pub struct ToolRegistry {
    tools: Vec<Box<dyn Tool>>,
    /// MCP tool definitions in OpenAI function calling format
    mcp_definitions: Vec<serde_json::Value>,
    /// Names of MCP tools (for checking if a tool is MCP-managed)
    mcp_tool_names: Vec<String>,
    /// Owner-rewritten descriptions, applied when the definitions are handed out.
    descriptions: BTreeMap<String, String>,
}

impl ToolRegistry {
    /// Create the registry for one run: the built-ins its `policy` allows, plus
    /// the MCP tools of the servers its agent references.
    ///
    /// A disabled tool is dropped here, once — which is why it disappears from
    /// both `definitions` (the model is never offered it) and `execute` (a name
    /// the model dug out of an older turn answers "unknown tool" instead of
    /// running).
    pub fn new(mcp_definitions: Vec<serde_json::Value>, policy: ToolPolicy) -> Self {
        let tools: Vec<Box<dyn Tool>> = builtins()
            .into_iter()
            .filter(|(tool, scope)| scope.offered(&policy) && policy.allows(tool.name()))
            .map(|(tool, _)| tool)
            .collect();
        let mcp_definitions: Vec<serde_json::Value> = mcp_definitions
            .into_iter()
            .filter(|d| policy.allows(d["function"]["name"].as_str().unwrap_or("")))
            .collect();
        let mcp_tool_names: Vec<String> = mcp_definitions
            .iter()
            .filter_map(|d| d["function"]["name"].as_str().map(String::from))
            .collect();
        Self {
            tools,
            mcp_definitions,
            mcp_tool_names,
            descriptions: policy.descriptions,
        }
    }

    /// Get all tool definitions for the LLM API request (built-in + MCP), with
    /// any owner-rewritten description swapped in. Only the description is
    /// replaced — the parameter schema is the tool's implementation contract,
    /// not prose.
    pub fn definitions(&self) -> serde_json::Value {
        let mut defs: Vec<serde_json::Value> = self.tools.iter().map(|t| t.definition()).collect();
        defs.extend(self.mcp_definitions.iter().cloned());
        for def in &mut defs {
            let Some(name) = def["function"]["name"].as_str() else { continue };
            if let Some(text) = self.descriptions.get(name) {
                def["function"]["description"] = serde_json::Value::String(text.clone());
            }
        }
        serde_json::Value::Array(defs)
    }

    /// Check if a tool name belongs to an MCP server
    pub fn is_mcp_tool(&self, name: &str) -> bool {
        self.mcp_tool_names.iter().any(|n| n == name)
    }

    /// Find and execute a built-in tool by name
    pub async fn execute(&self, name: &str, arguments: &str, ctx: &ToolContext) -> String {
        ctx.log(&format!("Tool call: {}({})", name, arguments));
        for tool in &self.tools {
            if tool.name() == name {
                return tool.execute(arguments, ctx).await;
            }
        }
        super::tool_error(format!("unknown tool: {}", name))
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

    /// A policy with no owner configuration — the context gates only.
    fn policy(depth: usize, chat: bool, web_search: bool, group: bool) -> ToolPolicy {
        ToolPolicy {
            depth,
            include_chat: chat,
            include_web_search: web_search,
            include_group: group,
            ..ToolPolicy::default()
        }
    }

    fn registry(policy: ToolPolicy) -> ToolRegistry {
        ToolRegistry::new(vec![], policy)
    }

    /// The minimum context a tool call needs. Nothing here reaches the network:
    /// the call under test is rejected before any tool runs.
    fn test_ctx() -> ToolContext {
        use std::sync::{Arc, Mutex};
        ToolContext::new(
            crate::logging::LogStore(Arc::new(Mutex::new(Vec::new()))),
            crate::shell::ShellStore(Arc::new(Mutex::new(std::collections::HashMap::new()))),
            crate::config::AiConfig {
                agent_id: "test".to_string(),
                api_key: String::new(),
                base_url: String::new(),
                model: String::new(),
                provider: String::new(),
                context_window: 0,
                search_api_key: String::new(),
                mcp_servers: Vec::new(),
                reasoning: String::new(),
            },
            crate::mcp::new_mcp_store(),
            "test-session".to_string(),
            None,
            None,
            false,
        )
    }

    fn mcp_def(name: &str) -> serde_json::Value {
        serde_json::json!({
            "type": "function",
            "function": { "name": name, "description": "mcp tool", "parameters": {} }
        })
    }

    #[test]
    fn spawn_subagent_offered_only_at_top_level() {
        // Depth 0 (the pet itself) can delegate; deeper sub-agents cannot, which
        // is what prevents runaway recursive spawning.
        assert!(tool_names(&registry(policy(0, false, false, false))).contains(&"spawn_subagent".to_string()));
        assert!(!tool_names(&registry(policy(1, false, false, false))).contains(&"spawn_subagent".to_string()));
    }

    #[test]
    fn screenshot_tool_always_offered() {
        // The pet can look at the user's screen at any depth and in any session.
        assert!(tool_names(&registry(policy(0, false, false, false))).contains(&"screenshot".to_string()));
        assert!(tool_names(&registry(policy(1, false, false, false))).contains(&"screenshot".to_string()));
    }

    #[test]
    fn web_search_offered_only_when_key_configured() {
        // The tool needs a Tavily key to work, so it's withheld without one and
        // offered (at any depth) once a key is present.
        assert!(!tool_names(&registry(policy(0, false, false, false))).contains(&"web_search".to_string()));
        assert!(tool_names(&registry(policy(0, false, true, false))).contains(&"web_search".to_string()));
        assert!(tool_names(&registry(policy(1, false, true, false))).contains(&"web_search".to_string()));
    }

    #[test]
    fn chat_tool_offered_only_to_heartbeats() {
        // Normal sessions can't proactively message the owner; heartbeats can.
        assert!(!tool_names(&registry(policy(0, false, false, false))).contains(&"chat".to_string()));
        assert!(tool_names(&registry(policy(0, true, false, false))).contains(&"chat".to_string()));
    }

    #[test]
    fn group_chat_tool_offered_only_in_group_runs() {
        // Only group-page agent runs can post into the shared room; nothing else
        // gets the GroupChat tool.
        assert!(!tool_names(&registry(policy(0, false, false, false))).contains(&"GroupChat".to_string()));
        assert!(tool_names(&registry(policy(0, false, false, true))).contains(&"GroupChat".to_string()));
    }

    #[tokio::test]
    async fn a_disabled_tool_is_neither_offered_nor_executable() {
        // Withholding the definition isn't enough: the model can call a name it
        // saw in an earlier turn of the same session.
        let policy = ToolPolicy { disabled: vec!["bash".to_string()], ..policy(0, false, false, false) };
        let registry = ToolRegistry::new(vec![mcp_def("mcp_tool")], policy);
        assert!(!tool_names(&registry).contains(&"bash".to_string()));

        let ctx = test_ctx();
        let out = registry.execute("bash", r#"{"command":"echo disabled-tool-test"}"#, &ctx).await;
        assert!(out.contains("unknown tool"), "{out}");
    }

    #[test]
    fn a_disabled_mcp_tool_is_dropped_too() {
        // MCP tools go through the same registry, so one switch covers both —
        // and `is_mcp_tool` must stop claiming it, or the chat loop would route
        // the call to the server anyway.
        let policy = ToolPolicy { disabled: vec!["mcp_tool".to_string()], ..policy(0, false, false, false) };
        let registry = ToolRegistry::new(vec![mcp_def("mcp_tool"), mcp_def("kept")], policy);
        let names = tool_names(&registry);
        assert!(!names.contains(&"mcp_tool".to_string()));
        assert!(names.contains(&"kept".to_string()));
        assert!(!registry.is_mcp_tool("mcp_tool"));
    }

    #[test]
    fn the_owner_switch_can_only_subtract() {
        // Enabling a context-gated tool in config must not hand it to a run the
        // gate excludes.
        let policy = ToolPolicy { disabled: vec![], ..policy(1, false, false, false) };
        let names = tool_names(&ToolRegistry::new(vec![], policy));
        assert!(!names.contains(&"chat".to_string()));
        assert!(!names.contains(&"spawn_subagent".to_string()));
    }

    #[test]
    fn description_override_replaces_only_the_description() {
        let mut descriptions = BTreeMap::new();
        descriptions.insert("bash".to_string(), "只用来跑 git".to_string());
        descriptions.insert("mcp_tool".to_string(), "改过的 MCP 描述".to_string());
        let policy = ToolPolicy { descriptions, ..policy(0, false, false, false) };
        let registry = ToolRegistry::new(vec![mcp_def("mcp_tool")], policy);

        let defs = registry.definitions();
        let defs = defs.as_array().unwrap();
        let find = |name: &str| {
            defs.iter().find(|d| d["function"]["name"] == name).unwrap().clone()
        };
        let bash = find("bash");
        assert_eq!(bash["function"]["description"], "只用来跑 git");
        // The parameter schema is the implementation's contract, not prose.
        assert!(bash["function"]["parameters"]["properties"]["command"].is_object());
        assert_eq!(find("mcp_tool")["function"]["description"], "改过的 MCP 描述");
        // A tool with no override keeps its built-in text.
        assert!(find("read_file")["function"]["description"]
            .as_str()
            .unwrap()
            .contains("line numbers"));
    }

    #[test]
    fn the_catalog_lists_every_built_in_with_its_gate() {
        let catalog = builtin_catalog();
        let chat = catalog.iter().find(|t| t.name == "chat").unwrap();
        assert_eq!(chat.scope, ToolScope::Heartbeat);
        // Gated tools are in the catalog even though no single run offers them
        // all — the settings page has to be able to switch them off.
        assert_eq!(catalog.len(), builtins().len());
        assert!(!chat.description.is_empty());
    }
}
