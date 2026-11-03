use std::collections::BTreeMap;

use super::agent_tools::SpawnSubagentTool;
use super::chat_tool::ChatTool;
use super::context::ToolContext;
use super::file_tools::{EditFileTool, ReadFileTool, WriteFileTool};
use super::read_image_tool::ReadImageTool;
use super::screenshot_tool::ScreenshotTool;
use super::shell_tools::{BashTool, CheckShellStatusTool, WriteStdinTool};
use super::swarm_tools::{
    SwarmCompleteTaskTool, SwarmCreateTaskTool, SwarmDeleteTaskTool, SwarmGetSummaryTool,
    SwarmUpdateConfigTool, SwarmUpdateTaskTool,
};
use super::tool::Tool;
use super::web_search_tool::WebSearchTool;
use crate::mcp::{McpStore, McpToolDefinition};

/// What kind of run is asking for tools. Replaces the old pile of booleans
/// (`include_chat`/`include_web_search`/`include_swarm`/`include_swarm_worker`):
/// adding a gated tool now means matching on this, not threading another flag
/// through every caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunKind {
    /// A normal panel chat turn, or its CLI equivalent.
    Interactive,
    /// A scheduled heartbeat — the only run that may message the owner directly.
    Heartbeat,
    /// A group-chat agent.
    Group,
    /// The swarm Orchestrator.
    SwarmMain,
    /// A swarm worker running one task.
    SwarmWorker,
}

/// Which side of a swarm a run sits on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwarmRole {
    Main,
    Worker,
}

/// The run context a tool's availability is decided from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunGate {
    /// Sub-agent nesting level (0 = the pet itself).
    pub depth: usize,
    pub kind: RunKind,
    /// Whether a Tavily key is configured (`web_search` cannot work without one).
    pub has_search_key: bool,
    /// Set for swarm runs; carried alongside `kind` so availability rules can
    /// ask "is this a swarm at all" without matching every kind.
    pub swarm: Option<SwarmRole>,
}

impl Default for RunGate {
    fn default() -> Self {
        Self::for_kind(RunKind::Interactive, 0, false)
    }
}

impl RunGate {
    pub fn for_kind(kind: RunKind, depth: usize, has_search_key: bool) -> Self {
        let swarm = match kind {
            RunKind::SwarmMain => Some(SwarmRole::Main),
            RunKind::SwarmWorker => Some(SwarmRole::Worker),
            RunKind::Interactive | RunKind::Heartbeat | RunKind::Group => None,
        };
        Self {
            depth,
            kind,
            has_search_key,
            swarm,
        }
    }

    /// A normal panel chat (or CLI) run: depth 0, no heartbeat/swarm.
    pub fn interactive(has_search_key: bool) -> Self {
        Self::for_kind(RunKind::Interactive, 0, has_search_key)
    }

    /// The gate a running `ToolContext` opens: its kind and depth, plus whether
    /// this run may use web search.
    pub fn from_context(ctx: &ToolContext, has_search_key: bool) -> Self {
        Self::for_kind(ctx.kind, ctx.depth, has_search_key)
    }
}

/// A built-in tool's display label: which context gate it sits behind, as shown
/// in the settings UI. Availability itself is decided by `ToolPolicy::offered`,
/// so this type carries no behaviour.
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
    /// Only when a Tavily key is configured — the tool cannot work without one.
    WebSearch,
    /// Only for runs attached to a task-oriented swarm.
    Swarm,
    /// Only for the swarm Orchestrator, not its Workers.
    SwarmOrchestrator,
    /// Only for a worker that already owns one task.
    SwarmWorker,
}


/// Which tools a run may offer: the run gate, plus the owner's own
/// configuration (tools switched off, descriptions rewritten).
///
/// The owner's part can only ever *subtract*. Switching `chat` on in config
/// doesn't hand it to a normal session, and `spawn_subagent` stays hidden inside
/// a sub-agent: those gates guard runaway recursion and calls that would always
/// fail, which is not a preference.
#[derive(Debug, Clone, Default)]
pub struct ToolPolicy {
    pub gate: RunGate,
    /// Tool names the owner switched off (`tools.disabled` in config.yaml).
    /// Applies to MCP tools too — they go through the same registry.
    pub disabled: Vec<String>,
    /// Tool name → owner-rewritten `description` (`prompts/tools/<name>.md`).
    pub descriptions: BTreeMap<String, String>,
}

impl ToolPolicy {
    /// The policy one run gets: `gate` decides what its kind may use, and the
    /// owner's configuration (read once here) subtracts from that. This is the
    /// single constructor the agent loop, the settings preview and the CLI use,
    /// so their offer sets cannot drift apart.
    pub fn for_gate(gate: RunGate) -> Self {
        Self {
            gate,
            disabled: crate::settings::get_settings()
                .map(|s| s.tools.disabled)
                .unwrap_or_default(),
            descriptions: crate::prompts::tool_descriptions(),
        }
    }

    /// Whether this run's gate lets a tool behind `scope` through. The owner's
    /// switch is applied separately — it can only subtract.
    fn offered(&self, scope: ToolScope) -> bool {
        match scope {
            ToolScope::Always => true,
            ToolScope::TopLevel => self.gate.depth == 0,
            ToolScope::Heartbeat => self.gate.kind == RunKind::Heartbeat,
            ToolScope::WebSearch => self.gate.has_search_key,
            ToolScope::Swarm => self.gate.swarm.is_some(),
            ToolScope::SwarmOrchestrator => self.gate.swarm == Some(SwarmRole::Main),
            ToolScope::SwarmWorker => self.gate.swarm == Some(SwarmRole::Worker),
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
        (Box::new(ReadImageTool), ToolScope::Always),
        (Box::new(ScreenshotTool), ToolScope::Always),
        (Box::new(WebSearchTool), ToolScope::WebSearch),
        (Box::new(SpawnSubagentTool), ToolScope::TopLevel),
        (Box::new(ChatTool), ToolScope::Heartbeat),
        (Box::new(SwarmUpdateTaskTool), ToolScope::SwarmWorker),
        (Box::new(SwarmCompleteTaskTool), ToolScope::SwarmWorker),
        (Box::new(SwarmCreateTaskTool), ToolScope::Swarm),
        (Box::new(SwarmDeleteTaskTool), ToolScope::SwarmOrchestrator),
        (Box::new(SwarmUpdateConfigTool), ToolScope::SwarmOrchestrator),
        (Box::new(SwarmGetSummaryTool), ToolScope::Swarm),
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
        .map(|(tool, scope)| {
            let spec = tool.spec();
            BuiltinTool {
                name: spec.name.to_string(),
                description: spec.description.to_string(),
                scope,
            }
        })
        .collect()
}

/// One built-in tool as the settings page shows it: the catalog entry plus the
/// owner's layer on top of it (rewritten description, on/off switch).
#[derive(Debug, Clone, serde::Serialize)]
pub struct ToolCatalogEntry {
    pub name: String,
    /// The description in force (the owner's rewrite if there is one).
    pub description: String,
    /// The built-in text, so "restore default" has something to show.
    pub default_description: String,
    pub customized: bool,
    pub enabled: bool,
    /// The context gate this tool sits behind — a switched-on tool is still only
    /// offered to runs that gate allows.
    pub scope: ToolScope,
}

/// The settings-page view of the built-in catalog: every tool, switched-off ones
/// included. The catalog is the whole set; `enabled` and `description` are the
/// owner's layer on top of it.
pub fn catalog_for_ui() -> Result<Vec<ToolCatalogEntry>, String> {
    let settings = crate::settings::get_settings()?;
    let overrides = crate::prompts::tool_descriptions();
    Ok(builtin_catalog()
        .into_iter()
        .map(|tool| {
            let custom = overrides.get(&tool.name);
            ToolCatalogEntry {
                description: custom.unwrap_or(&tool.description).clone(),
                customized: custom.is_some(),
                enabled: !settings.tools.disabled.contains(&tool.name),
                default_description: tool.description,
                name: tool.name,
                scope: tool.scope,
            }
        })
        .collect())
}

/// Where a tool comes from: a built-in Rust impl or one MCP server.
enum ToolSource {
    Builtin(Box<dyn Tool>),
    Mcp { server: String },
}

/// One tool in the registry: its definition (already normalized to the provider
/// envelope) and where a call to it should go.
struct ToolEntry {
    def: serde_json::Value,
    source: ToolSource,
}

/// Registry holding all available tools (built-in + MCP), all in one
/// representation so definitions and dispatch can't disagree.
pub struct ToolRegistry {
    entries: Vec<ToolEntry>,
    mcp: McpStore,
}

impl ToolRegistry {
    /// Create the registry for one run: the built-ins its `policy` allows, plus
    /// the MCP tools of the servers the run's agent references.
    ///
    /// A disabled tool is dropped here, once — which is why it disappears from
    /// both `definitions` (the model is never offered it) and `execute` (a name
    /// the model dug out of an older turn answers "unknown tool" instead of
    /// running).
    pub async fn new(mcp_store: &McpStore, servers: &[String], policy: ToolPolicy) -> Self {
        let mcp_definitions = mcp_store.read().await.definitions(servers);
        Self::build(mcp_store, mcp_definitions, policy)
    }

    fn build(
        mcp_store: &McpStore,
        mcp_definitions: Vec<McpToolDefinition>,
        policy: ToolPolicy,
    ) -> Self {
        let mut entries: Vec<ToolEntry> = Vec::new();
        let mut names: Vec<String> = Vec::new();

        for (tool, scope) in builtins() {
            let spec = tool.spec();
            if !policy.offered(scope) || !policy.allows(spec.name) {
                continue;
            }
            let mut def = tool.definition();
            apply_description(&mut def, spec.name, &policy.descriptions);
            names.push(spec.name.to_string());
            entries.push(ToolEntry {
                def,
                source: ToolSource::Builtin(tool),
            });
        }

        for mcp_def in mcp_definitions {
            let Some(mut def) = normalize_definition(mcp_def.definition) else {
                continue;
            };
            let name = def["function"]["name"].as_str().unwrap_or("").to_string();
            // A built-in wins a name collision, and a server earlier in the
            // agent's list wins over a later one (McpHub::definitions already
            // de-duplicates those). Two identically named functions would make
            // most providers reject the whole request with a 400.
            if name.is_empty() || names.contains(&name) || !policy.allows(&name) {
                continue;
            }
            apply_description(&mut def, &name, &policy.descriptions);
            names.push(name);
            entries.push(ToolEntry {
                def,
                source: ToolSource::Mcp {
                    server: mcp_def.server,
                },
            });
        }

        Self {
            entries,
            mcp: mcp_store.clone(),
        }
    }

    #[cfg(test)]
    pub(crate) fn for_test(tools: Vec<Box<dyn Tool>>) -> Self {
        let entries = tools
            .into_iter()
            .map(|tool| {
                let def = tool.definition();
                ToolEntry {
                    def,
                    source: ToolSource::Builtin(tool),
                }
            })
            .collect();
        Self {
            entries,
            mcp: crate::mcp::new_mcp_store(),
        }
    }

    /// Get all tool definitions for the LLM API request (built-in + MCP), with
    /// any owner-rewritten description swapped in. Only the description is
    /// replaced — the parameter schema is the tool's implementation contract,
    /// not prose.
    pub fn definitions(&self) -> serde_json::Value {
        serde_json::Value::Array(self.entries.iter().map(|e| e.def.clone()).collect())
    }

    /// Whether this tool is served by an MCP server rather than built in.
    /// Used only for display, never for dispatch.
    pub fn is_mcp(&self, name: &str) -> bool {
        self.entries.iter().any(|entry| {
            entry_name(entry) == name && matches!(entry.source, ToolSource::Mcp { .. })
        })
    }

    /// Find and execute a tool by name, routing built-ins to their impl and MCP
    /// tools to the server that advertised them.
    pub async fn execute(&self, name: &str, arguments: &str, ctx: &ToolContext) -> String {
        let Some(entry) = self.entries.iter().find(|entry| entry_name(entry) == name) else {
            return super::tool_error(format!("unknown tool: {}", name));
        };
        match &entry.source {
            ToolSource::Builtin(tool) => {
                ctx.log(&format!("Tool call: {}({})", name, arguments));
                tool.execute(arguments, ctx).await
            }
            ToolSource::Mcp { server } => {
                ctx.log(&format!("MCP tool call: {}({})", name, arguments));
                let arguments = if arguments.trim().is_empty() {
                    serde_json::Value::Null
                } else {
                    match serde_json::from_str(arguments) {
                        Ok(value) => value,
                        Err(e) => {
                            return super::tool_error(format!("invalid arguments: {}", e))
                        }
                    }
                };
                // Read locks let different MCP calls run concurrently while
                // still excluding reconnect/shutdown mutations.
                let hub = self.mcp.read().await;
                match hub.call_tool(std::slice::from_ref(server), name, arguments).await {
                    Ok(result) => result,
                    Err(error) => super::tool_error(error),
                }
            }
        }
    }
}

fn entry_name(entry: &ToolEntry) -> &str {
    entry.def["function"]["name"].as_str().unwrap_or("")
}

fn apply_description(def: &mut serde_json::Value, name: &str, descriptions: &BTreeMap<String, String>) {
    if let Some(text) = descriptions.get(name) {
        def["function"]["description"] = serde_json::Value::String(text.clone());
    }
}

/// Turn any accepted MCP definition shape into the provider envelope, filling
/// in the optional fields. Servers normally send the wrapped shape, but a bare
/// `{"name","description","parameters"}` object is accepted too — the old data
/// path assumed the wrapped shape in three different places.
fn normalize_definition(value: serde_json::Value) -> Option<serde_json::Value> {
    let mut value = value;
    let function = if value.get("function").is_some_and(serde_json::Value::is_object) {
        value.as_object_mut()?.remove("function")?
    } else if value.get("name").is_some() {
        std::mem::take(&mut value)
    } else {
        return None;
    };

    let name = function.get("name")?.as_str()?.to_string();
    if name.is_empty() {
        return None;
    }
    let description = function
        .get("description")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    let parameters = function
        .get("parameters")
        .filter(|parameters| parameters.is_object())
        .cloned()
        .unwrap_or_else(|| serde_json::json!({"type": "object", "properties": {}}));

    Some(serde_json::json!({
        "type": "function",
        "function": {
            "name": name,
            "description": description,
            "parameters": parameters,
        }
    }))
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

    /// A policy with no owner configuration — the run gate only.
    fn policy(kind: RunKind, depth: usize, search: bool) -> ToolPolicy {
        ToolPolicy {
            gate: RunGate::for_kind(kind, depth, search),
            ..ToolPolicy::default()
        }
    }

    fn registry(policy: ToolPolicy) -> ToolRegistry {
        build_registry(vec![], policy)
    }

    fn build_registry(
        mcp_definitions: Vec<McpToolDefinition>,
        policy: ToolPolicy,
    ) -> ToolRegistry {
        ToolRegistry::build(&crate::mcp::new_mcp_store(), mcp_definitions, policy)
    }

    /// The minimum context a tool call needs. Nothing here reaches the network:
    /// the call under test is rejected before any tool runs.
    fn test_ctx() -> ToolContext {
        use std::sync::Arc;
        ToolContext::for_session(
            super::super::context::ToolStores::new(
                crate::logging::LogStore(Arc::new(std::sync::Mutex::new(Vec::new()))),
                crate::shell::ShellStore::new(),
                crate::mcp::new_mcp_store(),
            ),
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
            "test-session",
        )
    }

    fn mcp_def(server: &str, name: &str) -> McpToolDefinition {
        McpToolDefinition {
            server: server.to_string(),
            definition: serde_json::json!({
                "type": "function",
                "function": { "name": name, "description": "mcp tool", "parameters": {} }
            }),
        }
    }

    #[test]
    fn spawn_subagent_offered_only_at_top_level() {
        // Depth 0 (the pet itself) can delegate; deeper sub-agents cannot, which
        // is what prevents runaway recursive spawning.
        assert!(tool_names(&registry(policy(RunKind::Interactive, 0, false)))
            .contains(&"spawn_subagent".to_string()));
        assert!(!tool_names(&registry(policy(RunKind::Interactive, 1, false)))
            .contains(&"spawn_subagent".to_string()));
    }

    #[test]
    fn screenshot_tool_always_offered() {
        // The pet can look at the user's screen at any depth and in any session.
        assert!(tool_names(&registry(policy(RunKind::Interactive, 0, false)))
            .contains(&"screenshot".to_string()));
        assert!(tool_names(&registry(policy(RunKind::Interactive, 1, false)))
            .contains(&"screenshot".to_string()));
    }

    #[test]
    fn web_search_offered_only_when_key_configured() {
        // The tool needs a Tavily key to work, so it's withheld without one and
        // offered (at any depth) once a key is present.
        assert!(!tool_names(&registry(policy(RunKind::Interactive, 0, false)))
            .contains(&"web_search".to_string()));
        assert!(tool_names(&registry(policy(RunKind::Interactive, 0, true)))
            .contains(&"web_search".to_string()));
        assert!(tool_names(&registry(policy(RunKind::Interactive, 1, true)))
            .contains(&"web_search".to_string()));
    }

    #[test]
    fn chat_tool_offered_only_to_heartbeats() {
        // Normal sessions can't proactively message the owner; heartbeats can.
        assert!(!tool_names(&registry(policy(RunKind::Interactive, 0, false)))
            .contains(&"chat".to_string()));
        assert!(tool_names(&registry(policy(RunKind::Heartbeat, 0, false)))
            .contains(&"chat".to_string()));
    }

    #[test]
    fn swarm_tools_are_offered_only_to_swarm_runs() {
        let normal = registry(policy(RunKind::Interactive, 0, false));
        assert!(!tool_names(&normal).contains(&"swarm_get_summary".to_string()));
        let swarm = registry(policy(RunKind::SwarmMain, 0, false));
        assert!(tool_names(&swarm).contains(&"swarm_get_summary".to_string()));
        // Deleting tasks is the Orchestrator's call; Workers only run them.
        assert!(tool_names(&swarm).contains(&"swarm_delete_task".to_string()));
        assert!(tool_names(&swarm).contains(&"swarm_update_config".to_string()));
        let worker = registry(policy(RunKind::SwarmWorker, 1, false));
        assert!(!tool_names(&worker).contains(&"swarm_delete_task".to_string()));
        assert!(!tool_names(&worker).contains(&"swarm_update_config".to_string()));
    }

    #[tokio::test]
    async fn a_disabled_tool_is_neither_offered_nor_executable() {
        // Withholding the definition isn't enough: the model can call a name it
        // saw in an earlier turn of the same session.
        let policy = ToolPolicy {
            disabled: vec!["bash".to_string()],
            ..policy(RunKind::Interactive, 0, false)
        };
        let registry = build_registry(vec![mcp_def("s", "mcp_tool")], policy);
        assert!(!tool_names(&registry).contains(&"bash".to_string()));

        let ctx = test_ctx();
        let out = registry
            .execute("bash", r#"{"command":"echo disabled-tool-test"}"#, &ctx)
            .await;
        assert!(out.contains("unknown tool"), "{out}");
    }

    #[test]
    fn a_disabled_mcp_tool_is_dropped_too() {
        // MCP tools go through the same registry, so one switch covers both.
        let policy = ToolPolicy {
            disabled: vec!["mcp_tool".to_string()],
            ..policy(RunKind::Interactive, 0, false)
        };
        let registry = build_registry(
            vec![mcp_def("s", "mcp_tool"), mcp_def("s", "kept")],
            policy,
        );
        let names = tool_names(&registry);
        assert!(!names.contains(&"mcp_tool".to_string()));
        assert!(names.contains(&"kept".to_string()));
    }

    #[test]
    fn the_owner_switch_can_only_subtract() {
        // Enabling a context-gated tool in config must not hand it to a run the
        // gate excludes.
        let names = tool_names(&registry(policy(RunKind::Interactive, 1, false)));
        assert!(!names.contains(&"chat".to_string()));
        assert!(!names.contains(&"spawn_subagent".to_string()));
    }

    #[test]
    fn description_override_replaces_only_the_description() {
        let mut descriptions = BTreeMap::new();
        descriptions.insert("bash".to_string(), "只用来跑 git".to_string());
        descriptions.insert("mcp_tool".to_string(), "改过的 MCP 描述".to_string());
        let policy = ToolPolicy {
            descriptions,
            ..policy(RunKind::Interactive, 0, false)
        };
        let registry = build_registry(vec![mcp_def("s", "mcp_tool")], policy);

        let defs = registry.definitions();
        let defs = defs.as_array().unwrap();
        let find =
            |name: &str| defs.iter().find(|d| d["function"]["name"] == name).unwrap().clone();
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

    #[test]
    fn a_built_in_wins_a_name_collision_with_an_mcp_tool() {
        // Two `bash` functions in one request are rejected by most providers;
        // the built-in is the one that can actually run.
        let registry = build_registry(vec![mcp_def("s", "bash")], policy(RunKind::Interactive, 0, false));
        let names = tool_names(&registry);
        assert_eq!(names.iter().filter(|name| *name == "bash").count(), 1);
        assert!(!registry.is_mcp("bash"));
    }

    #[test]
    fn an_mcp_tool_is_accepted_in_its_bare_shape() {
        let registry = build_registry(
            vec![McpToolDefinition {
                server: "s".to_string(),
                definition: serde_json::json!({
                    "name": "bare_tool",
                    "description": "bare",
                    "parameters": { "type": "object", "properties": {} }
                }),
            }],
            policy(RunKind::Interactive, 0, false),
        );
        let defs = registry.definitions();
        let def = defs
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["function"]["name"] == "bare_tool")
            .unwrap();
        assert_eq!(def["type"], "function");
        assert_eq!(def["function"]["description"], "bare");
        assert!(registry.is_mcp("bare_tool"));
    }
}