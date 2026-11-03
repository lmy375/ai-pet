//! Scheduled heartbeat: a background loop that periodically wakes the pet up to
//! run one silent AI session (see `HEARTBEAT.md` and `SystemPrompt::Heartbeat`).
//! Each run is tracked as a `TaskKind::Heartbeat` task so it shows up in the
//! panel, but it does NOT inject into the main chat — the only way a heartbeat
//! reaches the owner is the `chat` tool.
//!
//! The interface layer starts this loop on its own runtime; `chat_hook` carries
//! the UI side effects of the `chat` tool (`None` for interfaces without one).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use crate::chat::{run_agent, AgentRun, ImageCollectingSink};
use crate::config::AiConfig;
use crate::logging::{LogSession, LogStore};
use crate::mcp::McpStore;
use crate::session;
use crate::settings::{get_settings, AgentConfig};
use crate::shell::{run_or_background, BackgroundRequest, ShellStore, TaskKind};
use crate::system_prompt::{format_interval_label, SystemPrompt};
use crate::tools::{ChatHook, RunKind, ToolContext, ToolStores};

/// How long a heartbeat session may run before it auto-converts to a background
/// task (it's launched in the background anyway, so this is mostly a formality).
const HEARTBEAT_TIMEOUT_MS: u64 = 120_000;

/// Tick once a minute and fire each agent's heartbeat when its configured
/// interval has elapsed — reading settings each tick so toggling the feature or
/// changing the interval takes effect without a restart.
pub async fn run_scheduler(
    log_store: LogStore,
    shell_store: ShellStore,
    mcp_store: McpStore,
    chat_hook: Arc<dyn ChatHook>,
) {
    // Minutes elapsed per agent since its last fire (or since it was enabled).
    // Multiple agents can have independent heartbeat schedules running at once.
    let mut elapsed: HashMap<String, u32> = HashMap::new();
    loop {
        tokio::time::sleep(Duration::from_secs(60)).await;

        let settings = get_settings().unwrap_or_default();
        // Drop counters for agents that no longer exist.
        elapsed.retain(|id, _| settings.agents.iter().any(|a| a.id == *id));

        for agent in &settings.agents {
            if !agent.heartbeat_enabled || agent.heartbeat_interval == 0 {
                // Reset so enabling it later starts a fresh interval rather than
                // firing immediately.
                elapsed.insert(agent.id.clone(), 0);
                continue;
            }

            let counter = elapsed.entry(agent.id.clone()).or_insert(0);
            *counter += 1;
            if *counter < agent.heartbeat_interval {
                continue;
            }
            *counter = 0;

            run_one(&log_store, &shell_store, &mcp_store, &chat_hook, agent).await;
        }
    }
}

/// Build the heartbeat's opening conversation: the active session's last
/// `turns` turns (so the heartbeat sees recent chat) followed by the wake-up
/// trigger as the current user turn. Falls back to just the trigger when
/// `turns == 0`, there's no active session, or it can't be loaded.
fn build_conv(turns: u32, user_msg: &str) -> Vec<serde_json::Value> {
    let trigger = crate::llm::store_message(&crate::llm::user_message(user_msg, &[]));
    if turns == 0 {
        return vec![trigger];
    }
    let id = session::list_sessions().active_id;
    if id.is_empty() {
        return vec![trigger];
    }
    let messages = match session::load_session(id) {
        Ok(s) => s.conv.messages,
        Err(_) => return vec![trigger],
    };
    let mut conv = session::recent_turns(&messages, turns as usize);
    conv.push(trigger);
    conv
}

async fn run_one(
    log_store: &LogStore,
    shell_store: &ShellStore,
    mcp_store: &McpStore,
    chat_hook: &Arc<dyn ChatHook>,
    agent: &AgentConfig,
) {
    let config = match AiConfig::from_agent(agent) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("heartbeat skipped ({}): {}", agent.name, e);
            return;
        }
    };

    let label_cadence = format_interval_label(agent.heartbeat_interval);
    let user_msg = "（系统定时心跳触发）现在自动醒来，结合最近的聊天上下文，按 HEARTBEAT.md 检查并执行需要的定时任务。";
    // Fork the (global, shared) active session's recent history so the heartbeat
    // is aware of what the owner has been talking about, then append the wake-up
    // trigger as the current turn. The heartbeat never writes this conversation
    // back — only the `chat` tool reaches the main session — so it can't disturb it.
    let conv = build_conv(agent.heartbeat_context_turns, user_msg);

    // Registration context: provides the shared store + a `heartbeat` session id
    // that matches no real conversation, and notifier = None so the heartbeat's
    // own completion is never injected into the main chat.
    let reg_ctx = ToolContext::for_session(
        ToolStores::new(log_store.clone(), shell_store.clone(), mcp_store.clone()),
        config.clone(),
        "heartbeat",
    );

    // Work context (owned, moved into the future): carries the UI chat hook and
    // the heartbeat flag so the `chat` tool is available and can reach the owner.
    // Each heartbeat is an independent conversation, not an extension of the last.
    // Give it its own LLM-log entry so the view keeps every heartbeat run, not
    // just the most recent (the shared "heartbeat" session_id is kept for task
    // routing).
    let work_ctx = ToolContext::for_session(
        ToolStores::new(log_store.clone(), shell_store.clone(), mcp_store.clone()),
        config.clone(),
        "heartbeat",
    )
    .with_chat_hook(Some(chat_hook.clone()))
    .with_kind(RunKind::Heartbeat)
    .with_log_session(LogSession::heartbeat());

    let work_config = config;
    let work_mcp = mcp_store.clone();
    let agent_id = agent.id.clone();
    let work = async move {
        let sink = ImageCollectingSink::new();
        let run = run_agent(AgentRun {
            conv,
            turn: None,
            prompt: SystemPrompt::Heartbeat { agent_id: &agent_id, interval_label: &label_cadence },
            sink: &sink,
            config: &work_config,
            mcp_store: &work_mcp,
            ctx: &work_ctx,
            max_turns: None,
        })
        .await;
        match run {
            Ok(outcome) => (Some(0), outcome.text),
            Err(e) => (Some(1), crate::tools::tool_error(format!("heartbeat failed: {}", e))),
        }
    };

    run_or_background(
        &reg_ctx.shell_store,
        reg_ctx.notifier.clone(),
        BackgroundRequest {
            session_id: &reg_ctx.session_id,
            kind: TaskKind::Heartbeat,
            label: "定时心跳".to_string(),
            input: user_msg.to_string(),
            timeout_ms: HEARTBEAT_TIMEOUT_MS,
            run_in_background: true,
        },
        work,
    )
    .await;
}