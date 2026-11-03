mod commands;
mod telegram;

use pet_core::logging::log_dir;
use pet_core::stores::EngineStores;
use tauri::Manager;

/// The stores every core runtime shares, plus the GUI-only Telegram one.
/// Built once and moved into `setup`, which derives each runtime from the
/// engine stores instead of re-reading `app.state::<T>()` for the same three.
#[derive(Clone)]
struct CoreStores {
    engine: EngineStores,
    telegram: telegram::TelegramStore,
}

impl CoreStores {
    fn new() -> Self {
        Self {
            engine: EngineStores::buffered(),
            telegram: telegram::new_telegram_store(),
        }
    }
}

/// Install the app menu, appending the devtools toggle to the View submenu.
fn app_setup_menu(app: &tauri::App) {
    if let Err(e) = install_menu(app.handle()) {
        eprintln!("Failed to set app menu: {e}");
    }
}

fn install_menu(handle: &tauri::AppHandle) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem, MenuItemKind, PredefinedMenuItem};
    let menu = Menu::default(handle)?;
    let devtools = MenuItem::with_id(
        handle,
        "toggle-devtools",
        "Toggle Web Console",
        true,
        Some("CmdOrCtrl+Shift+I"),
    )?;
    let items = menu.items()?;
    for item in &items {
        if let MenuItemKind::Submenu(sub) = item {
            if sub.text()?.as_str() == "View" {
                sub.append(&PredefinedMenuItem::separator(handle)?)?;
                sub.append(&devtools)?;
                break;
            }
        }
    }
    handle.set_menu(menu)?;
    Ok(())
}

/// Restore the pet window to its last position (it starts hidden so it's
/// positioned before appearing, avoiding a center flash) and keep every
/// window on a visible monitor.
fn app_setup_windows(handle: &tauri::AppHandle) {
    commands::window::restore_main_window(handle);
    commands::window::start_visibility_watch(handle.clone());
}

/// Build and manage the three runtimes (their event sinks need the app
/// handle), then start the background work that uses them: MCP servers,
/// Telegram bots and the scheduled-heartbeat loop.
fn app_setup_background(app: &tauri::App, core: &CoreStores) {
    let turns = commands::chat::new_turn_store(app.handle().clone(), core.engine.clone());
    let turn_runner = turns.0.clone();
    app.manage(turns);
    app.manage(commands::group::new_group_store(
        app.handle().clone(),
        core.engine.clone(),
    ));
    app.manage(commands::swarm::new_swarm_store(
        app.handle().clone(),
        core.engine.clone(),
    ));

    let background = core.clone();
    tauri::async_runtime::spawn(async move {
        let settings = pet_core::settings::get_settings().unwrap_or_default();

        // Connect every enabled MCP server some agent references — one
        // process per server, shared by all of them. A server switched off,
        // or referenced by nobody, isn't started.
        let servers = settings.referenced_server_configs();
        if !servers.is_empty() {
            background.engine.mcp.write().await.ensure(&servers).await;
            eprintln!("MCP servers initialized");
        }

        // Start a Telegram bot for every agent with telegram enabled.
        commands::telegram::restart_all_bots(
            &background.telegram,
            turn_runner,
            background.engine.log.clone(),
        )
        .await;
    });

    // The scheduled-heartbeat loop checks settings each tick, so it's a no-op
    // until the owner enables it in Settings.
    commands::heartbeat::start_scheduler(
        app.handle().clone(),
        core.engine.log.clone(),
        core.engine.shell.clone(),
        core.engine.mcp.clone(),
    );
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Ensure log directory exists
    let _ = std::fs::create_dir_all(log_dir());

    // Ensure each configured agent's memory dir + mandatory files exist
    // (`memory/<id>/{SOUL,USER,MEMORY,HEARTBEAT}.md`).
    if let Ok(settings) = pet_core::settings::get_settings() {
        pet_core::settings::ensure_agent_files(&settings);
    }

    let core = CoreStores::new();
    let setup_core = core.clone();
    let engine = core.engine.clone();
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .manage(engine.log)
        .manage(engine.shell)
        .manage(engine.mcp)
        .manage(core.telegram)
        .manage(commands::window::ActiveWindow(std::sync::Mutex::new(
            "main".to_string(),
        )))
        .on_menu_event(|app, event| {
            if event.id() == "toggle-devtools" {
                #[cfg(debug_assertions)]
                {
                    let target = app
                        .webview_windows()
                        .into_iter()
                        .find(|(_, w)| w.is_focused().unwrap_or(false))
                        .map(|(_, w)| w)
                        .or_else(|| app.get_webview_window("main"));
                    if let Some(wv) = target {
                        if wv.is_devtools_open() {
                            wv.close_devtools();
                        } else {
                            wv.open_devtools();
                        }
                    }
                }
            }
        })
        .setup(move |app| {
            app_setup_menu(app);
            app_setup_windows(app.handle());
            app_setup_background(app, &setup_core);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::chat::send_chat,
            commands::chat::attach_turn,
            commands::chat::cancel_chat,
            commands::chat::running_turns,
            commands::settings::get_settings,
            commands::settings::save_settings,
            commands::settings::get_config_raw,
            commands::settings::save_config_raw,
            commands::settings::set_active_agent,
            commands::settings::set_agent_model,
            commands::settings::new_agent,
            commands::settings::new_model,
            commands::settings::new_mcp_server,
            commands::memory::get_memory_file,
            commands::memory::save_memory_file,
            commands::open::open_dir,
            commands::settings::default_live2d_dir,
            commands::settings::download_example_live2d,
            commands::settings::list_models,
            commands::settings::list_providers,
            commands::settings::test_model,
            commands::gallery::default_gallery_dir,
            commands::workdir::get_workdir,
            commands::workdir::set_workdir,
            commands::gallery::list_gallery_media,
            commands::skills::list_skills,
            commands::skills::expand_skill_command,
            commands::prompts::list_prompts,
            commands::prompts::save_prompt,
            commands::prompts::reset_prompt,
            commands::tools::list_tools,
            commands::tools::set_tool_enabled,
            commands::tools::save_tool_description,
            commands::tools::reset_tool_description,
            commands::window::open_panel,
            #[cfg(debug_assertions)]
            commands::window::open_debug,
            commands::window::open_devtools,
            commands::window::keep_window_visible,
            commands::window::save_window_position,
            commands::window::save_ball_position,
            commands::window::collapse_to_ball,
            commands::window::expand_from_ball,
            commands::window::set_active_window,
            commands::debug::get_logs,
            commands::debug::clear_logs,
            commands::debug::get_llm_index,
            commands::debug::get_llm_entry,
            commands::shell::check_task_status,
            commands::shell::list_tasks,
            commands::shell::kill_task,
            commands::mcp::get_mcp_status,
            commands::mcp::reconnect_mcp,
            commands::mcp::sync_mcp_server,
            commands::tools::list_available_tools,
            commands::session::list_sessions,
            commands::session::set_active_session,
            commands::session::load_session,
            commands::session::create_session,
            commands::session::rename_session,
            commands::session::delete_session,
            commands::session::prune_session_items,
            commands::telegram::get_telegram_status,
            commands::telegram::reconnect_telegram,
            commands::group::group_list,
            commands::group::group_create,
            commands::group::group_load,
            commands::group::group_configure,
            commands::group::group_delete,
            commands::group::group_send,
            commands::group::group_set_paused,
            commands::group::group_reset,
            commands::swarm::swarm_list,
            commands::swarm::swarm_create,
            commands::swarm::swarm_load,
            commands::swarm::swarm_configure,
            commands::swarm::swarm_create_task,
            commands::swarm::swarm_delete_task,
            commands::swarm::swarm_delete,
            commands::swarm::swarm_start,
            commands::swarm::swarm_end,
            commands::swarm::swarm_stop,
            commands::swarm::swarm_send_main,
            commands::swarm::swarm_cancel_main,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app_handle, event| {
        #[cfg(target_os = "macos")]
        if let tauri::RunEvent::Reopen { .. } = event {
            // Dock click or Finder relaunch of an already-running app. The
            // panel keeps the screen while it exists; otherwise recover the
            // pet. A hidden pet plus an off-screen ball looks like a dead
            // process, and `has_visible_windows` cannot express that.
            commands::window::present_foreground(app_handle);
        }
        #[cfg(not(target_os = "macos"))]
        let _ = (app_handle, event);
    });
}
