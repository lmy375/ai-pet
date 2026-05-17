mod butler_history;
mod commands;
mod companionship;
mod config;
mod consolidate;
mod db;
mod decision_log;
mod feedback_history;
mod input_idle;
mod log_rotation;
mod mcp;
mod mood;
mod mood_history;
mod mute_count;
mod proactive;
mod speech_history;
mod task_heartbeat;
mod task_queue;
mod telegram;
mod tool_call_history;
mod tool_review;
mod tool_review_policy;
mod tool_risk;
mod tools;
mod wake_detector;

use commands::debug::{log_dir, new_process_counters, LogStore};
use commands::shell::ShellStore;
use std::collections::HashMap;
use std::sync::Arc;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Ensure log directory exists
    let _ = std::fs::create_dir_all(log_dir());

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
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
            commands::window::open_panel,
            commands::window::open_debug,
            commands::window::restart_pet_window,
            commands::window::open_devtools,
            commands::debug::get_logs,
            commands::debug::append_log,
            commands::debug::clear_logs,
            commands::debug::open_logs_dir,
            commands::debug::get_llm_logs,
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
            proactive::get_pending_reminders,
            consolidate::trigger_consolidate,
            consolidate::cancel_consolidate,
            proactive::trigger_proactive_turn,
            proactive::trigger_proactive_turn_for_task,
            proactive::trigger_proactive_turn_with_prompt,
            commands::shell::check_shell_status,
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
            commands::memory::memory_category_churn_7d,
            commands::memory::memory_detail_abs_path,
            db::db_butler_tasks_list,
            db::get_db_stats,
            db::task_stats,
            db::task_archive_purge_older_than,
            commands::app::app_version,
            commands::app::ping_llm,
            commands::task::task_create,
            commands::task::task_list,
            commands::task::task_retry,
            commands::task::task_mark_done,
            commands::task::task_undo_done,
            commands::task::task_skip_once,
            commands::task::task_cancel,
            commands::task::task_get_detail,
            commands::task::task_set_priority,
            commands::task::task_set_due,
            commands::task::task_set_snooze,
            commands::task::task_set_pinned,
            commands::task::task_set_silent,
            commands::task::task_set_tags,
            commands::task::task_save_detail,
            commands::task::task_overdue_count,
            commands::task::task_unarchive,
            commands::task::regenerate_task_title,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
