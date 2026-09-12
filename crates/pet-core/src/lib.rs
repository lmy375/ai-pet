//! pet-core — the shared engine behind both pet interfaces.
//!
//! Everything here is UI-agnostic: the chat pipeline (`chat`), built-in tools
//! (`tools`), MCP client management (`mcp`), settings/sessions/memory on disk,
//! and the multi-agent group-chat orchestrator (`group`). Interfaces (the Tauri
//! GUI in `src-tauri`, the CLI in `crates/pet-cli`) plug in by implementing a
//! few small traits:
//!
//! - [`turn::TurnEvents`] — where chat-turn activity (start / stream / finish)
//!   is broadcast; turns themselves are run and persisted by [`turn::TurnRunner`]
//! - [`chat::ChatEventSink`] — where a single agent run streams its events
//!   (heartbeats, group agents; chat turns use the runner's own sink)
//! - [`group::GroupEvents`] — where group-chat activity is broadcast
//! - [`tools::ChatHook`] — UI side effects of the heartbeat-only `chat` tool
//!
//! [`shell::TaskNotifier`] (background-task completions) is implemented by the
//! runner, which injects each completion into its session as a new turn.

pub mod chat;
pub mod common;
pub mod config;
pub mod group;
pub mod heartbeat_file;
pub mod llm;
pub mod logging;
pub mod mcp;
pub mod memory;
pub mod prompt;
pub mod provider;
pub mod session;
pub mod settings;
pub mod shell;
pub mod skills;
pub mod tools;
pub mod turn;
