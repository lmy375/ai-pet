//! Single proactive turn: prompt build → LLM call → emit/persist.
//!
//! Pulled out of `proactive.rs` (Iter 050-15). Body is unchanged — only the
//! enclosing module moved. Sibling submodules (clock, prompt_assembler,
//! telemetry, task_hints, …) are accessed via `super::*` glob re-exports
//! still pinned at the proactive root.

use chrono::{Datelike, Timelike};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::commands::chat::{run_chat_pipeline, ChatMessage, CollectingSink};
use crate::commands::debug::LogStore;
use crate::commands::settings::{get_settings, get_soul};
use crate::commands::shell::ShellStore;
use crate::config::AiConfig;
use crate::mcp::McpManagerStore;
use crate::mood::{read_current_mood_parsed, read_mood_for_event};
use crate::tools::ToolContext;

use super::clock::InteractionClockStore;
use super::daily_review;
use super::reminder_hints::build_reminders_hint_with_proposals;
use super::session_helpers::{
    build_plan_hint, load_active_session, persist_assistant_message, read_daily_review_description,
};

#[derive(Clone, Serialize)]
pub struct ProactiveMessage {
    pub text: String,
    pub timestamp: String,
    /// Snapshot of `ai_insights/current_mood` text (motion prefix stripped) after the LLM
    /// ran. None if the LLM hasn't written one yet.
    pub mood: Option<String>,
    /// Live2D motion group the LLM picked via the `[motion: X]` prefix in its mood update.
    /// Frontend prefers this over keyword matching when present.
    pub motion: Option<String>,
    /// 早安播报附图（GOAL 003）：morning_briefing 触发时附带的可爱风格视觉
    /// 问候 data/http URL；其它 proactive 路径恒为 `None`。前端拿到后塞进
    /// in-memory map keyed by ts，**不**入磁盘 history —— 满足「不持久化
    /// 二进制」。重启后图就没了，但文本里的 `[早安图]` marker 仍在，给
    /// LLM 提供「之前发过图」的上下文。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
}

/// What `run_proactive_turn` returns. `reply == Some` means the pet spoke; `None` means
/// it stayed silent (empty reply or `<silent>` marker). `tools` lists the unique tool
/// names the LLM called during this turn — empty when the model ignored every tool, or
/// when the turn aborted before reaching the final pipeline response.
pub struct ProactiveTurnOutcome {
    pub reply: Option<String>,
    pub tools: Vec<String>,
}

/// Build the prompt, ask the LLM, emit the reply, and persist it. Returns the spoken
/// reply text on success — `Some(text)` when the pet actually said something, `None`
/// when it chose to stay silent. Callers can use this for status display; the spawn
/// loop discards the value.
pub(super) async fn run_proactive_turn(
    app: &AppHandle,
    idle_seconds: u64,
    input_idle_seconds: Option<u64>,
) -> Result<ProactiveTurnOutcome, String> {
    let config = AiConfig::from_settings()?;
    let mcp_store = app.state::<McpManagerStore>().inner().clone();
    let log_store = app.state::<LogStore>().inner().clone();
    let shell_store = app.state::<ShellStore>().inner().clone();
    let process_counters = app
        .state::<crate::commands::debug::ProcessCountersStore>()
        .inner()
        .clone();
    let clock = app.state::<InteractionClockStore>().inner().clone();

    let tools_used: std::sync::Arc<std::sync::Mutex<Vec<String>>> =
        std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    // 调试器：收 LLM 在本 turn 中的全部工具调用（name+args+result），按调用
    // 顺序。run_chat_pipeline 在 ctx.tool_calls.is_some() 时会推到这里。
    // 处理路径外的 chat 流（reactive / telegram / consolidate）不带这个
    // collector → 零开销。
    let tool_calls: std::sync::Arc<std::sync::Mutex<Vec<crate::proactive::ToolCallEntry>>> =
        std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    // Iter TR3: proactive turns also flow through the human-review gate. The
    // panel modal can resolve a parked tool call regardless of which entry
    // point initiated it.
    let tool_review = app
        .state::<crate::tool_review::ToolReviewRegistryStore>()
        .inner()
        .clone();
    // Iter R2: forward the decision_log so review outcomes show up alongside
    // proactive Spoke / Silent / Skip entries in the panel.
    let decision_log_for_ctx = app
        .state::<crate::decision_log::DecisionLogStore>()
        .inner()
        .clone();
    let ctx = ToolContext::new(log_store, shell_store, process_counters)
        .with_tools_used_collector(tools_used.clone())
        .with_tool_calls_collector(tool_calls.clone())
        .with_tool_review(tool_review)
        .with_decision_log(decision_log_for_ctx);

    // Try to load the latest session so the proactive turn has the recent context. If none
    // exists yet, fall back to a system-only conversation.
    let (session_id, mut messages) = load_active_session();

    // PanelDebug "✏️ 编辑临时 prompt" 路径在 FORCED_PROMPT_OVERRIDE 塞值，
    // 这里 take（消费式读）替代 get_soul()。take 防漏：下一次自然 tick
    // 不会复用此 override。命中时不写盘（"临时"语义）。
    let soul = match super::FORCED_PROMPT_OVERRIDE
        .lock()
        .ok()
        .and_then(|mut g| g.take())
    {
        Some(s) => s,
        None => get_soul().unwrap_or_default(),
    };
    let now_local = chrono::Local::now();
    // Iter R12: silent end-of-day review write. Idempotent per day (LAST_DAILY_REVIEW_DATE
    // + index existence check). Runs before the rest of the turn so the memory write
    // happens even if the turn is later gated to Silent — the review is its own outcome.
    daily_review::maybe_run(now_local).await;
    let idle_minutes = idle_seconds / 60;
    let input_hint = match input_idle_seconds {
        Some(secs) => format!("用户键鼠空闲约 {} 秒。", secs),
        None => "（无法读取键鼠空闲信息。）".to_string(),
    };

    let mood_parsed = read_current_mood_parsed();
    let is_first_mood = !matches!(&mood_parsed, Some((text, _)) if !text.trim().is_empty());
    let mood_hint = super::format_proactive_mood_hint(
        mood_parsed.as_ref().map(|(t, _)| t.as_str()).unwrap_or(""),
        &|s| s.to_string(),
    );

    // Distance since the pet last spoke proactively — different from idle_seconds (which
    // resets on any interaction). Lets the LLM pick a register: continuation vs. casual
    // check-in vs. "haven't talked in ages".
    let (cadence_hint, since_last_proactive_minutes) = {
        let snap = clock.snapshot().await;
        let mins = snap.since_last_proactive_seconds.map(|s| s / 60);
        let hint = match mins {
            Some(m) => format!("距上次你主动开口约 {} 分钟（{}）。", m, super::idle_tier(m)),
            None => "你还没有主动开过口，这是第一次。".to_string(),
        };
        (hint, mins)
    };

    // Iter R1: classify the previous proactive turn now that we're firing a new
    // one. raw_awaiting tells us whether the user actually replied between the
    // two: false → replied (mark_user_message cleared the flag), true → ignored.
    // Dedup via LAST_FEEDBACK_RECORDED_FOR keyed on the prior turn's timestamp.
    {
        let prev_ts = super::LAST_PROACTIVE_TIMESTAMP
            .lock()
            .ok()
            .and_then(|g| g.clone());
        let prev_reply = super::LAST_PROACTIVE_REPLY
            .lock()
            .ok()
            .and_then(|g| g.clone());
        let already_for = super::LAST_FEEDBACK_RECORDED_FOR
            .lock()
            .ok()
            .and_then(|g| g.clone());
        if let (Some(ts), Some(text)) = (prev_ts.clone(), prev_reply) {
            if Some(&ts) != already_for.as_ref() {
                let raw = clock.raw_awaiting().await;
                let kind = if raw {
                    crate::feedback_history::FeedbackKind::Ignored
                } else {
                    crate::feedback_history::FeedbackKind::Replied
                };
                crate::feedback_history::record_event(kind, text.trim()).await;
                if let Ok(mut g) = super::LAST_FEEDBACK_RECORDED_FOR.lock() {
                    *g = Some(ts);
                }
            }
        }
    }
    // Build the hint from the most recent entry (after we may have just written
    // one above). Empty when there's no history yet.
    //
    // Iter R26: widened from `recent_feedback(1)` to `recent_feedback(20)` —
    // last entry feeds `format_feedback_hint` (latest event), full window
    // feeds `format_feedback_aggregate_hint` (trend). Same window the
    // gate's R7 cooldown adapter uses, so prompt and gate agree on
    // "recent era" of feedback.
    let recent_feedback = crate::feedback_history::recent_feedback(20).await;
    // R60: pass redact closure so excerpt gets the same privacy filter
    // as other prompt hints (speech_hint / repeated_topic_hint etc).
    let feedback_hint =
        crate::feedback_history::format_feedback_hint(&recent_feedback, &|s| s.to_string());
    let feedback_aggregate_hint =
        crate::feedback_history::format_feedback_aggregate_hint(&recent_feedback);
    // Iter R33: trailing silence streak detection. Reads the ring buffer
    // (cap=5) and counts how many of the most-recent turns ended in
    // "silent". If ≥3, prompt gets a nudge to break the streak — but
    // softly ("否则继续沉默也无妨" preserves LLM judgment).
    let consecutive_silent_hint = {
        const SILENT_STREAK_THRESHOLD: usize = 3;
        let streak = super::LAST_PROACTIVE_TURNS
            .lock()
            .ok()
            .map(|g| {
                let snap: Vec<super::TurnRecord> = g.iter().cloned().collect();
                super::count_trailing_silent(&snap)
            })
            .unwrap_or(0);
        super::format_consecutive_silent_hint(streak, SILENT_STREAK_THRESHOLD)
    };
    // Iter R35: mirror on the feedback side — trailing-negative streak
    // (Ignored | Dismissed in a row). 3+ is "I'm not landing" — try a
    // different angle. Reuses the recent_feedback fetch (R26 widened
    // it to (20)).
    let consecutive_negative_hint = {
        const NEGATIVE_STREAK_THRESHOLD: usize = 3;
        let streak = crate::feedback_history::count_trailing_negative(&recent_feedback);
        crate::feedback_history::format_consecutive_negative_hint(streak, NEGATIVE_STREAK_THRESHOLD)
    };
    // Iter R55: transient instruction note. Wraps the user-provided text
    // with a clear "[临时指示]" header so LLM treats it as authoritative
    // current-state directive (vs general history hint).
    let transient_note_hint = match super::transient_note_active() {
        Some(text) => format!(
            "[临时指示] 用户当前留下的状态/指令：「{}」。这是用户主动告知 pet 的当前状态，开口时请直接尊重 / 配合，不要怀疑或追问。",
            text
        ),
        None => String::new(),
    };

    // If the proactive loop noticed a sleep gap recently (≤ 10 minutes ago), surface it
    // so the LLM can choose a "welcome back" register. Strong signal that the user was
    // physically away rather than just idle at the desk.
    let wake_hint = {
        let detector = app
            .state::<crate::wake_detector::WakeDetectorStore>()
            .inner()
            .clone();
        match detector.last_wake_seconds_ago().await {
            Some(secs) if secs <= 600 => {
                format!(
                    "（用户的电脑在大约 {} 秒前刚从休眠唤醒，看起来 ta 离开桌子一会儿后才回来。）",
                    secs
                )
            }
            _ => String::new(),
        }
    };

    // Pull the pet's recent proactive lines from a dedicated history file so the model
    // doesn't repeat itself. Independent of session messages — survives session resets
    // and chat.max_context_messages trimming.
    // Iter R11: read recent speeches once and reuse for both speech_hint
    // (the bullet list of past utterances) and repeated_topic_hint (the
    // redundancy detector). Same window so both layers see the same
    // mental model.
    let recent_speeches = crate::speech_history::recent_speeches(5).await;
    let speech_hint = if recent_speeches.is_empty() {
        String::new()
    } else {
        // Iter Cy: redact each line before re-injecting into the prompt. The pet's
        // own past utterances may have referenced private terms (the LLM doesn't
        // know to self-redact); redacting at read-time prevents re-leak even
        // though the on-disk history file stays pristine.
        let bullets: Vec<String> = recent_speeches
            .iter()
            .map(|line| {
                let stripped = crate::speech_history::strip_timestamp(line);
                format!("· {}", stripped)
            })
            .collect();
        format!(
            "你最近主动说过的几句话（旧→新），开口前看一眼避免重复：\n{}",
            bullets.join("\n")
        )
    };
    // Iter R11: 4-char window, fires when a topic appears in ≥ 3 of the
    // last 5 utterances. The bullets in `speech_hint` already redact
    // private terms; the detector runs on raw lines (after timestamp
    // strip) so it operates on actual content. Resulting hint then gets
    // its own redaction pass — defense in depth.
    let repeated_topic_hint =
        match crate::speech_history::detect_repeated_topic(&recent_speeches, 4, 3) {
            Some(topic) => format!(
                "你最近多次提到「{}」——这次开口请换个角度或换个话题，避免让用户觉得在重复。",
                topic
            ),
            None => String::new(),
        };
    // Iter R19: length-register variance nudge. Reuses the same recent_speeches
    // binding as speech_hint + repeated_topic_hint — three layers of insight
    // (bullet list / topic ngram / length distribution) from one fetch.
    let length_register_hint = crate::speech_history::format_speech_length_hint(&recent_speeches);

    let period = super::period_of_day(now_local.hour() as u8);
    let time_str = now_local.format("%Y-%m-%d %H:%M").to_string();
    // Iter Cβ: weekday/weekend label, e.g. "周日 · 周末". Joins with period in the
    // time line so the LLM can lean on "周五晚上"-flavor cues without parsing dates.
    let day_of_week = super::format_day_of_week_hint(now_local.weekday());
    // Iter Cμ: register cue derived from idle_minutes — "用户刚刚还在" vs
    // "用户至少一天没和你互动". Lets the LLM differentiate 5-min idle from 5-hour idle.
    let idle_register = super::user_absence_tier(idle_minutes);

    // Scan the `todo` memory category for user-set reminders that have just come due.
    // Each becomes a bullet line. The whole hint is empty when nothing's due.
    // GOAL 004: hint 尾巴可能挂「周期性观察」提议（≥3 天同时段同 topic 时），
    // 让 LLM 在合适时机邀请用户切换为 recur-daily。聚类读 butler_history
    // 自带的 reminder 事件流（由 build_reminders_hint 内部 dedup 后写入）。
    let reminders_hint = build_reminders_hint_with_proposals(now_local).await;

    // Pull the pet's own short-term plan from ai_insights/daily_plan, if it has written one.
    let plan_hint = build_plan_hint();
    let persona_hint = super::build_persona_hint();
    // Iter 103: read mood-trend summary from mood_history.log (window=50, min=5).
    // Window is generous because mood is deduped against the last entry, so 50 lines
    // typically span 1-2 weeks of distinct mood changes. min=5 avoids early-day noise.
    let mood_trend_hint = crate::mood_history::build_trend_hint(50, 5).await;
    // Iter Cα: surface user_profile memory as ambient context so the LLM sees
    // basic user habits without firing memory_search every turn. Empty until
    // the pet has written at least one user_profile entry.
    let user_profile_hint = super::build_user_profile_hint();
    // Iter Cγ: surface owner-assigned butler tasks each proactive turn so the
    // pet's task queue stays visible — the pet shouldn't forget the user asked
    // it to "每天早上发日历" between turns. Iter Cζ adds schedule-awareness
    // (`[every: HH:MM]` / `[once: ...]` prefixes); `now` is passed so due tasks
    // bubble to the top with a "⏰ 到期" marker.
    let mut butler_tasks_hint = super::build_butler_tasks_hint(now_local.naive_local());
    // Panel "▶️ 现在跑一次"：take（消费式读）已塞入的 forced focus title。
    // 命中时把一条强势指令拼到 butler_tasks_hint 最前 —— LLM 看到队列前
    // 还有一条"用户在面板上点击了 X 的现在跑一次，请把本轮重心放在这条
    // 任务上"，自然不会偏。take 防止该 forced focus 在下一轮自然 tick
    // 时再次生效，与命令侧 defer-clear 互为兜底。
    if let Ok(mut g) = super::FORCED_TASK_FOCUS.lock() {
        if let Some(forced_title) = g.take() {
            let line = format!(
                "⚡ 用户在面板上点击了「{}」的『现在跑一次』按钮，请把本轮重心放在这条任务上：要么推进它、要么写一句进展、要么标 done / error。如该任务不在下面的队列里说明已被归档 / 重命名，回一句简短确认即可。\n",
                forced_title,
            );
            butler_tasks_hint = format!("{}{}", line, butler_tasks_hint);
        }
    }
    // 长任务心跳：把"被动过手却停滞过久"的 pending 任务点名出来，让 LLM
    // 这一轮要么写一句进展、要么改 done / error。读 settings 决定阈值
    // (0 = 关闭)，IO 层在 build_task_heartbeat_hint 里已做空串短路。
    let task_heartbeat_hint = {
        let threshold = get_settings()
            .map(|s| s.proactive.task_heartbeat_minutes)
            .unwrap_or(0);
        super::build_task_heartbeat_hint(now_local.naive_local(), threshold)
    };
    // 「刚完成」点名：和心跳互补——心跳让 LLM 推进卡住的任务，本 hint
    // 让 LLM 在用户那边收尾确认。每次 tick 自维护 LAST_SEEN_BUTLER_DONE_TITLES
    // 静态实现"只 fire 一次"语义。
    let task_completion_hint = super::build_task_completion_hint();
    let recent_completion_hint = super::build_recent_completion_hint(now_local.naive_local());

    // Iter R77: pull butler_tasks with `[deadline:]` prefix and format the
    // urgency-aware hint. Reads same memory category as butler_tasks_hint
    // but filters to deadline-prefixed items only — pet reminds user about
    // the deadline rather than auto-executing. Empty when no Approaching /
    // Imminent / Overdue deadlines.
    let deadline_hint = super::build_butler_deadlines_hint(now_local.naive_local());
    // Iter Cυ: owner-name from settings, empty when unset.
    let user_name = get_settings().map(|s| s.user_name).unwrap_or_default();
    // GOAL 055: pet's own name from settings, empty when unset.
    let pet_name = get_settings().map(|s| s.pet_name).unwrap_or_default();

    // Lifetime proactive utterance count — drives the icebreaker rule.
    let proactive_history_count = crate::speech_history::count_speeches().await;
    // Days since the pet was first installed — drives the long-term persona register
    // (Iter 101). On first ever proactive turn this also writes install_date.txt.
    let companionship_days = crate::companionship::companionship_days().await;
    // Today's proactive count from the per-day sidecar — drives the "tone it down today"
    // rule when at or above the user-configurable threshold.
    let today_speech_count = crate::speech_history::today_speech_count().await;
    let chatty_day_threshold = get_settings()
        .ok()
        .map(|s| s.proactive.effective_chatty_threshold())
        .unwrap_or(5);
    // Iter R14: at the first proactive turn of a new day, surface yesterday's
    // last 2 utterances so the pet can pick up a thread instead of starting
    // cold every morning. Empty when (a) not first-of-day or (b) yesterday
    // has no recorded speeches. Each line is timestamp-stripped + redacted.
    //
    // Iter R16: also pull yesterday's `daily_review_YYYY-MM-DD` description as
    // a separate "总览" hint — pairs with the speeches as high-level + specific
    // two-layer recap. Same first-of-day gate.
    let yesterday = now_local.date_naive() - chrono::Duration::days(1);
    let cross_day_hint = if today_speech_count == 0 {
        let lines = crate::speech_history::speeches_for_date_async(yesterday, 2).await;
        if lines.is_empty() {
            String::new()
        } else {
            let bullets: Vec<String> = lines
                .iter()
                .map(|line| {
                    let stripped = crate::speech_history::strip_timestamp(line);
                    format!("· {}", stripped)
                })
                .collect();
            format!(
                "[昨日尾声] 昨天最后说过：\n{}\n如果话题自然能续上就续，不必生硬呼应。",
                bullets.join("\n")
            )
        }
    } else {
        String::new()
    };
    let yesterday_recap_hint = if today_speech_count == 0 {
        let desc = read_daily_review_description(yesterday);
        super::format_yesterday_recap_hint(desc.as_deref())
    } else {
        String::new()
    };
    // Env-awareness ratio over the recent window (process-wide atomic, reset by panel).
    // Drives a self-correction rule: when the model's been ignoring env tools, nudge it
    // to call get_active_window this turn.
    let env_counters = &app
        .state::<crate::commands::debug::ProcessCountersStore>()
        .inner()
        .env_tool;
    let env_spoke_total = env_counters
        .spoke_total
        .load(std::sync::atomic::Ordering::Relaxed);
    let env_spoke_with_any = env_counters
        .spoke_with_any
        .load(std::sync::atomic::Ordering::Relaxed);

    let pre_quiet_minutes = {
        let settings = get_settings().ok();
        settings.and_then(|s| {
            super::minutes_until_quiet_start(
                now_local.hour() as u8,
                now_local.minute() as u8,
                s.proactive.quiet_hours_start,
                s.proactive.quiet_hours_end,
                15,
            )
        })
    };
    let prompt = super::build_proactive_prompt(&super::PromptInputs {
        time: &time_str,
        period,
        day_of_week: &day_of_week,
        idle_minutes,
        idle_register,
        input_hint: &input_hint,
        cadence_hint: &cadence_hint,
        mood_hint: &mood_hint,
        wake_hint: &wake_hint,
        speech_hint: &speech_hint,
        is_first_mood,
        pre_quiet_minutes,
        reminders_hint: &reminders_hint,
        plan_hint: &plan_hint,
        proactive_history_count,
        today_speech_count,
        chatty_day_threshold,
        env_spoke_total,
        env_spoke_with_any,
        since_last_proactive_minutes,
        companionship_days,
        persona_hint: &persona_hint,
        mood_trend_hint: &mood_trend_hint,
        user_profile_hint: &user_profile_hint,
        butler_tasks_hint: &butler_tasks_hint,
        task_heartbeat_hint: &task_heartbeat_hint,
        task_completion_hint: &task_completion_hint,
        recent_completion_hint: &recent_completion_hint,
        user_name: &user_name,
        pet_name: &pet_name,
        feedback_hint: &feedback_hint,
        feedback_aggregate_hint: &feedback_aggregate_hint,
        consecutive_silent_hint: &consecutive_silent_hint,
        consecutive_negative_hint: &consecutive_negative_hint,
        transient_note_hint: &transient_note_hint,
        hour: now_local.hour() as u8,
        recently_fired_wellness: super::late_night_wellness_in_cooldown(),
        repeated_topic_hint: &repeated_topic_hint,
        cross_day_hint: &cross_day_hint,
        yesterday_recap_hint: &yesterday_recap_hint,
        length_register_hint: &length_register_hint,
        deadline_hint: &deadline_hint,
    });
    // Iter E1: stash the prompt so the panel can show "what did the LLM see this
    // turn?" — useful for prompt tuning without instrumenting log scraping.
    if let Ok(mut g) = super::LAST_PROACTIVE_PROMPT.lock() {
        *g = Some(prompt.clone());
    }
    // Iter E3: also stash the timestamp at prompt-build time. Set here (not at
    // reply time) so the user sees "when was this turn started" — closer to
    // when the displayed signals (mood/cadence/etc.) were sampled.
    if let Ok(mut g) = super::LAST_PROACTIVE_TIMESTAMP.lock() {
        *g = Some(now_local.format("%Y-%m-%d %H:%M:%S").to_string());
    }

    // Ensure system message anchors the conversation; build a temporary message list.
    if messages.is_empty() {
        messages.push(serde_json::json!({ "role": "system", "content": soul }));
    }
    messages.push(serde_json::json!({ "role": "user", "content": prompt }));

    let chat_messages: Vec<ChatMessage> = messages
        .iter()
        .filter_map(|v| serde_json::from_value(v.clone()).ok())
        .collect();

    let sink = CollectingSink::new();
    let reply = run_chat_pipeline(chat_messages, &sink, &config, &mcp_store, &ctx).await?;
    let reply_trimmed = reply.trim();
    // Iter E2: stash the raw reply so the panel modal can pair it with the
    // prompt — full request/response loop in one view.
    if let Ok(mut g) = super::LAST_PROACTIVE_REPLY.lock() {
        *g = Some(reply.clone());
    }

    let tools = tools_used.lock().map(|g| g.clone()).unwrap_or_default();
    // Iter E3: stash the distinct tool names alongside the prompt/reply pair.
    // Deduped via BTreeSet then collected so panel UI doesn't have to handle
    // duplicates from the registry's per-call list.
    let tools_dedup: Vec<String> = {
        let mut seen = std::collections::BTreeSet::new();
        for t in &tools {
            seen.insert(t.clone());
        }
        seen.into_iter().collect()
    };
    if let Ok(mut g) = super::LAST_PROACTIVE_TOOLS.lock() {
        *g = tools_dedup.clone();
    }
    // 调试器：拿出本 turn 累积的完整 tool 调用记录（name+args+result，按
    // LLM 调用顺序），随 TurnRecord 一起进 ring buffer 给 modal 展示。
    let tool_calls_collected = tool_calls.lock().map(|g| g.clone()).unwrap_or_default();
    // Iter E4: also append the full turn record to the ring buffer so the
    // panel can navigate prev/next across the last N turns. Cap at
    // PROACTIVE_TURN_HISTORY_CAP via pop_front.
    if let Ok(mut g) = super::LAST_PROACTIVE_TURNS.lock() {
        let ts = super::LAST_PROACTIVE_TIMESTAMP
            .lock()
            .ok()
            .and_then(|t| t.clone())
            .unwrap_or_default();
        // Iter R25: classify outcome inline. Same condition the silent-marker
        // check below uses — kept in sync so TurnRecord.outcome doesn't drift
        // from the actual return path.
        let outcome = if super::is_silent_reply(reply_trimmed) {
            "silent"
        } else {
            "spoke"
        };
        g.push_back(super::TurnRecord {
            timestamp: ts,
            prompt: prompt.clone(),
            reply: reply.clone(),
            tools_used: tools_dedup,
            tool_calls: tool_calls_collected,
            outcome: outcome.to_string(),
        });
        while g.len() > super::PROACTIVE_TURN_HISTORY_CAP {
            g.pop_front();
        }
    }

    // Treat empty / silent marker (any case) as "do nothing". 064-part1：用
    // is_silent_reply 替代裸 contains —— 拦截 `<Silent>` / 含推理 marker。
    if super::is_silent_reply(reply_trimmed) {
        ctx.log(&format!("Proactive: silent (idle={}s)", idle_seconds));
        return Ok(ProactiveTurnOutcome { reply: None, tools });
    }

    ctx.log(&format!(
        "Proactive: speaking ({} chars, idle={}s)",
        reply_trimmed.len(),
        idle_seconds
    ));

    // Persist into the active session: the proactive prompt is hidden from the user, but the
    // assistant's reply is shown so the conversation context stays coherent.
    if let Some(id) = session_id {
        let _ = persist_assistant_message(&id, reply_trimmed);
    }

    clock.mark_proactive_spoken().await;
    // Iter #389: record speech + per-speech 触发 meta（band / factor / mode
    // / deadline_factor）让 PanelDebug ⏰ chip "为何开口" 半边能读到上
    // 下文。compute_record_meta 复用 build_cooldown_breakdown 同算法 —
    // 与 ToneStrip 当前态 chip 一致。
    super::record_speech_with_current_meta(reply_trimmed).await;

    // Re-read mood after the turn — if the LLM updated it via memory_edit, the file has been
    // rewritten and we should ship the latest snapshot to the frontend.
    let (mood_after, motion_after) = read_mood_for_event(&ctx, "Proactive");
    // Iter 103: append the post-turn mood to mood_history.log (best-effort, deduped
    // against last entry inside record_mood). Captures the trajectory of the pet's
    // emotional register over time so future proactive turns can reflect on it.
    if let Some(text) = &mood_after {
        crate::mood_history::record_mood(text, &motion_after).await;
    }

    let payload = ProactiveMessage {
        text: reply_trimmed.to_string(),
        timestamp: now_local.format("%Y-%m-%dT%H:%M:%S%.3f").to_string(),
        mood: mood_after,
        motion: motion_after,
        // 常规 proactive 路径不附图（GOAL 003 只覆盖 morning_briefing）。
        image_url: None,
    };
    let _ = app.emit("proactive-message", payload);
    // 053-part2：tray tooltip 显未读计数（main 窗口隐藏时才 bump）。
    super::unread_tray::record_emitted(app);

    // GOAL 052：pet 不在前台 ≥ 30s 时同步走 OS notification 通道。
    // emit 后立即 gate-check + 异步 send，避免阻塞返回主路径。
    {
        let state = super::WINDOW_FOREGROUND_STATE.lock().ok().and_then(|g| *g);
        if super::should_send_os_notification(
            state,
            std::time::Instant::now(),
            super::NOTIFICATION_FOREGROUND_THRESHOLD_SECS,
        ) {
            let body =
                super::format_notification_body(reply_trimmed, super::NOTIFICATION_BODY_CHAR_CAP);
            // pet name 来源：当前 AppSettings 无独立 pet_name 字段（角色由
            // SOUL.md 自由文本承载），通知 title 走兜底 "Pet"。spec 提到
            // 「PanelPersona 提供」未来加 pet_name setting 时一行替换。
            let pet_name = String::new();
            let app_for_notif = app.clone();
            tauri::async_runtime::spawn(async move {
                super::send_os_notification(
                    &app_for_notif,
                    &pet_name,
                    &body,
                    super::ProactiveNotificationKind::Other,
                )
                .await;
            });
        }
    }

    Ok(ProactiveTurnOutcome {
        reply: Some(reply_trimmed.to_string()),
        tools,
    })
}
