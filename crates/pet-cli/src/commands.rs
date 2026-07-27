//! Submit dispatch: every submitted line (chat message, group message or `/`
//! command) runs in a spawned task and reports back through `AppEvent`s, so
//! the draw loop never blocks on the engine (LLM turns, MCP startup).
//!
//! List commands (`/agents`, `/sessions`, `/tasks`, `/members`) never reach
//! this module — the TUI intercepts them and opens a picker overlay instead.

use std::sync::Arc;

use pet_core::group::{self, GroupRuntime};
use pet_core::session;
use pet_core::settings::{self, get_settings};
use tokio::sync::mpsc::UnboundedSender;

use crate::app::{CliApp, TurnInput};
use crate::event::AppEvent;
use crate::sink::TuiSink;
use crate::tui::picker::{members_picker, models_picker};
use crate::Mode;

#[derive(Clone)]
pub struct SubmitCtx {
    pub cli: Arc<CliApp>,
    pub group: Arc<GroupRuntime>,
    pub tx: UnboundedSender<AppEvent>,
}

impl SubmitCtx {
    fn notice(&self, text: impl Into<String>) {
        let _ = self.tx.send(AppEvent::Notice(text.into()));
    }
    fn error(&self, text: impl Into<String>) {
        let _ = self.tx.send(AppEvent::ErrorNotice(text.into()));
    }
    fn send(&self, ev: AppEvent) {
        let _ = self.tx.send(ev);
    }
}

/// Handle one submitted line for `mode`. Ends by sending `CommandDone` (or
/// `TurnDone` for chat turns) so the UI clears its busy state.
pub fn spawn_submit(ctx: SubmitCtx, mode: Mode, line: String) {
    tokio::spawn(async move {
        match mode {
            Mode::Chat => handle_chat(&ctx, line.trim()).await,
            Mode::Group => handle_group(&ctx, line.trim()).await,
        }
    });
}

/// Run a chat turn (user message or background-task resume) in a task.
pub fn spawn_turn(ctx: SubmitCtx, input: TurnInput) {
    tokio::spawn(async move {
        run_turn(&ctx, input).await;
    });
}

/// Fetch the active agent's model list from its OpenAI-compatible `/models`
/// endpoint and open the model picker.
pub fn spawn_open_models(ctx: SubmitCtx) {
    tokio::spawn(async move {
        let Some(agent) = ctx.cli.active_agent() else {
            return ctx.error("没有可用的 Agent");
        };
        ctx.notice("正在获取模型列表…");
        match settings::list_models(agent.api_base.clone(), agent.api_key.clone()).await {
            Ok(models) if !models.is_empty() => {
                ctx.send(AppEvent::OpenPicker(models_picker(&agent.model, &models)));
            }
            Ok(_) => ctx.notice("接口没有返回任何模型"),
            Err(e) => ctx.error(format!("获取模型列表失败：{e}")),
        }
    });
}

/// Recount the tools available to the active agent (built-in + its connected
/// MCP servers) for the status bar. Mirrors how the agent loop builds its
/// registry for a normal chat turn.
pub fn spawn_refresh_tools(ctx: SubmitCtx) {
    tokio::spawn(async move {
        let Ok(settings) = get_settings() else { return };
        let Some(agent) = settings.active_agent_config() else { return };
        let mcp_defs = {
            let managers = ctx.cli.mcp_store.lock().await;
            managers.get(&agent.id).map(|m| m.definitions()).unwrap_or_default()
        };
        let web_search = !settings.search_api_key.trim().is_empty();
        let registry = pet_core::tools::ToolRegistry::new(mcp_defs, 0, false, web_search, false);
        let n = registry.definitions().as_array().map(|a| a.len()).unwrap_or(0);
        ctx.send(AppEvent::ToolsCount(n));
    });
}

/// Load the current member set and open the members picker (group `/members`).
pub fn spawn_open_members(ctx: SubmitCtx) {
    tokio::spawn(async move {
        let members = group::load(&ctx.group).await.members;
        match get_settings() {
            Ok(s) if !s.agents.is_empty() => {
                ctx.send(AppEvent::OpenPicker(members_picker(&s, &members)));
            }
            Ok(_) => ctx.notice("还没有配置 Agent"),
            Err(e) => ctx.error(e),
        }
    });
}

/// Apply the member set chosen in the picker (connects MCP for new members).
pub fn spawn_set_members(ctx: SubmitCtx, ids: Vec<String>) {
    tokio::spawn(async move {
        let settings = get_settings().unwrap_or_default();
        for id in &ids {
            if let Some(a) = settings.agent(id) {
                if let Some(msg) = ctx.cli.ensure_mcp(a).await {
                    ctx.notice(msg);
                }
            }
        }
        group::set_members(&ctx.group, ids.clone()).await;
        if ids.is_empty() {
            ctx.notice("群成员已清空");
        } else {
            let names: Vec<String> = ids
                .iter()
                .map(|id| settings.agent(id).map(|a| a.name.clone()).unwrap_or_else(|| id.clone()))
                .collect();
            ctx.notice(format!("群成员已更新：{}", names.join("、")));
        }
        ctx.send(AppEvent::CommandDone);
    });
}

async fn run_turn(ctx: &SubmitCtx, input: TurnInput) {
    // Connect the active agent's MCP servers first (lazy; GUI does it at boot).
    if let Some(agent) = ctx.cli.active_agent() {
        if let Some(msg) = ctx.cli.ensure_mcp(&agent).await {
            ctx.notice(msg);
        }
    }
    let sink = TuiSink::new(ctx.tx.clone());
    let result = ctx.cli.run_chat_turn(input, &sink).await;
    ctx.send(AppEvent::TurnDone(result));
}

async fn handle_chat(ctx: &SubmitCtx, line: &str) {
    match line.split_whitespace().next().unwrap_or("") {
        "" => ctx.send(AppEvent::CommandDone),
        "/quit" | "/exit" => ctx.send(AppEvent::Quit),
        "/help" => {
            ctx.notice(CHAT_HELP);
            ctx.send(AppEvent::CommandDone);
        }
        "/new" => {
            match session::create_session() {
                Ok(_) => ctx.notice("新会话已创建"),
                Err(e) => ctx.error(e),
            }
            ctx.send(AppEvent::CommandDone);
        }
        "/group" => {
            enter_group(ctx).await;
            ctx.send(AppEvent::CommandDone);
        }
        cmd if cmd.starts_with('/') => {
            ctx.error(format!("未知命令 {cmd}。/help 查看用法"));
            ctx.send(AppEvent::CommandDone);
        }
        _ => run_turn(ctx, TurnInput::User(line.to_string())).await,
    }
}

async fn handle_group(ctx: &SubmitCtx, line: &str) {
    match line.split_whitespace().next().unwrap_or("") {
        "" => {}
        "/quit" | "/exit" => {
            ctx.send(AppEvent::Quit);
            return;
        }
        "/back" => {
            ctx.notice("已返回单聊（群聊后台继续，可随时 /group 回来）");
            ctx.send(AppEvent::SetMode(Mode::Chat));
        }
        "/help" => ctx.notice(GROUP_HELP),
        "/pause" => {
            group::set_paused(&ctx.group, true).await;
            ctx.notice("已暂停（所有进行中的回复已中止），/resume 恢复");
        }
        "/resume" => {
            group::set_paused(&ctx.group, false).await;
            ctx.notice("已恢复");
        }
        "/reset" => {
            group::reset(&ctx.group).await;
            ctx.notice("群聊已清空（成员保留）");
        }
        "/history" => {
            let n = line
                .strip_prefix("/history")
                .and_then(|s| s.trim().parse::<usize>().ok())
                .unwrap_or(20);
            replay_history(ctx, n).await;
        }
        cmd if cmd.starts_with('/') => {
            ctx.error(format!("未知命令 {cmd}。/help 查看群聊命令"));
        }
        _ => {
            // The owner's line comes back through the transcript event stream
            // (GroupMsg), so it isn't echoed locally.
            if let Err(e) = group::send_user_message(&ctx.group, line).await {
                ctx.error(e);
            }
        }
    }
    ctx.send(AppEvent::CommandDone);
}

async fn enter_group(ctx: &SubmitCtx) {
    let state = group::load(&ctx.group).await;
    let settings = get_settings().unwrap_or_default();

    if state.members.is_empty() {
        ctx.notice("群里还没有成员。用 /members 选择参与的 Agent。");
    } else {
        let names: Vec<String> = state
            .members
            .iter()
            .map(|id| settings.agent(id).map(|a| a.name.clone()).unwrap_or_else(|| id.clone()))
            .collect();
        let paused = if state.paused { "（已暂停 — /resume 恢复）" } else { "" };
        ctx.notice(format!("群成员：{}{}", names.join("、"), paused));
        // Members' MCP servers connect up front so the first message doesn't
        // stall every agent at once.
        for id in &state.members {
            if let Some(a) = settings.agent(id) {
                if let Some(msg) = ctx.cli.ensure_mcp(a).await {
                    ctx.notice(msg);
                }
            }
        }
        // Replay the recent room history so the view has context.
        let start = state.transcript.len().saturating_sub(10);
        for msg in &state.transcript[start..] {
            ctx.send(AppEvent::GroupMsg(msg.clone()));
        }
    }
    ctx.send(AppEvent::SetMode(Mode::Group));
}

async fn replay_history(ctx: &SubmitCtx, n: usize) {
    let state = group::load(&ctx.group).await;
    if state.transcript.is_empty() {
        return ctx.notice("群聊还没有记录");
    }
    ctx.notice(format!("—— 最近 {} 条群聊 ——", n.min(state.transcript.len())));
    let start = state.transcript.len().saturating_sub(n);
    for msg in &state.transcript[start..] {
        ctx.send(AppEvent::GroupMsg(msg.clone()));
    }
}

const CHAT_HELP: &str = "命令：
  /agents              选择并切换 Agent（↑↓ 选择，Enter 切换）
  /models              选择并切换当前 Agent 的模型
  /sessions            选择并切换会话（/new 新建）
  /tasks               查看后台任务
  /group               进入多 Agent 群聊
  /quit                退出
按键：输入 / 弹出命令面板（↑↓ 选择，Enter/Tab 确认）；
输入框为空时 ↑↓ 选中工具调用/思考过程，Enter 展开或收起；
PgUp/PgDn 滚动历史。";

const GROUP_HELP: &str = "群聊命令：
  /members             选择群成员（Space 勾选，Enter 应用）
  /pause /resume       暂停 / 恢复所有 Agent
  /reset               清空群聊记录（保留成员）
  /history [n]         回放最近 n 条群聊记录
  /back                返回单聊";
