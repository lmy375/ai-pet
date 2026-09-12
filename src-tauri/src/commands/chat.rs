//! Tauri side of chat turns: thin commands over `pet_core::turn::TurnRunner`
//! plus the Tauri implementations of pet-core's delivery traits (turn events,
//! heartbeat chat hook).
//!
//! The backend owns every turn. A window only starts one (`send_chat`), watches
//! the global `turn` event, and re-attaches (`attach_turn`) whenever it mounts
//! or regains focus — so switching tabs, reloading the webview or closing the
//! panel never interrupts a reply, and each window shows whatever its session
//! is doing right now.

use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};

use pet_core::chat::UserTurn;
use pet_core::logging::LogStore;
use pet_core::mcp::McpManagerStore;
use pet_core::shell::ShellStore;
use pet_core::tools::ChatHook;
use pet_core::turn::{TurnEvents, TurnNotice, TurnRunner, TurnSnapshot};

/// Tauri-managed handle to the turn runner.
pub struct TurnStore(pub Arc<TurnRunner>);

/// Broadcasts turn activity to every window as one `turn` event; each window
/// keeps only the notices for the session it is showing.
struct TauriTurnEvents {
    app: AppHandle,
}

impl TurnEvents for TauriTurnEvents {
    fn notice(&self, notice: &TurnNotice) {
        if let Err(e) = self.app.emit("turn", notice) {
            eprintln!("failed to emit turn event: {e}");
        }
    }
}

/// Build the managed runner. Called once in `lib.rs` setup (the event sink
/// needs the app handle).
pub fn new_turn_store(
    app: AppHandle,
    mcp_store: McpManagerStore,
    log_store: LogStore,
    shell_store: ShellStore,
) -> TurnStore {
    let events = Arc::new(TauriTurnEvents { app });
    let rt = tauri::async_runtime::handle().inner().clone();
    TurnStore(TurnRunner::new(events, log_store, shell_store, mcp_store, rt))
}

/// Start a turn with the owner's input. Returns the turn id; the reply arrives
/// through the `turn` event. Fails if the session already has a turn running.
#[tauri::command]
pub async fn send_chat(
    session_id: String,
    turn: UserTurn,
    store: State<'_, TurnStore>,
) -> Result<String, String> {
    store.0.send(&session_id, turn)
}

/// The turn running in `session_id`, replayed from its start — `None` when
/// idle. Called on mount / focus / session switch to catch up with a reply that
/// began while this window wasn't looking.
#[tauri::command]
pub fn attach_turn(session_id: String, store: State<'_, TurnStore>) -> Option<TurnSnapshot> {
    store.0.attach(&session_id)
}

/// Stop the turn running in `session_id` (partial answer kept). No-op when idle.
#[tauri::command]
pub fn cancel_chat(session_id: String, store: State<'_, TurnStore>) {
    store.0.cancel(&session_id);
}

/// Ids of every session with a turn in flight (the session rail marks them).
#[tauri::command]
pub fn running_turns(store: State<'_, TurnStore>) -> Vec<String> {
    store.0.running()
}

/// UI side of the heartbeat-only `chat` tool: native system notification plus a
/// `chat-inserted` event so the active window reloads the conversation (routed
/// to whichever window the owner is looking at; the other picks it up on next
/// focus).
pub struct TauriChatHook {
    pub app: tauri::AppHandle,
}

impl ChatHook for TauriChatHook {
    fn on_chat_inserted(&self, session_id: &str, message: &str) {
        {
            use tauri_plugin_notification::NotificationExt;
            if let Err(e) = self.app.notification().builder().title("宠物").body(message).show() {
                eprintln!("chat: failed to show notification: {}", e);
            }
        }
        let label = crate::commands::window::active_window_label(&self.app);
        let payload = serde_json::json!({ "sessionId": session_id });
        if let Err(e) = self.app.emit_to(&label, "chat-inserted", payload) {
            eprintln!("chat: failed to emit chat-inserted: {}", e);
        }
    }
}
