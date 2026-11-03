//! The stores every engine runtime needs, built once per process.
//!
//! Both interfaces used to assemble these three by hand (`mcp` hub, app log,
//! background shell tasks) and pass them individually to every runtime. One
//! constructor here keeps that wiring — and the store constructors themselves —
//! in a single place.

use std::sync::{Arc, Mutex};

use crate::group::{GroupEvents, GroupRuntime};
use crate::logging::LogStore;
use crate::mcp::McpStore;
use crate::shell::ShellStore;
use crate::swarm::{SwarmEvents, SwarmRuntime};
use crate::turn::{TurnEvents, TurnRunner};

/// Shared engine state. Cheap to clone: every field is an `Arc` internally.
#[derive(Clone)]
pub struct EngineStores {
    pub mcp: McpStore,
    pub log: LogStore,
    pub shell: ShellStore,
}

impl EngineStores {
    pub fn new(log: LogStore) -> Self {
        Self {
            mcp: crate::mcp::new_mcp_store(),
            log,
            shell: ShellStore::load_persisted(),
        }
    }

    /// GUI flavour: keeps an in-memory log tail for the debug log view.
    pub fn buffered() -> Self {
        Self::new(LogStore(Arc::new(Mutex::new(Vec::new()))))
    }

    /// CLI flavour: append-only, the buffer is never read back.
    pub fn detached() -> Self {
        Self::new(LogStore::detached())
    }

    pub fn turn(
        &self,
        events: Arc<dyn TurnEvents>,
        rt: tokio::runtime::Handle,
    ) -> Arc<TurnRunner> {
        TurnRunner::new(events, self.clone(), rt)
    }

    pub fn group(&self, events: Arc<dyn GroupEvents>) -> GroupRuntime {
        GroupRuntime::new(events, self.mcp.clone(), self.log.clone(), self.shell.clone())
    }

    pub fn swarm(
        &self,
        events: Arc<dyn SwarmEvents>,
        rt: tokio::runtime::Handle,
    ) -> Arc<SwarmRuntime> {
        SwarmRuntime::new(
            events,
            self.mcp.clone(),
            self.log.clone(),
            self.shell.clone(),
            rt,
        )
    }
}