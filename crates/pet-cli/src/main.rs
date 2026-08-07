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
mod sink;
mod tui;
mod ui;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use pet_core::group::GroupRuntime;
use pet_core::logging::{log_dir, LogStore};
use pet_core::settings::get_settings;
use pet_core::shell::{load_persisted_tasks, ShellStore};
use ratatui::crossterm::event::{
    KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::supports_keyboard_enhancement;

use app::{CliApp, TurnInput};
use commands::SubmitCtx;
use event::{AppEvent, CliNotifier, TuiGroupEvents};
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
    let args: Vec<String> = std::env::args().skip(1).collect();
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

    // Everything (terminal input, stream events, group activity, task
    // completions) flows through this one channel into the UI loop.
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<AppEvent>();

    let cli = Arc::new(CliApp {
        log_store: LogStore(Arc::new(Mutex::new(Vec::new()))),
        shell_store: ShellStore(Arc::new(Mutex::new(load_persisted_tasks()))),
        mcp_store: pet_core::mcp::new_mcp_store(),
        notifier: Arc::new(CliNotifier(tx.clone())),
    });

    // One-shot mode: plain streaming to stdout, then exit.
    if let Some(msg) = oneshot {
        let sink = sink::OneshotSink::new();
        let code = match cli.run_chat_turn(TurnInput::User(msg), &sink).await {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("{}✗ {}{}", ui::RED, e, ui::RESET);
                1
            }
        };
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
                Action::Turn(input) => commands::spawn_turn(ctx.clone(), input),
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
