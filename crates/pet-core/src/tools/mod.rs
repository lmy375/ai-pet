mod context;
mod registry;
mod tool;
pub mod agent_tools;
pub mod chat_tool;
pub mod shell_tools;
pub mod file_tools;
pub mod read_image_tool;
pub mod screenshot_tool;
pub mod swarm_tools;
pub mod web_search_tool;

pub use context::{ChatHook, ToolContext, ToolStores};
pub use registry::{
    builtin_catalog, catalog_for_ui, BuiltinTool, RunGate, RunKind, SwarmRole, ToolCatalogEntry,
    ToolPolicy, ToolRegistry, ToolScope,
};
pub use tool::{parse_args, tool_error, Tool, ToolSpec};