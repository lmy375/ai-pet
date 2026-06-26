//! Scheduled heartbeat: a background loop that periodically wakes the pet up to
//! run one silent AI session (see `HEARTBEAT.md` and `prompt::prepend_heartbeat
//! _system_messages`). Each run is tracked as a `TaskKind::Heartbeat` task so it
//! shows up in the panel, but it does NOT inject into the main chat — the only
//! way a heartbeat reaches the owner is the `chat` tool.

use std::time::Duration;

use crate::commands::chat::{run_agent_loop, CollectingSink};
use crate::commands::debug::LogStore;
use crate::commands::settings::get_settings;
use crate::commands::shell::{run_or_background, ShellStore, TaskKind};
use crate::commands::prompt;
use crate::commands::session;
use crate::config::AiConfig;
use crate::mcp::McpManagerStore;
use crate::tools::ToolContext;

/// How long a heartbeat session may run before it auto-converts to a background
/// task (it's launched in the background anyway, so this is mostly a formality).
const HEARTBEAT_TIMEOUT_MS: u64 = 120_000;

/// Spawn the heartbeat scheduler. Ticks once a minute and fires a heartbeat when
/// the configured interval has elapsed — reading settings each tick so toggling
/// the feature or changing the interval takes effect without a restart.
pub fn start_scheduler(
    app: tauri::AppHandle,
    log_store: LogStore,
    shell_store: ShellStore,
    mcp_store: McpManagerStore,
) {
    tauri::async_runtime::spawn(async move {
        // Minutes elapsed since the last fire (or since the feature was enabled).
        let mut elapsed_min: u32 = 0;
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;

            let settings = get_settings().unwrap_or_default();
            if !settings.heartbeat_enabled || settings.heartbeat_interval == 0 {
                // Reset so enabling it later starts a fresh interval rather than
                // firing immediately.
                elapsed_min = 0;
                continue;
            }

            elapsed_min += 1;
            if elapsed_min < settings.heartbeat_interval {
                continue;
            }
            elapsed_min = 0;

            run_one_heartbeat(
                &app,
                &log_store,
                &shell_store,
                &mcp_store,
                settings.heartbeat_interval,
                settings.heartbeat_context_turns,
            )
            .await;
        }
    });
}

/// Build the heartbeat's opening conversation: the active session's last
/// `turns` turns (so the heartbeat sees recent chat) followed by the wake-up
/// trigger as the current user turn. Falls back to just the trigger when
/// `turns == 0`, there's no active session, or it can't be loaded.
fn build_heartbeat_conv(turns: u32, user_msg: &str) -> Vec<serde_json::Value> {
    let trigger = serde_json::json!({ "role": "user", "content": user_msg });
    if turns == 0 {
        return vec![trigger];
    }
    let id = session::list_sessions().active_id;
    if id.is_empty() {
        return vec![trigger];
    }
    let messages = match session::load_session(id) {
        Ok(s) => s.messages,
        Err(_) => return vec![trigger],
    };
    let mut conv = session::recent_turns(&messages, turns as usize);
    conv.push(trigger);
    conv
}

async fn run_one_heartbeat(
    app: &tauri::AppHandle,
    log_store: &LogStore,
    shell_store: &ShellStore,
    mcp_store: &McpManagerStore,
    interval_min: u32,
    context_turns: u32,
) {
    let config = match AiConfig::from_settings() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("heartbeat skipped: {}", e);
            return;
        }
    };

    let label_cadence = prompt::format_interval_label(interval_min);
    let user_msg = "（系统定时心跳触发）现在自动醒来，结合最近的聊天上下文，按 HEARTBEAT.md 检查并执行需要的定时任务。";
    // Fork the active session's recent history so the heartbeat is aware of what
    // the owner has been talking about, then append the wake-up trigger as the
    // current turn. The heartbeat never writes this conversation back — only the
    // `chat` tool reaches the main session — so the fork can't disturb it.
    let mut conv = build_heartbeat_conv(context_turns, user_msg);
    prompt::prepend_heartbeat_system_messages(&mut conv, &label_cadence);

    // Registration context: provides the shared store + a `heartbeat` session id
    // that matches no real conversation, and notifier = None so the heartbeat's
    // own completion is never injected into the main chat.
    let reg_ctx = ToolContext::new(
        LogStore(log_store.0.clone()),
        ShellStore(shell_store.0.clone()),
        config.clone(),
        mcp_store.clone(),
        "heartbeat".to_string(),
        None,
        None,
        false,
    );

    // Work context (owned, moved into the future): carries the app handle and the
    // heartbeat flag so the `chat` tool is available and can reach the owner.
    let mut work_ctx = ToolContext::new(
        LogStore(log_store.0.clone()),
        ShellStore(shell_store.0.clone()),
        config.clone(),
        mcp_store.clone(),
        "heartbeat".to_string(),
        None,
        Some(app.clone()),
        true,
    );
    // Each heartbeat is an independent conversation, not an extension of the last.
    // Give it its own LLM-log group so the view keeps every heartbeat run, not
    // just the most recent (the shared "heartbeat" session_id is kept for task
    // routing).
    work_ctx.log_session = format!("heartbeat:{}", uuid::Uuid::new_v4());

    let work_config = config.clone();
    let work_mcp = mcp_store.clone();
    let work = async move {
        let sink = CollectingSink::new();
        match run_agent_loop(conv, &sink, &work_config, &work_mcp, &work_ctx).await {
            Ok(text) => (Some(0), text),
            Err(e) => (Some(1), format!(r#"{{"error": "heartbeat failed: {}"}}"#, e.replace('"', "'"))),
        }
    };

    run_or_background(
        &reg_ctx,
        TaskKind::Heartbeat,
        "定时心跳".to_string(),
        user_msg.to_string(),
        HEARTBEAT_TIMEOUT_MS,
        true,
        work,
    )
    .await;
}
