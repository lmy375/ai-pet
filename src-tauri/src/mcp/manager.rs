use crate::commands::settings::{AppSettings, McpServerConfig};
use rmcp::model::{CallToolRequestParams, Tool as McpTool};
use rmcp::service::{RoleClient, RunningService};
use rmcp::ServiceExt;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

pub type McpManagerStore = Arc<Mutex<McpManager>>;

pub fn new_mcp_store() -> McpManagerStore {
    Arc::new(Mutex::new(McpManager::new()))
}

#[derive(Debug, Clone, Serialize)]
pub struct McpServerStatus {
    pub name: String,
    pub connected: bool,
    pub tool_count: usize,
    pub tool_names: Vec<String>,
    pub error: Option<String>,
}

/// Holds a running MCP client connection
struct McpConnection {
    service: RunningService<RoleClient, ()>,
    #[allow(dead_code)]
    tools: Vec<McpTool>,
}

pub struct McpManager {
    connections: HashMap<String, McpConnection>,
    /// tool_name -> server_name mapping
    tool_map: HashMap<String, String>,
    /// Cached tool definitions in OpenAI function calling format
    tool_definitions: Vec<serde_json::Value>,
    /// Server statuses for UI
    statuses: Vec<McpServerStatus>,
}

impl McpManager {
    pub fn new() -> Self {
        Self {
            connections: HashMap::new(),
            tool_map: HashMap::new(),
            tool_definitions: Vec::new(),
            statuses: Vec::new(),
        }
    }

    /// Connect to all enabled MCP servers from config
    pub async fn start_from_settings(settings: &AppSettings) -> Self {
        let mut manager = Self::new();

        for (name, config) in &settings.mcp_servers {
            if !config.enabled {
                manager.statuses.push(McpServerStatus {
                    name: name.clone(),
                    connected: false,
                    tool_count: 0,
                    tool_names: vec![],
                    error: Some("Disabled".to_string()),
                });
                continue;
            }

            match Self::connect_server(config).await {
                Ok((service, tools)) => {
                    let tool_count = tools.len();
                    let tool_names: Vec<String> =
                        tools.iter().map(|t| t.name.to_string()).collect();
                    for tool in &tools {
                        let tool_name = tool.name.to_string();
                        let openai_def = mcp_tool_to_openai(tool);
                        manager.tool_map.insert(tool_name, name.clone());
                        manager.tool_definitions.push(openai_def);
                    }
                    manager.connections.insert(
                        name.clone(),
                        McpConnection { service, tools },
                    );
                    manager.statuses.push(McpServerStatus {
                        name: name.clone(),
                        connected: true,
                        tool_count,
                        tool_names,
                        error: None,
                    });
                }
                Err(e) => {
                    eprintln!("Failed to connect MCP server '{}': {}", name, e);
                    manager.statuses.push(McpServerStatus {
                        name: name.clone(),
                        connected: false,
                        tool_count: 0,
                        tool_names: vec![],
                        error: Some(e),
                    });
                }
            }
        }

        manager
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

        let service: RunningService<RoleClient, ()> = ().serve(transport)
            .await
            .map_err(|e| format!("Failed to initialize MCP client: {}", e))?;

        let tools = service
            .list_all_tools()
            .await
            .map_err(|e| format!("Failed to list tools: {}", e))?;

        Ok((service, tools))
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

        let service: RunningService<RoleClient, ()> = ().serve(transport)
            .await
            .map_err(|e| format!("Failed to initialize MCP client: {}", e))?;

        let tools = service
            .list_all_tools()
            .await
            .map_err(|e| format!("Failed to list tools: {}", e))?;

        Ok((service, tools))
    }

    /// Get all MCP tool definitions in OpenAI function calling format
    pub fn definitions(&self) -> Vec<serde_json::Value> {
        self.tool_definitions.clone()
    }

    /// Call an MCP tool by name
    pub async fn call_tool(
        &self,
        name: &str,
        arguments: serde_json::Value,
    ) -> Result<String, String> {
        let server_name = self
            .tool_map
            .get(name)
            .ok_or_else(|| format!("MCP tool not found: {}", name))?;

        let conn = self
            .connections
            .get(server_name)
            .ok_or_else(|| format!("MCP server not connected: {}", server_name))?;

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

        let tool_name = name.to_string();
        let params = CallToolRequestParams::new(tool_name).with_arguments(args_obj);
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

    /// Get status of all servers
    pub fn statuses(&self) -> &[McpServerStatus] {
        &self.statuses
    }

    /// Shutdown all connections
    pub async fn shutdown(&mut self) {
        for (name, conn) in self.connections.drain() {
            eprintln!("Shutting down MCP server: {}", name);
            let _ = conn.service.cancel().await;
        }
        self.tool_map.clear();
        self.tool_definitions.clear();
        self.statuses.clear();
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
