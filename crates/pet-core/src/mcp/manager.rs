use crate::settings::McpServerConfig;
use rmcp::model::{CallToolRequestParams, Tool as McpTool};
use rmcp::service::{RoleClient, RunningService};
use rmcp::ServiceExt;
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use tokio::sync::Mutex;

/// One process (or HTTP session) per configured MCP server, shared by every
/// agent that lists it. Servers are defined once globally (`mcp_servers` in
/// config.yaml) and referenced by name from `AgentConfig::mcp`, so two agents
/// using the same server talk to the same connection instead of spawning a
/// child process each.
pub type McpStore = Arc<Mutex<McpHub>>;

pub fn new_mcp_store() -> McpStore {
    Arc::new(Mutex::new(McpHub::new()))
}

#[derive(Debug, Clone, Serialize)]
pub struct McpServerStatus {
    pub name: String,
    pub connected: bool,
    pub tool_count: usize,
    pub tool_names: Vec<String>,
    pub error: Option<String>,
}

/// A live MCP client connection plus the tool set it advertised at handshake.
struct Connection {
    service: RunningService<RoleClient, ()>,
    /// Tool definitions in OpenAI function-calling format.
    definitions: Vec<serde_json::Value>,
    tool_names: Vec<String>,
}

pub struct McpHub {
    connections: HashMap<String, Connection>,
    /// Outcome of the last connection attempt per server name, including
    /// failures (which have no `Connection`), for the settings UI.
    statuses: BTreeMap<String, McpServerStatus>,
}

impl McpHub {
    pub fn new() -> Self {
        Self {
            connections: HashMap::new(),
            statuses: BTreeMap::new(),
        }
    }

    /// Connect every named server that isn't connected yet. Cheap to call
    /// repeatedly — an already-connected server is skipped, which is what makes
    /// the CLI's lazy "connect before this agent's first turn" and the GUI's
    /// "connect everything at startup" the same code path.
    pub async fn ensure(&mut self, servers: &[(&str, &McpServerConfig)]) {
        for (name, config) in servers {
            if self.connections.contains_key(*name) {
                continue;
            }
            self.connect(name, config).await;
        }
    }

    /// Drop every connection and reconnect the given servers — the settings
    /// "reconnect" button, after the server list or a command line changed.
    pub async fn reconnect(&mut self, servers: &[(&str, &McpServerConfig)]) {
        self.shutdown().await;
        self.ensure(servers).await;
    }

    /// Stop one server and forget its status — the settings switch turning a
    /// server off, which takes its tools away from every agent at once.
    pub async fn disconnect(&mut self, name: &str) {
        if let Some(conn) = self.connections.remove(name) {
            eprintln!("Shutting down MCP server: {}", name);
            let _ = conn.service.cancel().await;
        }
        self.statuses.remove(name);
    }

    async fn connect(&mut self, name: &str, config: &McpServerConfig) {
        let status = match Self::connect_server(config).await {
            Ok((service, tools)) => {
                let tool_names: Vec<String> = tools.iter().map(|t| t.name.to_string()).collect();
                let definitions: Vec<serde_json::Value> =
                    tools.iter().map(mcp_tool_to_openai).collect();
                let status = McpServerStatus {
                    name: name.to_string(),
                    connected: true,
                    tool_count: tool_names.len(),
                    tool_names: tool_names.clone(),
                    error: None,
                };
                self.connections.insert(
                    name.to_string(),
                    Connection { service, definitions, tool_names },
                );
                status
            }
            Err(e) => {
                eprintln!("Failed to connect MCP server '{}': {}", name, e);
                McpServerStatus {
                    name: name.to_string(),
                    connected: false,
                    tool_count: 0,
                    tool_names: vec![],
                    error: Some(e),
                }
            }
        };
        self.statuses.insert(name.to_string(), status);
    }

    async fn connect_server(
        config: &McpServerConfig,
    ) -> Result<(RunningService<RoleClient, ()>, Vec<McpTool>), String> {
        match config.transport.as_str() {
            "stdio" => Self::connect_stdio(config).await,
            "sse" | "http" => Self::connect_http(config).await,
            other => Err(format!("Unknown transport type: {}", other)),
        }
    }

    /// Serve the client over `transport`, then list its tools. Shared tail of
    /// every `connect_*` — the transports differ but the handshake is identical.
    async fn serve_and_list<T, E, A>(
        transport: T,
    ) -> Result<(RunningService<RoleClient, ()>, Vec<McpTool>), String>
    where
        T: rmcp::transport::IntoTransport<RoleClient, E, A>,
        E: std::error::Error + Send + Sync + 'static,
    {
        let service: RunningService<RoleClient, ()> = ().serve(transport)
            .await
            .map_err(|e| format!("Failed to initialize MCP client: {}", e))?;

        let tools = service
            .list_all_tools()
            .await
            .map_err(|e| format!("Failed to list tools: {}", e))?;

        Ok((service, tools))
    }

    async fn connect_stdio(
        config: &McpServerConfig,
    ) -> Result<(RunningService<RoleClient, ()>, Vec<McpTool>), String> {
        use rmcp::transport::TokioChildProcess;
        use tokio::process::Command;

        let mut cmd = Command::new(&config.command);
        cmd.args(&config.args);
        for (key, value) in &config.env {
            cmd.env(key, value);
        }

        let transport = TokioChildProcess::new(cmd)
            .map_err(|e| format!("Failed to spawn process: {}", e))?;

        Self::serve_and_list(transport).await
    }

    async fn connect_http(
        config: &McpServerConfig,
    ) -> Result<(RunningService<RoleClient, ()>, Vec<McpTool>), String> {
        use rmcp::transport::streamable_http_client::{
            StreamableHttpClientTransport, StreamableHttpClientTransportConfig,
        };

        let transport = if config.headers.is_empty() {
            StreamableHttpClientTransport::from_uri(config.url.clone())
        } else {
            let mut custom_headers = HashMap::new();
            for (key, value) in &config.headers {
                let header_name = reqwest::header::HeaderName::from_bytes(key.as_bytes())
                    .map_err(|e| format!("Invalid header name '{}': {}", key, e))?;
                let header_value = reqwest::header::HeaderValue::from_str(value)
                    .map_err(|e| format!("Invalid header value for '{}': {}", key, e))?;
                custom_headers.insert(header_name, header_value);
            }
            let mut http_config = StreamableHttpClientTransportConfig::with_uri(config.url.clone());
            http_config.custom_headers = custom_headers;
            StreamableHttpClientTransport::from_config(http_config)
        };

        Self::serve_and_list(transport).await
    }

    /// Tool definitions offered to one agent: the union of the servers it lists,
    /// in that order. A tool name served by two of them resolves to the first —
    /// the same server `call_tool` will route to.
    pub fn definitions(&self, servers: &[String]) -> Vec<serde_json::Value> {
        let mut seen: Vec<&str> = Vec::new();
        let mut defs = Vec::new();
        for name in servers {
            let Some(conn) = self.connections.get(name) else { continue };
            for (def, tool) in conn.definitions.iter().zip(&conn.tool_names) {
                if seen.contains(&tool.as_str()) {
                    continue;
                }
                seen.push(tool);
                defs.push(def.clone());
            }
        }
        defs
    }

    /// Call an MCP tool on behalf of an agent, routed to the first server in
    /// that agent's list which advertises it. A tool from a server the agent
    /// doesn't list is not callable, even if another agent has it connected.
    pub async fn call_tool(
        &self,
        servers: &[String],
        name: &str,
        arguments: serde_json::Value,
    ) -> Result<String, String> {
        let conn = servers
            .iter()
            .filter_map(|s| self.connections.get(s))
            .find(|c| c.tool_names.iter().any(|t| t == name))
            .ok_or_else(|| format!("MCP tool not found: {}", name))?;

        // Convert serde_json::Value to JsonObject (Map<String, Value>)
        let args_obj = match arguments {
            serde_json::Value::Object(map) => map,
            serde_json::Value::Null => serde_json::Map::new(),
            other => {
                let mut map = serde_json::Map::new();
                map.insert("input".to_string(), other);
                map
            }
        };

        let params = CallToolRequestParams::new(name.to_string()).with_arguments(args_obj);
        let result = conn
            .service
            .call_tool(params)
            .await
            .map_err(|e| format!("MCP tool call failed: {}", e))?;

        // Convert CallToolResult content to string
        let mut output = String::new();
        for content in &result.content {
            match &content.raw {
                rmcp::model::RawContent::Text(text) => {
                    if !output.is_empty() {
                        output.push('\n');
                    }
                    output.push_str(&text.text);
                }
                rmcp::model::RawContent::Image(img) => {
                    if !output.is_empty() {
                        output.push('\n');
                    }
                    output.push_str(&format!("[image: {}]", img.mime_type));
                }
                _ => {}
            }
        }

        if let Some(true) = result.is_error {
            output = format!("{{\"error\": {}}}", serde_json::json!(output));
        }

        Ok(output)
    }

    /// Status of every server that has been connected (or failed to), for the
    /// global MCP settings card. Agent-scoped views filter this by name.
    pub fn statuses(&self) -> Vec<McpServerStatus> {
        self.statuses.values().cloned().collect()
    }

    /// Shutdown all connections
    pub async fn shutdown(&mut self) {
        for (name, conn) in self.connections.drain() {
            eprintln!("Shutting down MCP server: {}", name);
            let _ = conn.service.cancel().await;
        }
        self.statuses.clear();
    }
}

impl Default for McpHub {
    fn default() -> Self {
        Self::new()
    }
}

/// Convert an MCP Tool definition to OpenAI function calling format
fn mcp_tool_to_openai(tool: &McpTool) -> serde_json::Value {
    let input_schema = serde_json::to_value(&*tool.input_schema)
        .unwrap_or_else(|_| serde_json::json!({"type": "object", "properties": {}}));

    serde_json::json!({
        "type": "function",
        "function": {
            "name": tool.name.as_ref(),
            "description": tool.description.as_deref().unwrap_or(""),
            "parameters": input_schema,
        }
    })
}
