mod butler_history;
mod commands;
mod companionship;
mod detail_history;
mod config;
mod consolidate;
mod db;
mod decision_log;
mod feedback_history;
mod input_idle;
mod local_file_input;
mod log_rotation;
mod memory_retrieval;
mod mcp;
mod mood;
mod mood_history;
mod mute_count;
mod proactive;
mod speech_history;
mod task_heartbeat;
mod time_ambiguity;
mod task_queue;
mod telegram;
mod tool_call_history;
mod tool_review;
mod tool_review_policy;
mod tool_risk;
mod tools;
mod url_fetch;
mod visual_memory;
mod wake_detector;
mod window_state;

use commands::debug::{log_dir, new_process_counters, LogStore};
use commands::shell::ShellStore;
use std::collections::HashMap;
use std::sync::Arc;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Ensure log directory exists
    let _ = std::fs::create_dir_all(log_dir());

    // 强制 eval `BOOT_TIME` LazyLock — 让 uptime 锚点贴近 main() 入口而非
    // 首次 `get_process_uptime_secs` 调用。PanelDebug 「⌚ 已运行」字段
    // 从此读 elapsed。
    let _ = *commands::debug::BOOT_TIME;

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        // GOAL 052：OS notification 通道 plugin。pet 不在前台 ≥ 30s 时
        // proactive utterance 同步走系统通知，让 user 切到别的 app 仍能感知。
        .plugin(tauri_plugin_notification::init())
        .manage(LogStore(Arc::new(std::sync::Mutex::new(Vec::new()))))
        .manage(ShellStore(Arc::new(std::sync::Mutex::new(HashMap::new()))))
        .manage(new_process_counters())
        .manage(decision_log::new_decision_log())
        .manage(tool_review::new_tool_review_registry())
        .manage(wake_detector::new_wake_detector())
        .manage(mcp::new_mcp_store())
        .manage(telegram::new_telegram_store())
        .manage(telegram::warnings::new_store())
        .manage(proactive::new_interaction_clock())
        .setup(|app| {
            // Initialize MCP servers from config on app start
            let mcp_store = app.state::<mcp::McpManagerStore>().inner().clone();
            let telegram_store = app.state::<telegram::TelegramStore>().inner().clone();
            let tg_warnings = app
                .state::<telegram::warnings::TgStartupWarningStore>()
                .inner()
                .clone();
            let log_store = app.state::<LogStore>().inner().clone();
            let shell_store = app.state::<ShellStore>().inner().clone();
            let process_counters = app
                .state::<commands::debug::ProcessCountersStore>()
                .inner()
                .clone();
            let app_handle_for_tg = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let settings = commands::settings::get_settings().unwrap_or_default();

                // Initialize MCP servers
                if !settings.mcp_servers.is_empty() {
                    let manager = mcp::McpManager::start_from_settings(&settings).await;
                    *mcp_store.lock().await = manager;
                    eprintln!("MCP servers initialized");
                }

                // Initialize Telegram bot
                let tg = &settings.telegram;
                if tg.enabled && !tg.bot_token.is_empty() {
                    let mcp_clone = mcp_store.clone();
                    match telegram::bot::TelegramBot::start(
                        tg.clone(),
                        mcp_clone,
                        log_store,
                        shell_store,
                        process_counters,
                        app_handle_for_tg,
                        tg_warnings.clone(),
                    )
                    .await
                    {
                        Ok(bot) => {
                            *telegram_store.lock().await = Some(bot);
                            eprintln!("Telegram bot started");
                        }
                        Err(e) => {
                            eprintln!("Failed to start Telegram bot: {}", e);
                            telegram::warnings::push(&tg_warnings, "bot_start", e);
                        }
                    }
                }
            });

            // GOAL 052：监听 main 窗口 focus / blur 事件 → 更新前台状态
            // tracker，proactive emit 路径据此判断是否走 OS notification 通道。
            // GOAL 053：close-to-hide —— 主窗口红圆 / Cmd+W 不退出，仅隐藏
            // 到 tray，让 proactive 循环 / Telegram bot / OS notification 通道
            // 在后台继续；只有 tray menu 的「退出」才真 quit。
            if let Some(main) = app.get_webview_window("main") {
                let main_for_handler = main.clone();
                let app_for_handler = app.handle().clone();
                main.on_window_event(move |event| match event {
                    tauri::WindowEvent::Focused(in_focus) => {
                        proactive::update_window_focus(*in_focus);
                        // 053-part2：用户重新关注 main 窗口视作「已读」，清
                        // unread 计数 + 复原 tray tooltip。
                        if *in_focus {
                            proactive::clear_unread_proactive(&app_for_handler);
                        }
                    }
                    tauri::WindowEvent::CloseRequested { api, .. } => {
                        api.prevent_close();
                        let _ = main_for_handler.hide();
                        // 053-part3：close-to-hide 持久化 → 下次启动恢复。
                        window_state::save_main_visible(false);
                    }
                    _ => {}
                });
                // 启动即记一次"前台"状态（应用刚启动主窗显示时默认前台）
                proactive::update_window_focus(true);
                // 053-part3：恢复上次保存的可见性。None（首次启动 / IO 失败）
                // 走 tauri.conf.json 默认（visible: true），不动；Some(false)
                // 隐藏主窗 —— 用户上次关闭时是隐藏到 tray，启动也应保持。
                if matches!(window_state::load_main_visible(), Some(false)) {
                    let _ = main.hide();
                }
            }

            // GOAL 053：menu bar tray icon。click 切显 / 隐 main 窗口；右键
            // menu 提供「显示 / 静一会儿 30m / 静一会儿 2h / 退出」。tray icon
            // 用 app default window icon 暂占位（custom pet 头像走 part-2）。
            let show_item = MenuItem::with_id(app, "tray-show", "显示", true, None::<&str>)?;
            let mute_30 = MenuItem::with_id(
                app,
                "tray-mute-30",
                "静一会儿（30 分）",
                true,
                None::<&str>,
            )?;
            let mute_2h = MenuItem::with_id(
                app,
                "tray-mute-2h",
                "静一会儿（2 小时）",
                true,
                None::<&str>,
            )?;
            let mute_clear =
                MenuItem::with_id(app, "tray-mute-clear", "解除静默", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "tray-quit", "退出", true, None::<&str>)?;
            let tray_menu = Menu::with_items(
                app,
                &[&show_item, &mute_30, &mute_2h, &mute_clear, &quit_item],
            )?;
            let icon = app
                .default_window_icon()
                .cloned()
                .ok_or("tray icon missing")?;
            let _tray = TrayIconBuilder::with_id("main-tray")
                .tooltip("Pet")
                .icon(icon)
                .menu(&tray_menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "tray-show" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                        proactive::clear_unread_proactive(app);
                        window_state::save_main_visible(true);
                    }
                    "tray-mute-30" => {
                        let _ = proactive::set_mute_minutes(30);
                    }
                    "tray-mute-2h" => {
                        let _ = proactive::set_mute_minutes(120);
                    }
                    "tray-mute-clear" => {
                        let _ = proactive::set_mute_minutes(0);
                    }
                    "tray-quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    // 左键单击 toggle main window 显 / 隐。其它 event（右键 /
                    // double click / enter / leave）一律交给 menu。
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(w) = app.get_webview_window("main") {
                            let visible = w.is_visible().unwrap_or(false);
                            if visible {
                                let _ = w.hide();
                                window_state::save_main_visible(false);
                            } else {
                                let _ = w.show();
                                let _ = w.set_focus();
                                // 053-part2：tray 单击展开 main → 视为已读。
                                proactive::clear_unread_proactive(app);
                                window_state::save_main_visible(true);
                            }
                        }
                    }
                })
                .build(app)?;

            // Start proactive engagement loop (reads settings each tick).
            proactive::spawn(app.handle().clone());

            // Start memory consolidation loop (long-period, opt-in).
            consolidate::spawn(app.handle().clone());

            // v3 SQLite backfill：把现有 yaml butler_tasks 段一次性同步到
            // pet.db。后续启动是 noop（已存在 title 跳过）。失败不阻塞
            // 启动 —— read path 仍走 yaml，下次再试。
            db::startup_backfill_butler_tasks();
            db::startup_backfill_todos();
            db::startup_backfill_task_archive();
            db::startup_backfill_ai_insights();

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::chat::chat,
            commands::chat::chat_test,
            commands::chat::regenerate_session_title,
            commands::settings::get_settings,
            commands::settings::save_settings,
            commands::settings::get_config_raw,
            commands::settings::save_config_raw,
            commands::settings::validate_config_raw,
            commands::settings::reset_config_to_defaults,
            commands::settings::export_settings_snapshot,
            commands::settings::import_settings_snapshot,
            commands::settings::trigger_motion,
            commands::settings::get_soul,
            commands::settings::get_user_name,
            commands::settings::save_soul,
            commands::settings::reset_soul_to_default,
            commands::settings::get_pet_data_dir,
            commands::settings::open_pet_data_dir,
            commands::settings::is_current_model_multimodal,
            commands::settings::check_multimodal_model_name,
            commands::image::image_generate,
            commands::screenshot::screenshot_capture,
            visual_memory::keep_visual_memory,
            visual_memory::read_attachment,
            proactive::get_briefing_tool_fail_counts,
            memory_retrieval::retrieve_memory_cmd,
            commands::window::open_panel,
            commands::window::open_debug,
            commands::window::restart_pet_window,
            commands::window::open_devtools,
            commands::debug::get_logs,
            commands::debug::append_log,
            commands::debug::clear_logs,
            commands::debug::open_logs_dir,
            commands::debug::get_logs_dir_path,
            commands::debug::get_process_uptime_secs,
            commands::debug::get_llm_logs,
            commands::debug::get_llm_tokens_recent_secs,
            commands::debug::get_llm_calls_per_day,
            commands::debug::get_cache_stats,
            commands::debug::reset_cache_stats,
            commands::debug::get_mood_tag_stats,
            commands::debug::reset_mood_tag_stats,
            commands::debug::get_llm_outcome_stats,
            commands::debug::reset_llm_outcome_stats,
            commands::debug::get_env_tool_stats,
            commands::debug::reset_env_tool_stats,
            commands::debug::get_prompt_tilt_stats,
            commands::debug::reset_prompt_tilt_stats,
            commands::debug::get_debug_snapshot,
            tool_review::submit_tool_review,
            tool_review::list_pending_tool_reviews,
            tool_review_policy::get_tool_risk_overview,
            tool_review_policy::set_tool_review_mode,
            tool_call_history::get_recent_tool_calls,
            tool_call_history::get_top_tools_used,
            tool_call_history::get_dedicated_tool_stats,
            feedback_history::get_recent_feedback,
            feedback_history::record_bubble_dismissed,
            feedback_history::record_bubble_liked,
            feedback_history::record_bubble_puzzled,
            feedback_history::record_message_disliked,
            decision_log::get_proactive_decisions,
            decision_log::clear_proactive_decisions,
            butler_history::get_butler_history,
            butler_history::get_butler_daily_summaries,
            speech_history::get_recent_speeches,
            speech_history::get_recent_speeches_with_meta,
            speech_history::get_lifetime_speech_count,
            speech_history::get_today_speech_count,
            speech_history::get_week_speech_count,
            speech_history::get_speech_count_days,
            speech_history::get_today_speech_hourly,
            companionship::get_companionship_days,
            companionship::get_install_date,
            proactive::get_persona_summary,
            proactive::get_last_proactive_prompt,
            proactive::get_last_proactive_reply,
            proactive::get_last_proactive_meta,
            proactive::get_last_manual_fire,
            proactive::get_manual_fire_history,
            proactive::reset_proactive_stash,
            proactive::get_recent_proactive_turns,
            mood::get_current_mood,
            mood::get_mood_emoji,
            mood::get_input_placeholder,
            mood_history::get_mood_trend_hint,
            mood_history::get_mood_daily_motions,
            mood_history::get_mood_half_day_motions,
            mood_history::clear_mood_history,
            mood_history::get_mood_entries_for_date,
            proactive::get_tone_snapshot,
            proactive::set_mute_minutes,
            mute_count::get_today_mute_count,
            proactive::get_mute_until,
            proactive::set_transient_note,
            proactive::get_transient_note,
            proactive::pin_transient_note,
            proactive::get_pending_reminders,
            consolidate::trigger_consolidate,
            consolidate::cancel_consolidate,
            consolidate::get_consolidate_schedule,
            proactive::trigger_proactive_turn,
            proactive::trigger_proactive_turn_for_task,
            proactive::trigger_proactive_turn_with_prompt,
            commands::shell::check_shell_status,
            commands::shell::get_shell_exit_code_stats,
            commands::shell::reset_shell_store,
            commands::mcp::get_mcp_status,
            commands::mcp::reconnect_mcp,
            commands::session::list_sessions,
            commands::session::load_session,
            commands::session::save_session,
            commands::session::create_session,
            commands::session::get_active_session_context_stats,
            commands::session::delete_session,
            commands::session::purge_fragment_sessions,
            commands::session::search_sessions,
            commands::session::set_session_pinned,
            commands::session::list_sessions_with_images,
            commands::session::list_sessions_with_task_calls,
            commands::session::clear_all_sessions,
            commands::session::export_sessions_snapshot,
            commands::session::import_sessions_snapshot,
            commands::telegram::get_telegram_status,
            commands::telegram::reconnect_telegram,
            commands::telegram::get_tg_startup_warnings,
            commands::telegram::reset_tg_commands,
            commands::telegram::ping_tg_bot,
            commands::memory::memory_list,
            commands::memory::memory_search,
            commands::memory::memory_edit,
            commands::memory::memory_rename,
            commands::memory::memory_move_category,
            commands::memory::memory_read_detail,
            commands::memory::memory_read_detail_full,
            commands::memory::memory_reveal_detail_in_finder,
            commands::memory::memory_detail_sizes,
            commands::memory::memory_disk_usage,
            commands::memory::detail_history_disk_usage,
            commands::memory::memory_detail_history,
            commands::memory::memory_reveal_history_dir,
            commands::memory::memory_reveal_cat_dir,
            commands::memory::memory_category_churn_7d,
            commands::memory::memory_detail_abs_path,
            db::db_butler_tasks_list,
            db::get_db_stats,
            db::task_stats,
            db::task_archive_purge_older_than,
            commands::app::app_version,
            commands::app::ping_llm,
            commands::app::list_available_models,
            commands::task::task_create,
            commands::task::task_list,
            commands::task::task_retry,
            commands::task::task_mark_done,
            commands::task::task_undo_done,
            commands::task::task_skip_once,
            commands::task::task_clone,
            commands::task::task_cancel,
            commands::task::task_get_detail,
            commands::task::task_set_priority,
            commands::task::task_set_due,
            commands::task::task_set_snooze,
            commands::task::task_set_pinned,
            commands::task::task_set_silent,
            commands::task::task_set_tags,
            commands::task::task_save_detail,
            commands::task::task_detail_history,
            commands::task::task_history_sparklines,
            commands::task::task_history_24h_hourly,
            commands::task::task_reveal_history_dir,
            commands::task::task_overdue_count,
            commands::task::task_unarchive,
            commands::task::regenerate_task_title,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
