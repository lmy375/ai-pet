//! pet-core — the shared engine behind both pet interfaces.
//!
//! Everything here is UI-agnostic: the chat pipeline (`chat`), built-in tools
//! (`tools`), MCP client management (`mcp`), settings/sessions/memory on disk,
//! and the multi-agent group-chat orchestrator (`group`). Interfaces (the Tauri
//! GUI in `src-tauri`, the CLI in `crates/pet-cli`) plug in by implementing a
//! few small traits:
//!
//! - [`chat::ChatEventSink`] — where a single agent run streams its events
//! - [`shell::TaskNotifier`] — where background-task completions are delivered
//! - [`group::GroupEvents`] — where group-chat activity is broadcast
//! - [`tools::ChatHook`] — UI side effects of the heartbeat-only `chat` tool

pub mod chat;
pub mod common;
pub mod config;
pub mod group;
pub mod heartbeat_file;
pub mod logging;
pub mod mcp;
pub mod memory;
pub mod prompt;
pub mod session;
pub mod settings;
pub mod shell;
pub mod tools;
