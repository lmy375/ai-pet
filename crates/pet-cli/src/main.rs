//! pet-cli — terminal interface to the desktop AI pet, as a ratatui TUI.
//!
//! Same engine, config, memory and session files as the GUI (they can run at
//! the same time). Scope is deliberately small: single-agent chat (markdown
//! rendered, collapsible tool calls / reasoning), agent switching, and the
//! multi-agent group room. Typing `/` pops up the command palette. Agent
//! configuration (models, keys, MCP servers, heartbeats, Telegram) stays in
//! the GUI's Settings — or edit `config.yaml` directly.

mod app;
mod commands;
mod event;
mod printer;
mod tui;
mod ui;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use pet_core::chat::{StreamEvent, UserTurn};
use pet_core::group::GroupRuntime;
use pet_core::logging::{log_dir, LogStore};
use pet_core::settings::get_settings;
use pet_core::shell::{load_persisted_tasks, pending_notify_count, ShellStore};
use pet_core::turn::{TurnNotice, TurnOrigin, TurnRunner};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use ratatui::crossterm::event::{
    KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::supports_keyboard_enhancement;

use app::CliApp;
use commands::SubmitCtx;
use event::{AppEvent, CliTurnEvents, TuiGroupEvents};
use printer::OneshotPrinter;
use tui::{Action, TuiApp};

#[derive(Clone, Copy, PartialEq)]
pub enum Mode {
    Chat,
    Group,
}

fn print_usage() {
    println!("pet-cli — 桌面宠物的命令行界面（与 GUI 共享配置和会话）");
    println!();
    println!("用法:");
    println!("  pet-cli               交互 TUI");
    println!("  pet-cli -p <消息>     单次执行：发送一条消息，输出回复后退出");
    println!();
    println!("TUI 内：输入 / 弹出命令面板（↑↓ 选择，Enter/Tab 确认）；/help 查看全部命令");
}

fn main() {
    // A leading `--` is what package-manager wrappers insert when forwarding
    // args (`pnpm cli -- -p "hi"`), and it lands here as a literal argument.
    // Skipping it means both that form and the bare one work.
    let args: Vec<String> = std::env::args()
        .skip(1)
        .skip_while(|a| a == "--")
        .collect();
    let mut oneshot: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-p" | "--print" => {
                if i + 1 >= args.len() {
                    eprintln!("-p 需要一条消息");
                    std::process::exit(2);
                }
                oneshot = Some(args[i + 1].clone());
                i += 2;
            }
            "-h" | "--help" => {
                print_usage();
                return;
            }
            other => {
                eprintln!("未知参数: {other}");
                print_usage();
                std::process::exit(2);
            }
        }
    }

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime");
    let code = rt.block_on(async_main(oneshot));
    std::process::exit(code);
}

async fn async_main(oneshot: Option<String>) -> i32 {
    let _ = std::fs::create_dir_all(log_dir());
    if let Ok(settings) = get_settings() {
        for agent in &settings.agents {
            let _ = pet_core::memory::ensure_memory_files(&agent.id);
            let _ = pet_core::heartbeat_file::ensure_heartbeat_file(&agent.id);
        }
    }

    // Everything (terminal input, turn activity, group activity) flows through
    // this one channel into the UI loop.
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<AppEvent>();

    let log_store = LogStore(Arc::new(Mutex::new(Vec::new())));
    let shell_store = ShellStore(Arc::new(Mutex::new(load_persisted_tasks())));
    let mcp_store = pet_core::mcp::new_mcp_store();
    let cli = Arc::new(CliApp {
        turns: TurnRunner::new(
            Arc::new(CliTurnEvents(tx.clone())),
            LogStore(log_store.0.clone()),
            ShellStore(shell_store.0.clone()),
            mcp_store.clone(),
            tokio::runtime::Handle::current(),
        ),
        log_store,
        shell_store,
        mcp_store,
    });

    // One-shot mode: plain streaming to stdout, then exit — but not before the
    // turn's background tasks (spawn_subagent / background bash) are drained.
    if let Some(msg) = oneshot {
        let code = run_oneshot(&cli, &mut rx, msg).await;
        cli.shutdown_mcp().await;
        return code;
    }

    // Group runtime (shared state with the GUI's group page, seeded from disk).
    let group_rt = Arc::new(GroupRuntime::new(
        Arc::new(TuiGroupEvents(tx.clone())),
        cli.mcp_store.clone(),
        LogStore(cli.log_store.0.clone()),
        ShellStore(cli.shell_store.0.clone()),
    ));

    let mut terminal = ratatui::init();
    // Plain terminals send Shift+Enter as a bare CR, indistinguishable from
    // Enter. The kitty keyboard protocol reports the modifier — ask for it where
    // it exists (Ctrl+J stays as the universal fallback). Must happen before the
    // reader thread starts, or it would swallow the capability reply.
    let shift_enter = enable_key_disambiguation();
    event::spawn_term_reader(tx.clone());

    let ctx = SubmitCtx { cli: cli.clone(), group: group_rt, tx: tx.clone() };
    let mut app = TuiApp::new();
    app.shift_enter = shift_enter;
    // A turn may already be running in the active session (started by the GUI,
    // or a background-task resumption): show it from where it is.
    sync_turn(&cli, &tx);
    let mut tick = tokio::time::interval(Duration::from_millis(120));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    'outer: loop {
        // Recount available tools when the agent / MCP set may have changed
        // (set at startup and by refresh_header after turns/commands).
        if app.tools_dirty {
            app.tools_dirty = false;
            commands::spawn_refresh_tools(ctx.clone());
        }

        if terminal.draw(|f| tui::draw::draw(f, &mut app)).is_err() {
            break;
        }

        let first = tokio::select! {
            ev = rx.recv() => match ev {
                Some(ev) => Some(ev),
                None => break,
            },
            _ = tick.tick(), if app.busy => {
                app.spin = app.spin.wrapping_add(1);
                None
            }
        };

        // Apply the received event plus everything already queued (token
        // streams arrive in bursts), then draw once.
        let mut actions = Vec::new();
        if let Some(ev) = first {
            actions.extend(app.apply(ev));
        }
        while let Ok(ev) = rx.try_recv() {
            actions.extend(app.apply(ev));
        }

        for action in actions {
            match action {
                Action::Submit(line) => commands::spawn_submit(ctx.clone(), app.mode, line),
                Action::SyncTurn => sync_turn(&cli, &tx),
                Action::OpenTasks => {
                    let tasks = pet_core::shell::list_tasks(&cli.shell_store);
                    let _ = if tasks.is_empty() {
                        tx.send(AppEvent::Notice("没有后台任务".to_string()))
                    } else {
                        tx.send(AppEvent::OpenPicker(tui::picker::tasks_picker(&tasks)))
                    };
                }
                Action::OpenModels => commands::spawn_open_models(ctx.clone()),
                Action::OpenMembers => commands::spawn_open_members(ctx.clone()),
                Action::SetMembers(ids) => commands::spawn_set_members(ctx.clone(), ids),
                Action::Quit => break 'outer,
            }
        }
    }

    if shift_enter {
        let _ = execute!(std::io::stdout(), PopKeyboardEnhancementFlags);
    }
    ratatui::restore();
    cli.shutdown_mcp().await;
    0
}

/// Overall cap on waiting for background tasks in one-shot mode, so a stuck
/// task can't hang `-p` forever. Override with PET_ONESHOT_WAIT_MS (0 = 无上限).
const ONESHOT_WAIT_MS_DEFAULT: u64 = 600_000;

fn oneshot_wait_ms() -> u64 {
    std::env::var("PET_ONESHOT_WAIT_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(ONESHOT_WAIT_MS_DEFAULT)
}

/// Attach the TUI to the active session's running turn, if any, by replaying
/// it into the event stream. Called at startup and after a session switch.
fn sync_turn(cli: &CliApp, tx: &UnboundedSender<AppEvent>) {
    let active_id = pet_core::session::list_sessions().active_id;
    if let Some(snap) = cli.turns.attach(&active_id) {
        let _ = tx.send(AppEvent::TurnSnapshot(snap));
    }
}

/// Nothing running, nothing queued, and no task left that will notify.
fn quiescent(cli: &CliApp) -> bool {
    cli.turns.is_idle() && pending_notify_count(&cli.shell_store) == 0
}

/// `-p` mode: send one message, print its reply, then stay until every
/// background task it spawned has finished and been fed back — each completion
/// resumes the session with a follow-up turn (run by the runner, printed here,
/// possibly spawning more). Exiting earlier would silently drop work the model
/// was told "you will be notified" about. Returns the process exit code.
async fn run_oneshot(cli: &Arc<CliApp>, rx: &mut UnboundedReceiver<AppEvent>, msg: String) -> i32 {
    // Connect the active agent's MCP servers first (lazy; GUI does it at boot).
    if let Some(agent) = cli.active_agent() {
        if let Some(m) = cli.ensure_mcp(&agent).await {
            println!("{}{}{}", ui::DIM, m, ui::RESET);
        }
    }
    let session_id = match cli
        .active_session_id()
        .and_then(|sid| cli.turns.send(&sid, UserTurn::text(msg)).map(|_| sid))
    {
        Ok(sid) => sid,
        Err(e) => {
            eprintln!("{}✗ {}{}", ui::RED, e, ui::RESET);
            return 1;
        }
    };

    let mut printer = OneshotPrinter::new();
    let mut code = 0;
    let mut waiting_notice = false;
    let wait_ms = oneshot_wait_ms();
    let deadline =
        (wait_ms > 0).then(|| tokio::time::Instant::now() + Duration::from_millis(wait_ms));

    loop {
        // The overall cap applies only while purely waiting on background tasks
        // (no turn running), so a long but live reply is never cut off.
        let event = match deadline.filter(|_| cli.turns.is_idle()) {
            Some(d) => match tokio::time::timeout_at(d, rx.recv()).await {
                Ok(ev) => ev,
                Err(_) => {
                    eprintln!(
                        "{}✗ 等待后台任务超过 {}ms，放弃（还有 {} 个在跑；PET_ONESHOT_WAIT_MS 可调，0 为无上限）{}",
                        ui::RED,
                        wait_ms,
                        pending_notify_count(&cli.shell_store),
                        ui::RESET
                    );
                    return 1;
                }
            },
            None => rx.recv().await,
        };
        let Some(event) = event else { return code };

        match event {
            AppEvent::Turn(TurnNotice::Started {
                session_id: sid,
                origin: TurnOrigin::Completion { label },
                ..
            }) => {
                waiting_notice = false;
                if sid == session_id {
                    println!("{}后台任务完成：{} — 自动继续对话{}", ui::DIM, label, ui::RESET);
                } else {
                    println!("{}后台任务完成（其他会话）：{}，已在后台续聊{}", ui::DIM, label, ui::RESET);
                }
            }
            AppEvent::Turn(TurnNotice::Stream { session_id: sid, event, .. }) if sid == session_id => {
                if matches!(event, StreamEvent::Error { .. }) {
                    code = 1;
                }
                printer.apply(&event);
            }
            AppEvent::Turn(TurnNotice::Finished { .. }) => {
                // A finishing task marks itself done a moment before its
                // completion turn registers, so confirm after a short grace.
                if quiescent(cli) {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    if quiescent(cli) {
                        return code;
                    }
                }
                let pending = pending_notify_count(&cli.shell_store);
                if !waiting_notice && pending > 0 {
                    waiting_notice = true;
                    println!("{}… 等待 {} 个后台任务完成{}", ui::DIM, pending, ui::RESET);
                }
            }
            _ => {} // 其他事件在 one-shot 下与本会话无关，忽略
        }
    }
}

/// Ask the terminal to disambiguate escape codes (kitty keyboard protocol), so
/// Enter arrives with its Shift/Alt modifier. `false` = unsupported terminal,
/// nothing pushed, nothing to pop.
fn enable_key_disambiguation() -> bool {
    if !supports_keyboard_enhancement().unwrap_or(false) {
        return false;
    }
    execute!(
        std::io::stdout(),
        PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
    )
    .is_ok()
}
