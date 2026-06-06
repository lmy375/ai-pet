//! Background engagement loop. Reads settings each tick, runs scheduled
//! `morning_briefing` / `welcome_back` triggers, then routes to
//! `run_proactive_turn` via the gate. Decision-log + dispatch metrics are
//! stamped before the LLM call so silent / skip / error paths stay visible.

use std::time::Duration;

use chrono::Timelike;
use tauri::{AppHandle, Manager};

use crate::commands::debug::{write_log, LogStore};
use crate::commands::settings::get_settings;

use super::clock::InteractionClockStore;
use super::gate::{evaluate_loop_tick, LoopAction};
use super::prompt_rules::{
    active_composite_rule_labels, active_data_driven_rule_labels, active_environmental_rule_labels,
    late_night_wellness_in_cooldown, mark_late_night_wellness_fired,
};
use super::reminder_hints::build_reminders_hint;
use super::session_helpers::build_plan_hint;
use super::telemetry::{chatty_mode_tag, record_proactive_outcome};
use super::time_helpers::minutes_until_quiet_start;
use super::{morning_briefing, welcome_back};

/// Spawn the background engagement loop. Reads settings on every tick so changes take effect
/// without a restart. Honors `proactive.enabled`; sleeps a short fallback interval when disabled.
pub fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        // A short startup delay so we don't fire before the UI is ready.
        tokio::time::sleep(Duration::from_secs(20)).await;

        loop {
            let settings = match get_settings() {
                Ok(s) => s,
                Err(_) => {
                    tokio::time::sleep(Duration::from_secs(60)).await;
                    continue;
                }
            };
            let interval = settings.proactive.interval_seconds.max(60);

            // Heartbeat — if the gap since the last iteration is unexpectedly large the
            // process was likely suspended (laptop closed / system sleep). The detector
            // remembers the wake timestamp; run_proactive_turn looks it up to inject a
            // "welcome back" hint into the prompt.
            let wake_detector = app
                .state::<crate::wake_detector::WakeDetectorStore>()
                .inner()
                .clone();
            if let Some(gap) = wake_detector.observe().await {
                let log_store = app.state::<LogStore>().inner().clone();
                write_log(
                    &log_store.0,
                    &format!(
                        "Proactive: wake-from-sleep detected (gap {}s)",
                        gap.as_secs()
                    ),
                );
            }

            // 早安简报：与常规 proactive 节奏并列，但绕过 cooldown / chatty
            // 等数值化门控（自带"每日 1 次"语义）。先于 evaluate_loop_tick 跑，
            // 这样如果今天还没说早安，门控时刻一到就立刻触发；触发后通过
            // mark_proactive_spoken 让本 tick 之后的常规 cooldown 重新生效。
            let _ = morning_briefing::maybe_run(&app, &settings, chrono::Local::now()).await;

            // Welcome-back（GOAL 008）：用户从 ≥30min idle 回到桌前的瞬间
            // 打招呼。自带 per-idle-session dedup + 2h 全局冷却。放在
            // morning_briefing / memory_follow_up 之后 —— 若它们刚 emit
            // 过，clock.since_last_proactive_seconds 让本触发自动让位。
            let _ = welcome_back::maybe_run(&app, chrono::Local::now()).await;

            let action = evaluate_loop_tick(&app, &settings).await;
            // Record before dispatching so even paths that immediately sleep are visible
            // in the panel — the entire point of this log is "why didn't anything happen".
            let decisions = app
                .state::<crate::decision_log::DecisionLogStore>()
                .inner()
                .clone();
            // Pull soft-rule context once so we can tag both the gate decision and the
            // post-LLM outcome with the same numbers — keeps the decision log explainable
            // when the pet stays silent because of a prompt-level rule rather than a gate.
            let chatty_today = crate::speech_history::today_speech_count().await;
            let chatty_threshold = settings.proactive.effective_chatty_threshold();
            let chatty_tag = chatty_mode_tag(chatty_today, chatty_threshold);
            // Snapshot the data-driven prompt rules that *will* fire this turn (icebreaker /
            // chatty / env-awareness). Computed at dispatch time so every decision-log
            // entry — Silent / Skip / Run / Spoke / LlmSilent / LlmError — gets stamped
            // with the same rule set the LLM saw, enabling event-by-event audit later.
            let lifetime_count = crate::speech_history::lifetime_speech_count().await;
            let env_counters = &app
                .state::<crate::commands::debug::ProcessCountersStore>()
                .inner()
                .env_tool;
            let env_total = env_counters
                .spoke_total
                .load(std::sync::atomic::Ordering::Relaxed);
            let env_with_any = env_counters
                .spoke_with_any
                .load(std::sync::atomic::Ordering::Relaxed);
            // Combine environmental + data-driven labels so the decision-log rules tag
            // matches the panel "prompt: N hints" badge — same composition order as
            // ToneSnapshot.active_prompt_rules.
            let now_for_rules = chrono::Local::now();
            let mood_for_rules = crate::mood::read_current_mood_parsed();
            let wake_ago_for_rules = app
                .state::<crate::wake_detector::WakeDetectorStore>()
                .inner()
                .last_wake_seconds_ago()
                .await;
            let pre_quiet_for_rules = minutes_until_quiet_start(
                now_for_rules.hour() as u8,
                now_for_rules.minute() as u8,
                settings.proactive.quiet_hours_start,
                settings.proactive.quiet_hours_end,
                15,
            )
            .is_some();
            let wake_back_for_rules = matches!(wake_ago_for_rules, Some(secs) if secs <= 600);
            let has_plan_for_rules = !build_plan_hint().is_empty();
            let env_label_set = active_environmental_rule_labels(
                wake_back_for_rules,
                mood_for_rules
                    .as_ref()
                    .map(|(t, _)| t.trim().is_empty())
                    .unwrap_or(true),
                pre_quiet_for_rules,
                !build_reminders_hint(now_for_rules.naive_local()).is_empty(),
                has_plan_for_rules,
                chatty_today == 0,
            );
            let companionship_days_for_rules = crate::companionship::companionship_days().await;
            let data_label_set = active_data_driven_rule_labels(
                lifetime_count as usize,
                chatty_today,
                chatty_threshold,
                env_total,
                env_with_any,
                companionship_days_for_rules,
            );
            // Pull cadence (minutes since last proactive) for the long-idle composite,
            // and idle-minutes (user-side absence) for the long-absence-reunion rule.
            // Same source as run_proactive_turn's cadence_hint / idle_register.
            let snap_for_rules = app
                .state::<InteractionClockStore>()
                .inner()
                .snapshot()
                .await;
            let since_last_for_rules = snap_for_rules.since_last_proactive_seconds.map(|s| s / 60);
            let idle_min_for_rules: u64 = snap_for_rules.idle_seconds / 60;
            let composite_label_set = active_composite_rule_labels(
                wake_back_for_rules,
                has_plan_for_rules,
                since_last_for_rules,
                chatty_today,
                chatty_threshold,
                pre_quiet_for_rules,
                idle_min_for_rules,
                now_for_rules.hour() as u8,
                late_night_wellness_in_cooldown(),
            );
            let active_labels: Vec<&'static str> = env_label_set
                .iter()
                .chain(data_label_set.iter())
                .chain(composite_label_set.iter())
                .copied()
                .collect();
            // Iter R8: stamp the wellness static at dispatch time when the rule
            // appears in this turn's active set. Doing it here (loop wrapper)
            // rather than after the LLM response means even if the model stays
            // silent, we still consume the 30-min window — preventing a near-
            // edge thrash where we'd reactivate the rule on the next tick.
            if active_labels.contains(&"late-night-wellness") {
                mark_late_night_wellness_fired();
            }
            let rules_tag = if active_labels.is_empty() {
                None
            } else {
                Some(format!("rules={}", active_labels.join("+")))
            };
            match &action {
                LoopAction::Silent { reason } => {
                    decisions.push("Silent", (*reason).to_string());
                }
                LoopAction::Skip(reason) => {
                    decisions.push("Skip", reason.clone());
                }
                LoopAction::Run {
                    idle_seconds,
                    input_idle_seconds,
                } => {
                    let mut reason = format!(
                        "idle={}s, input_idle={}",
                        idle_seconds,
                        input_idle_seconds
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| "?".to_string()),
                    );
                    if let Some(t) = &chatty_tag {
                        reason.push_str(", ");
                        reason.push_str(t);
                    }
                    if let Some(t) = &rules_tag {
                        reason.push_str(", ");
                        reason.push_str(t);
                    }
                    decisions.push("Run", reason);
                }
            }

            match action {
                LoopAction::Silent { .. } => {}
                LoopAction::Skip(reason) => {
                    let log_store = app.state::<LogStore>().inner().clone();
                    write_log(&log_store.0, &reason);
                }
                LoopAction::Run {
                    idle_seconds,
                    input_idle_seconds,
                } => {
                    // Record long-running prompt tilt (Iter 96): bump exactly one of four
                    // buckets based on the active label set we computed for this Run. Done
                    // here rather than after the LLM call so the count tracks "Run with
                    // these rules dispatched" — the prompt was sent regardless of outcome.
                    app.state::<crate::commands::debug::ProcessCountersStore>()
                        .inner()
                        .prompt_tilt
                        .record_dispatch(&active_labels);
                    let outcome =
                        super::run_proactive_turn(&app, idle_seconds, input_idle_seconds).await;
                    let chatty_part = chatty_tag.clone().unwrap_or_else(|| "-".to_string());
                    let counters_for_outcome = app
                        .state::<crate::commands::debug::ProcessCountersStore>()
                        .inner()
                        .clone();
                    record_proactive_outcome(
                        &counters_for_outcome,
                        &decisions,
                        "loop",
                        &chatty_part,
                        rules_tag.as_deref(),
                        &outcome,
                    );
                    if let Err(e) = outcome {
                        eprintln!("Proactive turn failed: {}", e);
                    }
                }
            }

            tokio::time::sleep(Duration::from_secs(interval)).await;
        }
    });
}
