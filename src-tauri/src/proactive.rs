//! Proactive engagement engine.
//!
//! Spawns a background loop that wakes up periodically and decides whether the pet should
//! initiate a conversation with the user. Currently uses a single signal — time since the last
//! interaction — and asks the LLM whether to speak. Future iterations will add active-app
//! detection, idle-input detection, and mood state.
//!
//! Wire-up: see `lib.rs`. The engine is started once in `setup`. It writes proactive replies
//! into the active session and emits a `proactive-message` Tauri event the frontend listens for.

use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Timelike;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Mutex as TokioMutex;

use crate::commands::chat::{run_chat_pipeline, ChatMessage, CollectingSink};
use crate::commands::debug::{write_log, LogStore};
use crate::commands::session;
use crate::commands::settings::{get_settings, get_soul};
use crate::commands::shell::ShellStore;
use crate::config::AiConfig;
use crate::input_idle::user_input_idle_seconds;
use crate::mcp::McpManagerStore;
use crate::mood::{read_current_mood_parsed, read_mood_for_event, MOOD_CATEGORY, MOOD_TITLE};
use crate::tools::ToolContext;

/// Tracks interaction timing for the proactive engagement loop. Holds the last interaction
/// time, the last proactive utterance time, and whether the most recent proactive message
/// is still waiting for a user reply.
///
/// State transitions:
/// - `mark_user_message()` — user sent something. Clears `awaiting_user_reply`.
/// - `mark_proactive_spoken()` — pet spoke proactively. Sets `awaiting_user_reply = true`
///   and stamps `last_proactive`.
/// - `touch()` — any other interaction (e.g. assistant finished a reactive reply). Only
///   updates `last`; does not affect `awaiting_user_reply` or `last_proactive`.
pub struct InteractionClock {
    inner: TokioMutex<ClockInner>,
}

struct ClockInner {
    last: Instant,
    last_proactive: Option<Instant>,
    awaiting_user_reply: bool,
}

/// Snapshot of clock state used by the proactive scheduler to decide whether to fire.
pub struct ClockSnapshot {
    pub idle_seconds: u64,
    pub since_last_proactive_seconds: Option<u64>,
    pub awaiting_user_reply: bool,
}

impl InteractionClock {
    pub fn new() -> Self {
        Self {
            inner: TokioMutex::new(ClockInner {
                last: Instant::now(),
                last_proactive: None,
                awaiting_user_reply: false,
            }),
        }
    }

    pub async fn touch(&self) {
        let mut g = self.inner.lock().await;
        g.last = Instant::now();
    }

    /// Called when the user sends a message. Clears the awaiting-reply flag — once the user
    /// has spoken, we no longer consider any prior proactive message "ignored".
    pub async fn mark_user_message(&self) {
        let mut g = self.inner.lock().await;
        g.last = Instant::now();
        g.awaiting_user_reply = false;
    }

    /// Called after a proactive utterance is delivered. Sets `awaiting_user_reply = true`
    /// and records the time so cooldown checks can run.
    pub async fn mark_proactive_spoken(&self) {
        let now = Instant::now();
        let mut g = self.inner.lock().await;
        g.last = now;
        g.last_proactive = Some(now);
        g.awaiting_user_reply = true;
    }

    pub async fn snapshot(&self) -> ClockSnapshot {
        let g = self.inner.lock().await;
        ClockSnapshot {
            idle_seconds: g.last.elapsed().as_secs(),
            since_last_proactive_seconds: g.last_proactive.map(|t| t.elapsed().as_secs()),
            awaiting_user_reply: g.awaiting_user_reply,
        }
    }
}

pub type InteractionClockStore = Arc<InteractionClock>;

pub fn new_interaction_clock() -> InteractionClockStore {
    Arc::new(InteractionClock::new())
}

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
}

const SILENT_MARKER: &str = "<silent>";

/// What the proactive loop should do this tick. Each variant maps to one outer-loop branch:
/// `Silent` skips quietly, `Skip` logs the reason, `Run` triggers a real proactive turn.
/// All variants now carry a debug reason so the panel can show *why* a tick was silent
/// (disabled / quiet hours / idle short — these used to be indistinguishable in the UI).
#[derive(Debug, PartialEq, Eq)]
enum LoopAction {
    /// No log, just sleep — used when proactive is disabled or the user simply hasn't been
    /// idle long enough yet (the common case, not interesting). Static reason so the
    /// recorder can show which silent path was taken.
    Silent { reason: &'static str },
    /// Log the reason then sleep — guard fired (awaiting / cooldown / user-active).
    Skip(String),
    /// All gates passed; fire a proactive turn with these idle stats.
    Run {
        idle_seconds: u64,
        input_idle_seconds: Option<u64>,
    },
}

/// All the variable bits that go into the proactive prompt. Kept as a single struct so
/// the builder function has a clean signature and tests can inject specific values
/// without threading 9 individual arguments.
pub struct PromptInputs<'a> {
    pub time: &'a str,
    pub period: &'a str,
    pub idle_minutes: u64,
    pub input_hint: &'a str,
    pub cadence_hint: &'a str,
    pub mood_hint: &'a str,
    /// Empty string when the gate isn't applicable (no focus active, no recent wake,
    /// no prior speeches). Builder skips empty optional sections automatically.
    pub focus_hint: &'a str,
    pub wake_hint: &'a str,
    pub speech_hint: &'a str,
    /// True when the pet has never recorded a mood entry yet. Lets the rules block add a
    /// "create instead of update" hint so the model bootstraps the file correctly.
    pub is_first_mood: bool,
    /// Minutes until the configured quiet-hours window starts, when within the
    /// look-ahead window (default 15 min). None means "not approaching quiet hours" —
    /// either disabled, already inside, or more than 15 min away.
    pub pre_quiet_minutes: Option<u64>,
    /// Multi-line bullet list of user-set reminders that just came due, or empty.
    /// Scanned from the `todo` memory category every proactive turn.
    pub reminders_hint: &'a str,
    /// The pet's own short-term plan for the day (or current period). Empty when no plan
    /// has been written. Sourced from `ai_insights/daily_plan` memory; the LLM both writes
    /// and updates it. Gives the pet cross-turn intentionality so each utterance can
    /// nudge a thread forward instead of being drawn from scratch.
    pub plan_hint: &'a str,
    /// How many times the pet has ever spoken proactively (line count of
    /// speech_history.log). When small (< 3) the rules block adds an icebreaker hint so
    /// early conversations stay exploratory rather than info-dense.
    pub proactive_history_count: usize,
    /// How many times the pet has spoken proactively *today* (local time, from the
    /// speech_daily.json sidecar). When this reaches `chatty_day_threshold` the rules
    /// block adds a "go gentle today" hint so the pet doesn't keep piling on the same
    /// user the same day.
    pub today_speech_count: u64,
    /// Threshold for the "today you've already said a lot" rule. Sourced from settings
    /// so users can tune their own tolerance. 0 disables the rule entirely (a 0-threshold
    /// would otherwise fire constantly even on the first utterance, which is nonsense).
    pub chatty_day_threshold: u64,
    /// Total Spoke turns counted by `EnvToolCounters` since process start (or last reset).
    /// Together with `env_spoke_with_any` this drives the "you've been speaking without
    /// checking the environment" self-correction rule.
    pub env_spoke_total: u64,
    /// Of those, how many invoked at least one env-aware tool. When the ratio sits low
    /// past `ENV_AWARENESS_MIN_SAMPLES`, the rules block prods the model to use a tool.
    pub env_spoke_with_any: u64,
}

/// Minimum sample size before the env-awareness self-correction rule starts firing. Below
/// this we don't have enough signal to distinguish "user just got the pet talking" from
/// "the pet has been ignoring tools for a while". 10 turns is roughly half a day of
/// normal use given default cadences.
pub const ENV_AWARENESS_MIN_SAMPLES: u64 = 10;
/// Ratio threshold (numerator/100) below which the rule fires. 30 → fires when fewer
/// than 30% of recent Spoke turns consulted at least one env-aware tool. Keeps the prod
/// to the genuinely concerning floor; 50% (the panel chip's warning color) is too eager
/// for a prompt-side intervention.
pub const ENV_AWARENESS_LOW_RATE_PCT: u64 = 30;

/// The "约束" rules block of the proactive prompt — extracted into its own builder so
/// adding a rule is just `rules.push(...)` instead of squeezing a line into the middle
/// of a giant template. Rules are now context-aware: certain rules are added only when
/// the corresponding PromptInputs flag/hint is set, so the LLM gets the most useful
/// guidance for *this* moment instead of a static one-size-fits-all list.
pub fn proactive_rules(inputs: &PromptInputs) -> Vec<String> {
    let mut rules: Vec<String> = Vec::with_capacity(8);
    rules.push(format!(
        "- 如果你判断**不打扰**用户更好（比如只是想保持安静），只回复一个标记：`{}`，不要其他任何文字。",
        SILENT_MARKER
    ));
    rules.push(format!(
        "- 如果决定开口，就直接说话，不要解释自己为什么开口，也不要包含 `{}`。",
        SILENT_MARKER
    ));
    rules.push("- 只说一句话，简短自然，像伙伴一样。".into());
    rules.push("- 必要时可以调用工具：`get_active_window`（看用户在用什么 app，开口前优先调一次让话题贴合当下）、`get_upcoming_events`（看用户接下来几小时有没有日程，可用于提醒类话题，记得日程是私人内容不要原样念出）、`get_weather`（看下天气当作闲聊话题，偶尔用一次就好不要每次都查）、`memory_search`（翻一下用户偏好）。".into());
    rules.push("- 这三个环境工具（`get_active_window` / `get_weather` / `get_upcoming_events`）每次调用都有真实的 IO 成本，并且**同一次主动开口检查内重复调用同样的参数会拿到完全一样的结果**——所以一次足够了，不要为了「再确认一下」反复调，相信首次返回值直接做判断。".into());
    rules.push(format!(
        "- **决定开口后**：请用 `memory_edit` 更新 `{cat}` 类别下 `{title}` 的记忆（不存在就 `create`，存在就 `update`）。description 必须以这种格式开头：`[motion: X] 你此刻的心情和想法`，其中 X 是你想做的 Live2D 动作分组，从这四个里选一个：`Tap`（开心/活泼/兴奋）、`Flick`（想分享/有兴致/活力）、`Flick3`（焦虑/烦躁/不安）、`Idle`（平静/低落/累/沉静）。前缀后面才是自由文字。例：`[motion: Tap] 看用户在专心写代码，有点替他高兴`。沉默时无需更新。",
        cat = MOOD_CATEGORY,
        title = MOOD_TITLE,
    ));

    // ---- context-driven rules ----
    // Single source of truth: which contextual rules fire is decided by the two label
    // helpers; the match below maps each label back to its rule text. Avoids the
    // previous duplicated if-blocks (one set in `proactive_rules`, another set inside
    // each helper) — adding a new rule now means: bump the helper, add a match arm.
    let env_labels = active_environmental_rule_labels(
        !inputs.wake_hint.trim().is_empty(),
        inputs.is_first_mood,
        inputs.pre_quiet_minutes.is_some(),
        !inputs.reminders_hint.trim().is_empty(),
        !inputs.plan_hint.trim().is_empty(),
    );
    let data_labels = active_data_driven_rule_labels(
        inputs.proactive_history_count,
        inputs.today_speech_count,
        inputs.chatty_day_threshold,
        inputs.env_spoke_total,
        inputs.env_spoke_with_any,
    );
    for label in env_labels.iter().chain(data_labels.iter()) {
        let rule = match *label {
            "wake-back" => {
                "- **用户刚从离开桌子回来**：问候要简短克制，先轻打招呼或简短关心一句，不要立刻提日程/工作类信息密集的话题。".to_string()
            }
            "first-mood" => format!(
                "- **第一次开口**：你还没有写过 `{}/{}` 记忆条目，开口后应当用 `memory_edit create` 而非 `update` 来初始化它（按上面格式）。",
                MOOD_CATEGORY, MOOD_TITLE
            ),
            "pre-quiet" => format!(
                "- **快进入安静时段**：再过约 {} 分钟就到夜里的安静时段了。语气要往收尾靠——简短的晚安/睡前关心比新话题合适。",
                inputs.pre_quiet_minutes.unwrap_or(0)
            ),
            "reminders" => {
                "- **有到期的用户提醒**：上面 reminders 段列出的事项是用户之前明确让你提醒的，请把其中**最相关的一条**自然带进开口里（不要全念出来），并在开口后用 `memory_edit delete` 把已经提醒过的那条 todo 条目删掉，避免下次再提一遍。".to_string()
            }
            "plan" => {
                "- **你有今日计划在执行中**：上面 plan 段列出了你今天的小目标。开口时**优先**考虑推进其中一条（不必每次推进，看时机自然）；推进后用 `memory_edit update` 在 ai_insights/daily_plan 里更新进度（比如把 [0/2] 改成 [1/2]），全部完成的项可以删除。".to_string()
            }
            "icebreaker" => format!(
                "- **你和用户还不熟**：你之前主动开口过 {} 次（< 3 次的破冰阶段）。开口时偏向问一个简短、低压力的了解性问题（例如 ta 此刻的感受、当下在做什么、有没有最近喜欢的小事），别直接给建议或扔信息密集的话题。如果用户答了什么记得用 `memory_edit create` 写到 `user_profile` 类下方便日后用。",
                inputs.proactive_history_count
            ),
            "chatty" => format!(
                "- **今天已经聊了不少**：你今天已经主动开过 {} 次口了。除非有真正值得说的新信号（用户刚回来、有到期提醒、明显环境变化），优先**保持安静**（用 `{}`）；要说也只说极简一句，别再起新话题。",
                inputs.today_speech_count, SILENT_MARKER
            ),
            "env-awareness" => format!(
                "- **最近你开口前几乎都没看环境**：过去 {} 次主动开口里只有 {} 次调用了 `get_active_window` / `get_weather` / `get_upcoming_events` / `memory_search` 之一（< {}%）。如果决定开口，**这次先调一次 `get_active_window` 看看用户在用什么 app**，再据此说一句贴合当下的话；别凭空起话题。",
                inputs.env_spoke_total, inputs.env_spoke_with_any, ENV_AWARENESS_LOW_RATE_PCT
            ),
            // Unknown label means a helper added something proactive_rules doesn't know
            // about — log defensively rather than panic, so the prompt still ships.
            other => format!("- **[{}]**: (规则文本待补)", other),
        };
        rules.push(rule);
    }
    rules
}

/// Pure check: does the env-awareness ratio sit below the corrective threshold? Returns
/// false until at least `ENV_AWARENESS_MIN_SAMPLES` Spoke turns are recorded so we don't
/// fire on noise. Extracted for testability — the rule body uses it once.
pub fn env_awareness_low(spoke_total: u64, spoke_with_any: u64) -> bool {
    if spoke_total < ENV_AWARENESS_MIN_SAMPLES {
        return false;
    }
    // Compare spoke_with_any * 100 < ENV_AWARENESS_LOW_RATE_PCT * spoke_total instead of
    // floating-point division — exact integer arithmetic, no rounding edge cases at
    // exactly 30%.
    spoke_with_any * 100 < ENV_AWARENESS_LOW_RATE_PCT * spoke_total
}

/// Returns the labels for every *data-driven* contextual rule currently firing in the
/// proactive prompt. "Data-driven" means rules whose firing depends on counters/history
/// (icebreaker / chatty / env-awareness) — distinct from `active_environmental_rule_labels`
/// which covers state-driven rules like wake-back / first-mood / due-reminders.
///
/// Order matches the firing order in `proactive_rules` so a future "show in firing
/// sequence" tooltip stays correct.
pub fn active_data_driven_rule_labels(
    proactive_history_count: usize,
    today_speech_count: u64,
    chatty_day_threshold: u64,
    env_spoke_total: u64,
    env_spoke_with_any: u64,
) -> Vec<&'static str> {
    let mut labels: Vec<&'static str> = Vec::with_capacity(3);
    if proactive_history_count < 3 {
        labels.push("icebreaker");
    }
    if chatty_day_threshold > 0 && today_speech_count >= chatty_day_threshold {
        labels.push("chatty");
    }
    if env_awareness_low(env_spoke_total, env_spoke_with_any) {
        labels.push("env-awareness");
    }
    labels
}

/// Returns the labels for every *environmental* contextual rule currently firing —
/// rules whose firing depends on present-state signals like a recent wake-from-sleep,
/// missing mood file, approaching quiet hours, due reminders, or an in-flight daily
/// plan. Pairs with `active_data_driven_rule_labels`; both feed the panel "prompt:
/// N hints" badge and the decision-log `rules=...` tag.
///
/// Order matches the firing order in `proactive_rules`.
pub fn active_environmental_rule_labels(
    wake_back: bool,
    first_mood: bool,
    pre_quiet: bool,
    reminders_due: bool,
    has_plan: bool,
) -> Vec<&'static str> {
    let mut labels: Vec<&'static str> = Vec::with_capacity(5);
    if wake_back {
        labels.push("wake-back");
    }
    if first_mood {
        labels.push("first-mood");
    }
    if pre_quiet {
        labels.push("pre-quiet");
    }
    if reminders_due {
        labels.push("reminders");
    }
    if has_plan {
        labels.push("plan");
    }
    labels
}

/// Assemble the proactive prompt from a Vec of sections rather than a giant `format!()`.
/// Adding a new optional hint is now: (a) extend `PromptInputs`, (b) push it via
/// `push_if_nonempty`. Adding a new constraint rule is one push in `proactive_rules`.
pub fn build_proactive_prompt(inputs: &PromptInputs) -> String {
    let mut s: Vec<String> = Vec::with_capacity(20);
    s.push("[系统提示·主动开口检查]".into());
    s.push(String::new());
    s.push(format!(
        "现在是 {}（{}）。距离上次和用户互动已经过去约 {} 分钟。{}",
        inputs.time, inputs.period, inputs.idle_minutes, inputs.input_hint
    ));
    s.push(inputs.cadence_hint.to_string());
    s.push(String::new());
    s.push(inputs.mood_hint.to_string());
    push_if_nonempty(&mut s, inputs.focus_hint);
    push_if_nonempty(&mut s, inputs.wake_hint);
    push_if_nonempty(&mut s, inputs.speech_hint);
    push_if_nonempty(&mut s, inputs.reminders_hint);
    push_if_nonempty(&mut s, inputs.plan_hint);
    s.push(String::new());
    s.push(
        "请判断：作为陪伴用户的 AI 宠物，此时此刻你想主动跟用户说点什么吗？可以是关心、闲聊、提醒、分享想法都行。".into()
    );
    s.push(String::new());
    s.push("约束：".into());
    s.extend(proactive_rules(inputs));
    s.join("\n")
}

fn push_if_nonempty(sections: &mut Vec<String>, s: &str) {
    if !s.trim().is_empty() {
        sections.push(s.to_string());
    }
}

/// Format a compact "chatty mode" annotation for the decision log, e.g. `chatty=5/5`.
/// Returns `None` when the threshold is 0 (rule disabled) or when today's count is below
/// it — in those cases tagging would be noise. Pure / testable so we don't drift between
/// the gate-side push and the post-LLM push.
pub fn chatty_mode_tag(today: u64, threshold: u64) -> Option<String> {
    if threshold == 0 || today < threshold {
        None
    } else {
        Some(format!("chatty={}/{}", today, threshold))
    }
}

/// Snapshot of all the "conversational tone" signals the proactive prompt currently
/// uses. Exposed via `get_tone_snapshot` so the panel can render the same info the LLM
/// would see — handy for debugging "why did the pet say *that* right now?".
#[derive(serde::Serialize)]
pub struct ToneSnapshot {
    pub period: String,
    /// Cadence tier label, or None when this would be the pet's first proactive utterance.
    pub cadence: Option<String>,
    pub since_last_proactive_minutes: Option<u64>,
    pub wake_seconds_ago: Option<u64>,
    pub mood_text: Option<String>,
    pub mood_motion: Option<String>,
    /// Minutes until configured quiet hours kick in, when within the 15-min look-ahead.
    /// Lets the panel show "距安静时段 N 分钟" so the user can see why the pet is
    /// suddenly winding down.
    pub pre_quiet_minutes: Option<u64>,
    /// Lifetime count of proactive utterances, persisted in `speech_count.txt`.
    /// Doesn't saturate — used both by the icebreaker rule (< 3) and by the panel chip.
    pub proactive_count: u64,
    /// User-configured chatty-day threshold (`settings.proactive.chatty_day_threshold`).
    /// Surfaced so the panel can compare today's count against it and visually mark when
    /// the pet has crossed into "克制模式". 0 means the rule is disabled.
    pub chatty_day_threshold: u64,
    /// Labels for every data-driven contextual rule the proactive prompt currently has
    /// active (e.g. `["icebreaker", "chatty"]`). Empty when no data-driven rule is firing
    /// — the prompt is in its "neutral" state. Computed once on the backend so the panel
    /// doesn't need to know each rule's threshold logic.
    pub active_prompt_rules: Vec<String>,
}

#[derive(serde::Serialize)]
pub struct PendingReminder {
    pub time: String,
    pub topic: String,
    pub title: String,
    pub due_now: bool,
}

/// List every parseable reminder currently in the `todo` memory category, regardless of
/// whether it's due. Lets the panel show both "set for later" entries (helpful to verify
/// the chat actually wrote them) and "due now" entries (helpful to confirm the
/// proactive loop will surface them next tick).
#[tauri::command]
pub fn get_pending_reminders() -> Vec<PendingReminder> {
    let now = chrono::Local::now().naive_local();
    let Ok(index) = crate::commands::memory::memory_list(Some("todo".to_string())) else {
        return vec![];
    };
    let Some(cat) = index.categories.get("todo") else {
        return vec![];
    };
    let mut out = Vec::new();
    for item in &cat.items {
        if let Some((target, topic)) = parse_reminder_prefix(&item.description) {
            out.push(PendingReminder {
                time: format_target(&target),
                topic,
                title: item.title.clone(),
                due_now: is_reminder_due(&target, now, 30),
            });
        }
    }
    out
}

/// Force a proactive turn right now, bypassing the gates (awaiting / cooldown / idle /
/// quiet hours / focus / input-idle). Real values are still passed through into the
/// prompt so the LLM sees the actual idle stats. Used by panel "fire now" / demo flows
/// and for prompt iteration without waiting for natural conditions.
#[tauri::command]
pub async fn trigger_proactive_turn(app: tauri::AppHandle) -> Result<String, String> {
    let clock = app.state::<InteractionClockStore>().inner().clone();
    let snap = clock.snapshot().await;
    let input_idle = crate::input_idle::user_input_idle_seconds().await;
    let started = std::time::Instant::now();
    let outcome = run_proactive_turn(&app, snap.idle_seconds, input_idle).await?;
    let elapsed_ms = started.elapsed().as_millis();
    Ok(match outcome.reply {
        Some(text) => format!(
            "开口完成 ({} ms, idle={}s): {}",
            elapsed_ms, snap.idle_seconds, text
        ),
        None => format!(
            "宠物选择沉默 ({} ms, idle={}s)",
            elapsed_ms, snap.idle_seconds
        ),
    })
}

#[tauri::command]
pub async fn get_tone_snapshot(
    clock: tauri::State<'_, InteractionClockStore>,
    wake: tauri::State<'_, crate::wake_detector::WakeDetectorStore>,
    counters: tauri::State<'_, crate::commands::debug::ProcessCountersStore>,
) -> Result<ToneSnapshot, String> {
    let now = chrono::Local::now();
    let hour = now.hour() as u8;
    let minute = now.minute() as u8;
    let snap = clock.snapshot().await;
    let cadence_min = snap.since_last_proactive_seconds.map(|s| s / 60);
    let cadence = cadence_min.map(|m| idle_tier(m).to_string());
    let wake_ago = wake.last_wake_seconds_ago().await;
    let (mood_text, mood_motion) = match crate::mood::read_current_mood_parsed() {
        Some((t, m)) => (Some(t), m),
        None => (None, None),
    };
    let pre_quiet_minutes = get_settings().ok().and_then(|s| {
        minutes_until_quiet_start(
            hour,
            minute,
            s.proactive.quiet_hours_start,
            s.proactive.quiet_hours_end,
            15,
        )
    });
    let proactive_count = crate::speech_history::lifetime_speech_count().await;
    let chatty_day_threshold = get_settings()
        .ok()
        .map(|s| s.proactive.chatty_day_threshold)
        .unwrap_or(5);
    let today_count_for_rules = crate::speech_history::today_speech_count().await;
    let env_counters_for_rules = &counters.inner().env_tool;
    let env_total = env_counters_for_rules
        .spoke_total
        .load(std::sync::atomic::Ordering::Relaxed);
    let env_with_any = env_counters_for_rules
        .spoke_with_any
        .load(std::sync::atomic::Ordering::Relaxed);
    // Environmental rules: derive from already-fetched ToneSnapshot ingredients +
    // memory-IO probes for due reminders / active plan. Cost is one yaml read per
    // category — same as a panel `get_pending_reminders` call, which the panel was
    // already polling at 1 Hz, so this doesn't add new IO pressure.
    let wake_back = matches!(wake_ago, Some(secs) if secs <= 600);
    let first_mood = mood_text.as_ref().map(|t| t.trim().is_empty()).unwrap_or(true);
    let pre_quiet = pre_quiet_minutes.is_some();
    let reminders_due = !build_reminders_hint(now.naive_local()).is_empty();
    let has_plan = !build_plan_hint().is_empty();
    let env_labels = active_environmental_rule_labels(
        wake_back,
        first_mood,
        pre_quiet,
        reminders_due,
        has_plan,
    );
    let data_labels = active_data_driven_rule_labels(
        proactive_count as usize,
        today_count_for_rules,
        chatty_day_threshold,
        env_total,
        env_with_any,
    );
    let active_prompt_rules: Vec<String> = env_labels
        .iter()
        .chain(data_labels.iter())
        .map(|s| String::from(*s))
        .collect();
    Ok(ToneSnapshot {
        period: period_of_day(hour).to_string(),
        cadence,
        since_last_proactive_minutes: cadence_min,
        wake_seconds_ago: wake_ago,
        mood_text,
        mood_motion,
        pre_quiet_minutes,
        proactive_count,
        chatty_day_threshold,
        active_prompt_rules,
    })
}

/// Map an elapsed-minutes count (since the pet last spoke proactively) to a Chinese
/// "cadence" label. Lets the LLM shift register from "continuing a thread" through
/// "checking back in" to "haven't talked in ages" without doing the math itself.
/// Boundaries are conversational, not strict — 16 minutes is still "聊过一会儿".
pub fn idle_tier(minutes: u64) -> &'static str {
    match minutes {
        0..=15 => "刚说过话，话题还热",
        16..=60 => "聊过一会儿了",
        61..=360 => "几小时没说话",
        361..=1440 => "已经隔了大半天",
        _ => "上次聊已经是昨天或更早",
    }
}

/// Map a 24-hour clock value (0–23) to a Chinese period-of-day label. Used in the
/// proactive prompt so the LLM can riff on time-of-day vibes ("早上的咖啡时间到了") rather
/// than just seeing a numeric timestamp. Boundaries match common Chinese conversational
/// usage; the function is `pub` so tests can pin them.
pub fn period_of_day(hour: u8) -> &'static str {
    match hour {
        5..=7 => "清晨",
        8..=10 => "上午",
        11..=12 => "中午",
        13..=16 => "下午",
        17..=18 => "傍晚",
        19..=21 => "晚上",
        _ => "深夜", // 22, 23, 0..=4
    }
}

/// What kind of time a parsed reminder is targeting. `TodayHour` is the lightweight form
/// the user writes for "later today" (or "early tomorrow morning, before I sleep"); the
/// due check wraps across midnight to support that. `Absolute` is the full date-qualified
/// form the LLM should write when the user says "tomorrow 9am" / "in 2 days" — those
/// can't be expressed by HH:MM alone.
#[derive(Debug, PartialEq, Eq)]
pub enum ReminderTarget {
    TodayHour(u8, u8),
    Absolute(chrono::NaiveDateTime),
}

/// Parse a "user-set reminder" prefix from a memory item's description. Convention:
///   - `[remind: HH:MM] topic`              — today (or wraps a few minutes past midnight)
///   - `[remind: YYYY-MM-DD HH:MM] topic`   — specific moment (24-hour clock)
/// Returns `(target, topic)` when the prefix parses cleanly, `None` otherwise.
pub fn parse_reminder_prefix(desc: &str) -> Option<(ReminderTarget, String)> {
    let trimmed = desc.trim_start();
    let after_open = trimmed.strip_prefix("[remind:")?;
    let close_idx = after_open.find(']')?;
    let inside = after_open[..close_idx].trim();
    let topic = after_open[close_idx + 1..].trim().to_string();
    if topic.is_empty() {
        return None;
    }
    // Try the date-qualified form first: "YYYY-MM-DD HH:MM" — has a space inside.
    if let Some((date_str, time_str)) = inside.split_once(' ') {
        let date = chrono::NaiveDate::parse_from_str(date_str.trim(), "%Y-%m-%d").ok()?;
        let time = chrono::NaiveTime::parse_from_str(time_str.trim(), "%H:%M").ok()?;
        return Some((ReminderTarget::Absolute(date.and_time(time)), topic));
    }
    // Fall back to today-style HH:MM.
    let (hh, mm) = inside.split_once(':')?;
    let hour: u8 = hh.trim().parse().ok()?;
    let minute: u8 = mm.trim().parse().ok()?;
    if hour > 23 || minute > 59 {
        return None;
    }
    Some((ReminderTarget::TodayHour(hour, minute), topic))
}

/// Returns true if the reminder time is past `now` by no more than `window_minutes`.
/// We don't fire reminders that are still in the future or that we missed by too much.
/// `TodayHour` form additionally wraps across midnight when the gap is small enough.
pub fn is_reminder_due(
    target: &ReminderTarget,
    now: chrono::NaiveDateTime,
    window_minutes: u64,
) -> bool {
    let window = chrono::Duration::minutes(window_minutes as i64);
    let zero = chrono::Duration::zero();
    match target {
        ReminderTarget::Absolute(dt) => {
            let delta = now - *dt;
            delta >= zero && delta <= window
        }
        ReminderTarget::TodayHour(h, m) => {
            let Some(today_t) = now
                .date()
                .and_hms_opt(*h as u32, *m as u32, 0)
            else {
                return false;
            };
            let delta = now - today_t;
            if delta >= zero && delta <= window {
                return true;
            }
            // Maybe target was yesterday's HH:MM and we're early in the new day.
            let yesterday_t = today_t - chrono::Duration::days(1);
            let yd = now - yesterday_t;
            yd >= zero && yd <= window
        }
    }
}

/// How many minutes until the next quiet-hours boundary, when that boundary is within
/// `look_ahead_minutes`. Returns `None` when:
/// - quiet hours are disabled (start == end)
/// - we're already inside the quiet window (then there's nothing to "approach")
/// - the boundary is more than `look_ahead_minutes` away
///
/// Used to inject a "winding down for the night" rule into the prompt so the pet eases
/// into the quiet window with a gentler tone instead of going from full chatter to a
/// hard silent gate. Pure so tests can pin every interesting (now, start) combination.
pub fn minutes_until_quiet_start(
    now_hour: u8,
    now_minute: u8,
    quiet_start: u8,
    quiet_end: u8,
    look_ahead_minutes: u64,
) -> Option<u64> {
    if quiet_start == quiet_end {
        return None;
    }
    if in_quiet_hours(now_hour, quiet_start, quiet_end) {
        return None;
    }
    let now_total = now_hour as i32 * 60 + now_minute as i32;
    let start_total = quiet_start as i32 * 60;
    let mut delta = start_total - now_total;
    if delta < 0 {
        delta += 24 * 60; // next day's quiet_start
    }
    let delta_u = delta as u64;
    if delta_u <= look_ahead_minutes {
        Some(delta_u)
    } else {
        None
    }
}

/// Returns true if `hour` (0–23) falls inside the quiet window `[start, end)`. Handles the
/// midnight wrap-around case (start > end, e.g. 23:00–07:00). When start == end, the gate
/// is treated as disabled (no quiet hours configured).
fn in_quiet_hours(hour: u8, start: u8, end: u8) -> bool {
    if start == end {
        return false;
    }
    if start < end {
        // Same-day window, e.g. 13–15.
        hour >= start && hour < end
    } else {
        // Wraps past midnight, e.g. 23–7. In quiet if hour >= 23 OR hour < 7.
        hour >= start || hour < end
    }
}

/// Length of the wake-from-sleep grace window (seconds). Within this window after a
/// detected wake, cooldown is treated as elapsed and idle threshold is halved — so the
/// pet is more eager to greet a returning user. Awaiting / focus / quiet gates stay
/// untouched: those reflect user preference, wake doesn't override them.
const WAKE_GRACE_WINDOW_SECS: u64 = 600;

/// True when a wake event happened recently enough to soften the gates.
fn wake_recent(wake_seconds_ago: Option<u64>) -> bool {
    matches!(wake_seconds_ago, Some(s) if s <= WAKE_GRACE_WINDOW_SECS)
}

/// Pure-data gates that don't need IO. Returns `Err(action)` with the final LoopAction to
/// short-circuit the tick, or `Ok(())` signaling "all sync gates passed, caller should run
/// the input-idle gate next". Inputs:
/// - `hour`: local 24-hour clock (0–23), injected for testability
/// - `focus_active`: macOS Focus state, `None` means unknown/non-macOS (gate is no-op)
/// - `wake_seconds_ago`: how long ago the proactive loop last detected a wake-from-sleep.
///   Within `WAKE_GRACE_WINDOW_SECS` we soften cooldown + idle gates so the pet can
///   greet the returning user rather than wait out the normal nap.
fn evaluate_pre_input_idle(
    cfg: &crate::commands::settings::ProactiveConfig,
    snap: &ClockSnapshot,
    hour: u8,
    focus_active: Option<bool>,
    wake_seconds_ago: Option<u64>,
) -> Result<(), LoopAction> {
    if !cfg.enabled {
        return Err(LoopAction::Silent { reason: "disabled" });
    }
    // Gate 1: a real friend doesn't keep talking when ignored.
    if snap.awaiting_user_reply {
        return Err(LoopAction::Skip(
            "Proactive: skip — awaiting user reply to previous proactive message".into(),
        ));
    }
    // Gate 2: cooldown since the last proactive utterance, regardless of user idle.
    // Wake softens this — when the user has been away, the cooldown's "don't double up"
    // intent doesn't apply (the prior utterance was probably hours ago anyway).
    let wake_soft = wake_recent(wake_seconds_ago);
    if let Some(since) = snap.since_last_proactive_seconds {
        if !wake_soft && cfg.cooldown_seconds > 0 && since < cfg.cooldown_seconds {
            return Err(LoopAction::Skip(format!(
                "Proactive: skip — cooldown ({}s < {}s)",
                since, cfg.cooldown_seconds
            )));
        }
    }
    // Gate 3: quiet hours. A real friend lets you sleep. Silent skip rather than logged
    // skip — this can happen on every tick during night, no value in spamming logs.
    if in_quiet_hours(hour, cfg.quiet_hours_start, cfg.quiet_hours_end) {
        return Err(LoopAction::Silent { reason: "quiet_hours" });
    }
    // Gate 4: macOS Focus / DND. The user explicitly opted into "don't disturb me", so
    // skip with a logged reason (less frequent than nightly quiet hours, worth surfacing).
    if cfg.respect_focus_mode && focus_active == Some(true) {
        return Err(LoopAction::Skip(
            "Proactive: skip — macOS Focus / Do-Not-Disturb is active".into(),
        ));
    }
    // Gate 5: minimum quiet time since last interaction. Below threshold = silent skip
    // (this is the common idle-yet case, not worth logging on every tick). Wake softens
    // by halving the threshold (still floor 60s) — user just got back, "haven't been
    // idle long" doesn't really mean what it usually means.
    let raw_threshold = cfg.idle_threshold_seconds.max(60);
    let threshold = if wake_soft {
        (raw_threshold / 2).max(60)
    } else {
        raw_threshold
    };
    if snap.idle_seconds < threshold {
        return Err(LoopAction::Silent { reason: "idle_below_threshold" });
    }
    Ok(())
}

/// Gate 4: input-idle. Don't interrupt while the user is actively at the keyboard/mouse.
/// `input_idle_seconds = 0` disables the gate; `input_idle = None` (non-macOS) is treated
/// as a pass so behavior degrades to "rely on the interaction-time gate only".
fn evaluate_input_idle_gate(
    cfg: &crate::commands::settings::ProactiveConfig,
    snap: &ClockSnapshot,
    input_idle: Option<u64>,
) -> LoopAction {
    let input_ok = match (cfg.input_idle_seconds, input_idle) {
        (0, _) => true,
        (_, Some(secs)) => secs >= cfg.input_idle_seconds,
        (_, None) => true,
    };
    if !input_ok {
        return LoopAction::Skip(format!(
            "Proactive: skip — user active (input_idle={}s < {}s)",
            input_idle.unwrap_or(0),
            cfg.input_idle_seconds
        ));
    }
    LoopAction::Run {
        idle_seconds: snap.idle_seconds,
        input_idle_seconds: input_idle,
    }
}

/// Evaluate every gate in priority order and return the action this tick should take.
/// Composes the pure pre-input-idle gates with the IO call to query keyboard/mouse idle.
async fn evaluate_loop_tick(
    app: &AppHandle,
    settings: &crate::commands::settings::AppSettings,
) -> LoopAction {
    let cfg = &settings.proactive;
    let clock = app.state::<InteractionClockStore>().inner().clone();
    let snap = clock.snapshot().await;
    let hour = chrono::Local::now().hour() as u8;
    // Only fetch focus state when the gate is enabled — saves a file read every tick.
    let focus_active = if cfg.respect_focus_mode {
        crate::focus_mode::focus_mode_active().await
    } else {
        None
    };
    let wake_seconds_ago = app
        .state::<crate::wake_detector::WakeDetectorStore>()
        .inner()
        .last_wake_seconds_ago()
        .await;

    if let Err(action) =
        evaluate_pre_input_idle(cfg, &snap, hour, focus_active, wake_seconds_ago)
    {
        return action;
    }
    let input_idle = user_input_idle_seconds().await;
    evaluate_input_idle_gate(cfg, &snap, input_idle)
}

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
                    &format!("Proactive: wake-from-sleep detected (gap {}s)", gap.as_secs()),
                );
            }

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
            let chatty_threshold = settings.proactive.chatty_day_threshold;
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
            let env_label_set = active_environmental_rule_labels(
                matches!(wake_ago_for_rules, Some(secs) if secs <= 600),
                mood_for_rules
                    .as_ref()
                    .map(|(t, _)| t.trim().is_empty())
                    .unwrap_or(true),
                pre_quiet_for_rules,
                !build_reminders_hint(now_for_rules.naive_local()).is_empty(),
                !build_plan_hint().is_empty(),
            );
            let data_label_set = active_data_driven_rule_labels(
                lifetime_count as usize,
                chatty_today,
                chatty_threshold,
                env_total,
                env_with_any,
            );
            let active_labels: Vec<&'static str> = env_label_set
                .iter()
                .chain(data_label_set.iter())
                .copied()
                .collect();
            let rules_tag = if active_labels.is_empty() {
                None
            } else {
                Some(format!("rules={}", active_labels.join("+")))
            };
            // Append optional comma-separated tag onto a reason string. Centralizes the
            // ", " separator so reasons stay parseable by the panel.
            fn append_tag(reason: &mut String, tag: &str) {
                if !reason.is_empty() && reason != "-" {
                    reason.push_str(", ");
                } else if reason == "-" {
                    reason.clear();
                }
                reason.push_str(tag);
            }
            match &action {
                LoopAction::Silent { reason } => {
                    decisions.push("Silent", (*reason).to_string());
                }
                LoopAction::Skip(reason) => {
                    decisions.push("Skip", reason.clone());
                }
                LoopAction::Run { idle_seconds, input_idle_seconds } => {
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
                LoopAction::Run { idle_seconds, input_idle_seconds } => {
                    let outcome = run_proactive_turn(&app, idle_seconds, input_idle_seconds).await;
                    let chatty_part = chatty_tag.clone().unwrap_or_else(|| "-".to_string());
                    let outcome_counters = &app
                        .state::<crate::commands::debug::ProcessCountersStore>()
                        .inner()
                        .llm_outcome;
                    let env_tool_counters = &app
                        .state::<crate::commands::debug::ProcessCountersStore>()
                        .inner()
                        .env_tool;
                    match &outcome {
                        Ok(o) if o.reply.is_some() => {
                            outcome_counters.spoke.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            env_tool_counters.record_spoke(&o.tools);
                            let mut reason = chatty_part.clone();
                            if let Some(t) = &rules_tag {
                                append_tag(&mut reason, t);
                            }
                            if !o.tools.is_empty() {
                                let tools_tag = format!("tools={}", o.tools.join("+"));
                                append_tag(&mut reason, &tools_tag);
                            }
                            if reason.is_empty() {
                                reason = "-".to_string();
                            }
                            decisions.push("Spoke", reason);
                        }
                        Ok(_) => {
                            outcome_counters.silent.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            let mut reason = chatty_part.clone();
                            if let Some(t) = &rules_tag {
                                append_tag(&mut reason, t);
                            }
                            decisions.push("LlmSilent", reason);
                        }
                        Err(e) => {
                            outcome_counters.error.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            let mut tail = chatty_part.clone();
                            if let Some(t) = &rules_tag {
                                append_tag(&mut tail, t);
                            }
                            decisions.push("LlmError", format!("{} ({})", e, tail));
                        }
                    }
                    if let Err(e) = outcome {
                        eprintln!("Proactive turn failed: {}", e);
                    }
                }
            }

            tokio::time::sleep(Duration::from_secs(interval)).await;
        }
    });
}

/// Build the prompt, ask the LLM, emit the reply, and persist it. Returns the spoken
/// reply text on success — `Some(text)` when the pet actually said something, `None`
/// when it chose to stay silent. Callers can use this for status display; the spawn
/// loop discards the value.
/// What `run_proactive_turn` returns. `reply == Some` means the pet spoke; `None` means
/// it stayed silent (empty reply or `<silent>` marker). `tools` lists the unique tool
/// names the LLM called during this turn — empty when the model ignored every tool, or
/// when the turn aborted before reaching the final pipeline response.
pub struct ProactiveTurnOutcome {
    pub reply: Option<String>,
    pub tools: Vec<String>,
}

async fn run_proactive_turn(
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
    let ctx =
        ToolContext::new(log_store, shell_store, process_counters).with_tools_used_collector(tools_used.clone());

    // Try to load the latest session so the proactive turn has the recent context. If none
    // exists yet, fall back to a system-only conversation.
    let (session_id, mut messages) = load_active_session();

    let soul = get_soul().unwrap_or_default();
    let now_local = chrono::Local::now();
    let idle_minutes = idle_seconds / 60;
    let input_hint = match input_idle_seconds {
        Some(secs) => format!("用户键鼠空闲约 {} 秒。", secs),
        None => "（无法读取键鼠空闲信息。）".to_string(),
    };

    let mood_parsed = read_current_mood_parsed();
    let is_first_mood = !matches!(&mood_parsed, Some((text, _)) if !text.trim().is_empty());
    let mood_hint = match &mood_parsed {
        Some((text, _)) if !text.trim().is_empty() => {
            format!("你上次记录的心情/状态：「{}」。", text.trim())
        }
        _ => "（还没有记录过你自己的心情/状态。这是第一次。）".to_string(),
    };

    // Distance since the pet last spoke proactively — different from idle_seconds (which
    // resets on any interaction). Lets the LLM pick a register: continuation vs. casual
    // check-in vs. "haven't talked in ages".
    let cadence_hint = {
        let snap = clock.snapshot().await;
        match snap.since_last_proactive_seconds.map(|s| s / 60) {
            Some(m) => format!("距上次你主动开口约 {} 分钟（{}）。", m, idle_tier(m)),
            None => "你还没有主动开过口，这是第一次。".to_string(),
        }
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
    let speech_hint = {
        let recent = crate::speech_history::recent_speeches(5).await;
        if recent.is_empty() {
            String::new()
        } else {
            let bullets: Vec<String> = recent
                .iter()
                .map(|line| format!("· {}", crate::speech_history::strip_timestamp(line)))
                .collect();
            format!(
                "你最近主动说过的几句话（旧→新），开口前看一眼避免重复：\n{}",
                bullets.join("\n")
            )
        }
    };

    // Surface the user's active Focus mode (if any) so the pet can speak around it. This
    // path normally only runs when the user has unset `respect_focus_mode` — otherwise the
    // gate would have skipped before we got here.
    let focus_hint = match crate::focus_mode::focus_status().await {
        Some(s) if s.active => match &s.name {
            Some(n) => format!("用户当前开着 macOS Focus 模式：「{}」（说明 ta 想专注，开口要克制）。", n),
            None => "用户当前开着某个 macOS Focus 模式（说明 ta 想专注，开口要克制）。".to_string(),
        },
        _ => String::new(),
    };

    let period = period_of_day(now_local.hour() as u8);
    let time_str = now_local.format("%Y-%m-%d %H:%M").to_string();

    // Scan the `todo` memory category for user-set reminders that have just come due.
    // Each becomes a bullet line. The whole hint is empty when nothing's due.
    let reminders_hint = build_reminders_hint(now_local.naive_local());

    // Pull the pet's own short-term plan from ai_insights/daily_plan, if it has written one.
    let plan_hint = build_plan_hint();

    // Lifetime proactive utterance count — drives the icebreaker rule.
    let proactive_history_count = crate::speech_history::count_speeches().await;
    // Today's proactive count from the per-day sidecar — drives the "tone it down today"
    // rule when at or above the user-configurable threshold.
    let today_speech_count = crate::speech_history::today_speech_count().await;
    let chatty_day_threshold = get_settings()
        .ok()
        .map(|s| s.proactive.chatty_day_threshold)
        .unwrap_or(5);
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
            minutes_until_quiet_start(
                now_local.hour() as u8,
                now_local.minute() as u8,
                s.proactive.quiet_hours_start,
                s.proactive.quiet_hours_end,
                15,
            )
        })
    };
    let prompt = build_proactive_prompt(&PromptInputs {
        time: &time_str,
        period,
        idle_minutes,
        input_hint: &input_hint,
        cadence_hint: &cadence_hint,
        mood_hint: &mood_hint,
        focus_hint: &focus_hint,
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
    });

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

    let tools = tools_used.lock().map(|g| g.clone()).unwrap_or_default();

    // Treat empty / silent marker as "do nothing".
    if reply_trimmed.is_empty() || reply_trimmed.contains(SILENT_MARKER) {
        ctx.log(&format!("Proactive: silent (idle={}s)", idle_seconds));
        return Ok(ProactiveTurnOutcome { reply: None, tools });
    }

    ctx.log(&format!("Proactive: speaking ({} chars, idle={}s)", reply_trimmed.len(), idle_seconds));

    // Persist into the active session: the proactive prompt is hidden from the user, but the
    // assistant's reply is shown so the conversation context stays coherent.
    if let Some(id) = session_id {
        let _ = persist_assistant_message(&id, reply_trimmed);
    }

    clock.mark_proactive_spoken().await;
    // Append to the dedicated speech history so the next proactive turn's prompt can
    // surface this line back to the LLM and avoid repetition.
    crate::speech_history::record_speech(reply_trimmed).await;

    // Re-read mood after the turn — if the LLM updated it via memory_edit, the file has been
    // rewritten and we should ship the latest snapshot to the frontend.
    let (mood_after, motion_after) = read_mood_for_event(&ctx, "Proactive");

    let payload = ProactiveMessage {
        text: reply_trimmed.to_string(),
        timestamp: now_local.format("%Y-%m-%dT%H:%M:%S%.3f").to_string(),
        mood: mood_after,
        motion: motion_after,
    };
    let _ = app.emit("proactive-message", payload);

    Ok(ProactiveTurnOutcome {
        reply: Some(reply_trimmed.to_string()),
        tools,
    })
}

/// Scan the `todo` memory category for items whose description starts with a reminder
/// prefix and are due now (within the 30-minute window). Returns a multi-line bullet
/// hint, or empty string when nothing is due. `now` is injected so tests / call sites
/// share the same time anchor for parsed Absolute targets.
fn build_reminders_hint(now: chrono::NaiveDateTime) -> String {
    let Ok(index) = crate::commands::memory::memory_list(Some("todo".to_string())) else {
        return String::new();
    };
    let Some(cat) = index.categories.get("todo") else {
        return String::new();
    };
    let mut lines = vec!["你有以下到期的用户提醒（请挑最相关的一条带进开口）：".to_string()];
    for item in &cat.items {
        if let Some((target, topic)) = parse_reminder_prefix(&item.description) {
            if is_reminder_due(&target, now, 30) {
                let when = format_target(&target);
                lines.push(format!("· {} {}（条目标题: {}）", when, topic, item.title));
            }
        }
    }
    if lines.len() == 1 {
        String::new()
    } else {
        lines.join("\n")
    }
}

/// Read the pet's own short-term plan from `ai_insights/daily_plan`. Returns the plan
/// description verbatim with a header line, or empty when nothing's been written. The
/// plan format is intentionally open — the LLM owns the structure (bullet list with
/// progress markers like `[1/2]` is the suggested convention but not enforced).
fn build_plan_hint() -> String {
    let Ok(index) = crate::commands::memory::memory_list(Some("ai_insights".to_string())) else {
        return String::new();
    };
    let Some(cat) = index.categories.get("ai_insights") else {
        return String::new();
    };
    let plan = cat.items.iter().find(|i| i.title == "daily_plan");
    match plan {
        Some(item) if !item.description.trim().is_empty() => {
            format!("你今天的小目标 / 计划：\n{}", item.description.trim())
        }
        _ => String::new(),
    }
}

/// Format a reminder target for display in prompt / panel. TodayHour shows just the
/// HH:MM (compact, since context is "today"); Absolute spells out the full date.
pub fn format_target(target: &ReminderTarget) -> String {
    match target {
        ReminderTarget::TodayHour(h, m) => format!("{:02}:{:02}", h, m),
        ReminderTarget::Absolute(dt) => dt.format("%Y-%m-%d %H:%M").to_string(),
    }
}

/// Whether this reminder is "stale" — past its target by more than `cutoff_hours`.
/// Only `Absolute` targets can go stale; `TodayHour` is intentionally recurring-friendly
/// and doesn't carry a creation date in the memory entry, so we never auto-delete those.
/// Used by the consolidate sweep to clean up forgotten one-shot reminders.
pub fn is_stale_reminder(
    target: &ReminderTarget,
    now: chrono::NaiveDateTime,
    cutoff_hours: u64,
) -> bool {
    match target {
        ReminderTarget::Absolute(dt) => {
            let cutoff = chrono::Duration::hours(cutoff_hours as i64);
            (now - *dt) > cutoff
        }
        ReminderTarget::TodayHour(_, _) => false,
    }
}

/// Load the most recent session's messages (without the proactive prompt). Returns
/// `(session_id, messages)` or `(None, [])` if none exists yet.
fn load_active_session() -> (Option<String>, Vec<serde_json::Value>) {
    let index = session::list_sessions();
    let Some(meta) = index.sessions.last().cloned() else {
        return (None, vec![]);
    };
    match session::load_session(meta.id.clone()) {
        Ok(s) => (Some(s.id), s.messages),
        Err(_) => (None, vec![]),
    }
}

/// Append an assistant turn to the active session file so the bubble + history reflect it.
fn persist_assistant_message(session_id: &str, text: &str) -> Result<(), String> {
    let mut sess = session::load_session(session_id.to_string())?;
    sess.messages
        .push(serde_json::json!({ "role": "assistant", "content": text }));
    sess.items
        .push(serde_json::json!({ "type": "assistant", "content": text }));
    sess.updated_at = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S%.3f").to_string();
    session::save_session(sess)
}

#[cfg(test)]
mod prompt_tests {
    use super::*;

    fn base_inputs<'a>() -> PromptInputs<'a> {
        PromptInputs {
            time: "2026-05-03 14:30",
            period: "下午",
            idle_minutes: 20,
            input_hint: "用户键鼠空闲约 60 秒。",
            cadence_hint: "距上次你主动开口约 8 分钟（刚说过话，话题还热）。",
            mood_hint: "你上次记录的心情/状态：「平静」。",
            focus_hint: "",
            wake_hint: "",
            speech_hint: "",
            is_first_mood: false,
            pre_quiet_minutes: None,
            reminders_hint: "",
            plan_hint: "",
            // Default well past the icebreaker threshold so existing tests stay at the
            // base 6 rule count; icebreaker tests bump this down explicitly.
            proactive_history_count: 100,
            // Default below the chatty-day threshold so existing tests don't pick up the
            // new rule; the chatty-day tests bump this above chatty_day_threshold.
            today_speech_count: 0,
            // Mirrors the production default (5) — keeps existing tests stable while
            // letting chatty-day tests assert behavior at exact threshold boundary.
            chatty_day_threshold: 5,
            // Default 0/0 = below ENV_AWARENESS_MIN_SAMPLES → rule won't fire. Tests that
            // exercise the corrective rule bump these explicitly.
            env_spoke_total: 0,
            env_spoke_with_any: 0,
        }
    }

    #[test]
    fn prompt_includes_required_sections() {
        let p = build_proactive_prompt(&base_inputs());
        assert!(p.starts_with("[系统提示·主动开口检查]"));
        assert!(p.contains("2026-05-03 14:30"));
        assert!(p.contains("下午"));
        assert!(p.contains("20 分钟"));
        assert!(p.contains("用户键鼠空闲约 60 秒"));
        assert!(p.contains("刚说过话"));
        assert!(p.contains("「平静」"));
        assert!(p.contains("约束："));
        assert!(p.contains("[motion: Tap]"));
    }

    #[test]
    fn empty_optional_hints_skip_their_lines() {
        let p = build_proactive_prompt(&base_inputs());
        // None of the conditional hint markers should appear when their inputs are blank.
        assert!(!p.contains("Focus 模式"));
        assert!(!p.contains("从休眠唤醒"));
        assert!(!p.contains("最近主动说过的几句话"));
        // And no leading/trailing/double blank from skipped sections — the focus point
        // is that join("\n") doesn't produce stray empty lines for skipped optionals.
        assert!(!p.contains("\n\n\n"));
    }

    #[test]
    fn focus_hint_renders_when_provided() {
        let mut inputs = base_inputs();
        inputs.focus_hint = "用户当前开着 macOS Focus 模式：「work」。";
        let p = build_proactive_prompt(&inputs);
        assert!(p.contains("「work」"));
    }

    #[test]
    fn wake_hint_renders_when_provided() {
        let mut inputs = base_inputs();
        inputs.wake_hint = "（用户的电脑在大约 60 秒前刚从休眠唤醒。）";
        let p = build_proactive_prompt(&inputs);
        assert!(p.contains("从休眠唤醒"));
    }

    #[test]
    fn speech_hint_with_bullets_passes_through_verbatim() {
        let mut inputs = base_inputs();
        let bullets = "你最近主动说过的几句话（旧→新），开口前看一眼避免重复：\n· 早上好啊\n· 加油码代码";
        inputs.speech_hint = bullets;
        let p = build_proactive_prompt(&inputs);
        assert!(p.contains("早上好啊"));
        assert!(p.contains("加油码代码"));
    }

    #[test]
    fn mood_category_and_title_interpolated() {
        let p = build_proactive_prompt(&base_inputs());
        assert!(p.contains(MOOD_CATEGORY));
        assert!(p.contains(MOOD_TITLE));
    }

    // ---- proactive_rules ----

    #[test]
    fn rules_count_and_format() {
        let rules = proactive_rules(&base_inputs());
        assert_eq!(rules.len(), 6, "ladder change → update count + add a test");
        // Every rule is a bullet starting with "- ".
        for r in &rules {
            assert!(r.starts_with("- "), "rule must be a bullet: {:?}", r);
        }
    }

    #[test]
    fn rules_interpolate_constants() {
        let rules = proactive_rules(&base_inputs());
        let joined = rules.join("\n");
        assert!(joined.contains(SILENT_MARKER));
        assert!(joined.contains(MOOD_CATEGORY));
        assert!(joined.contains(MOOD_TITLE));
        // The motion-tag enumeration is part of the last rule and must remain there.
        for tag in ["Tap", "Flick", "Flick3", "Idle"] {
            assert!(joined.contains(tag), "missing motion tag: {}", tag);
        }
    }

    #[test]
    fn rules_appear_in_full_prompt() {
        let p = build_proactive_prompt(&base_inputs());
        let rules = proactive_rules(&base_inputs());
        for r in &rules {
            assert!(p.contains(r.as_str()), "rule missing from prompt: {:?}", r);
        }
    }

    // ---- context-driven rule additions ----

    #[test]
    fn wake_rule_appears_when_wake_hint_present() {
        let mut inputs = base_inputs();
        inputs.wake_hint = "（用户的电脑在大约 60 秒前刚从休眠唤醒。）";
        let rules = proactive_rules(&inputs);
        assert_eq!(rules.len(), 7, "base 6 + 1 wake-context rule");
        assert!(rules.iter().any(|r| r.contains("用户刚从离开桌子回来")));
    }

    #[test]
    fn first_mood_rule_appears_when_flagged() {
        let mut inputs = base_inputs();
        inputs.is_first_mood = true;
        let rules = proactive_rules(&inputs);
        assert_eq!(rules.len(), 7, "base 6 + 1 first-mood rule");
        assert!(rules.iter().any(|r| r.contains("memory_edit create")));
    }

    #[test]
    fn both_context_rules_can_coexist() {
        let mut inputs = base_inputs();
        inputs.wake_hint = "唤醒提示";
        inputs.is_first_mood = true;
        let rules = proactive_rules(&inputs);
        assert_eq!(rules.len(), 8, "base 6 + 2 contextual rules");
    }

    #[test]
    fn no_context_rules_with_default_inputs() {
        let rules = proactive_rules(&base_inputs());
        assert_eq!(rules.len(), 6, "no context-driven rules without their flags");
    }

    #[test]
    fn reminders_rule_appears_when_hint_present() {
        let mut inputs = base_inputs();
        let bullet_text = "你有以下到期的用户提醒（请挑最相关的一条带进开口）：\n· 23:00 吃药（条目标题: meds）";
        inputs.reminders_hint = bullet_text;
        let rules = proactive_rules(&inputs);
        assert_eq!(rules.len(), 7, "base 6 + 1 reminders rule");
        assert!(rules.iter().any(|r| r.contains("memory_edit delete")));
    }

    #[test]
    fn plan_rule_appears_when_hint_present() {
        let mut inputs = base_inputs();
        inputs.plan_hint = "你今天的小目标 / 计划：\n· 关心用户工作进展 [0/2]";
        let rules = proactive_rules(&inputs);
        assert!(rules.iter().any(|r| r.contains("你有今日计划在执行中")));
    }

    #[test]
    fn icebreaker_rule_appears_when_count_under_three() {
        let mut inputs = base_inputs();
        inputs.proactive_history_count = 0;
        let rules = proactive_rules(&inputs);
        assert!(rules.iter().any(|r| r.contains("你和用户还不熟")));
        // Must include the actual count so the LLM has a sense of where on the curve.
        assert!(rules.iter().any(|r| r.contains("0 次")));
    }

    #[test]
    fn icebreaker_rule_absent_at_threshold() {
        let mut inputs = base_inputs();
        inputs.proactive_history_count = 3;
        let rules = proactive_rules(&inputs);
        assert!(!rules.iter().any(|r| r.contains("你和用户还不熟")));
    }

    #[test]
    fn chatty_day_rule_appears_at_or_above_threshold() {
        let mut inputs = base_inputs();
        inputs.today_speech_count = inputs.chatty_day_threshold;
        let rules = proactive_rules(&inputs);
        assert!(rules.iter().any(|r| r.contains("今天已经聊了不少")));
        // The actual count must surface so the LLM knows where on the curve it sits.
        let count_str = format!("{} 次", inputs.chatty_day_threshold);
        assert!(rules.iter().any(|r| r.contains(&count_str)));
    }

    #[test]
    fn chatty_day_rule_absent_below_threshold() {
        let mut inputs = base_inputs();
        inputs.today_speech_count = inputs.chatty_day_threshold - 1;
        let rules = proactive_rules(&inputs);
        assert!(!rules.iter().any(|r| r.contains("今天已经聊了不少")));
    }

    #[test]
    fn chatty_day_rule_disabled_when_threshold_zero() {
        // threshold == 0 means user opted out of the chatty-day nudge entirely; the rule
        // must not fire even when today_speech_count is sky-high.
        let mut inputs = base_inputs();
        inputs.chatty_day_threshold = 0;
        inputs.today_speech_count = 9999;
        let rules = proactive_rules(&inputs);
        assert!(!rules.iter().any(|r| r.contains("今天已经聊了不少")));
    }

    #[test]
    fn env_awareness_low_below_min_samples_returns_false() {
        // Even 0/9 (which would be 0%) doesn't fire because we don't trust the ratio yet.
        assert!(!env_awareness_low(9, 0));
        assert!(!env_awareness_low(ENV_AWARENESS_MIN_SAMPLES - 1, 0));
    }

    #[test]
    fn env_awareness_low_at_threshold_strict_inequality() {
        // Exactly 30% (3/10) → not low; just under (2/10 = 20%) → low.
        assert!(!env_awareness_low(10, 3));
        assert!(env_awareness_low(10, 2));
    }

    #[test]
    fn env_awareness_low_at_full_coverage_returns_false() {
        // 100% coverage definitely shouldn't trigger the prod.
        assert!(!env_awareness_low(20, 20));
    }

    #[test]
    fn env_awareness_corrective_rule_appears_when_low() {
        let mut inputs = base_inputs();
        inputs.env_spoke_total = 12;
        inputs.env_spoke_with_any = 2; // ~16%
        let rules = proactive_rules(&inputs);
        assert!(rules.iter().any(|r| r.contains("最近你开口前几乎都没看环境")));
        // Includes the actual numbers + threshold so the LLM has concrete signal.
        assert!(rules.iter().any(|r| r.contains("12 次")));
        assert!(rules.iter().any(|r| r.contains("get_active_window")));
    }

    #[test]
    fn active_data_driven_rule_labels_empty_in_neutral_state() {
        // Past icebreaker, below chatty threshold, no env-awareness data.
        assert!(active_data_driven_rule_labels(100, 0, 5, 0, 0).is_empty());
    }

    #[test]
    fn active_data_driven_rule_labels_picks_up_each_rule_independently() {
        // Only icebreaker.
        assert_eq!(
            active_data_driven_rule_labels(0, 0, 5, 0, 0),
            vec!["icebreaker"],
        );
        // Only chatty.
        assert_eq!(
            active_data_driven_rule_labels(100, 5, 5, 0, 0),
            vec!["chatty"],
        );
        // Only env-awareness.
        assert_eq!(
            active_data_driven_rule_labels(100, 0, 5, 12, 2),
            vec!["env-awareness"],
        );
    }

    #[test]
    fn active_data_driven_rule_labels_combine_in_firing_order() {
        // All three at once: should appear in the same order proactive_rules pushes them.
        let labels = active_data_driven_rule_labels(0, 6, 5, 12, 1);
        assert_eq!(labels, vec!["icebreaker", "chatty", "env-awareness"]);
    }

    #[test]
    fn active_data_driven_rule_labels_zero_threshold_disables_chatty() {
        // chatty threshold == 0 means the user opted out — even today_count=99 shouldn't
        // surface the chatty label.
        assert!(active_data_driven_rule_labels(100, 99, 0, 0, 0).is_empty());
    }

    #[test]
    fn active_environmental_rule_labels_empty_when_all_false() {
        assert!(active_environmental_rule_labels(false, false, false, false, false).is_empty());
    }

    #[test]
    fn active_environmental_rule_labels_picks_each_independently() {
        assert_eq!(
            active_environmental_rule_labels(true, false, false, false, false),
            vec!["wake-back"],
        );
        assert_eq!(
            active_environmental_rule_labels(false, true, false, false, false),
            vec!["first-mood"],
        );
        assert_eq!(
            active_environmental_rule_labels(false, false, true, false, false),
            vec!["pre-quiet"],
        );
        assert_eq!(
            active_environmental_rule_labels(false, false, false, true, false),
            vec!["reminders"],
        );
        assert_eq!(
            active_environmental_rule_labels(false, false, false, false, true),
            vec!["plan"],
        );
    }

    #[test]
    fn active_environmental_rule_labels_combine_in_firing_order() {
        let labels = active_environmental_rule_labels(true, true, true, true, true);
        assert_eq!(
            labels,
            vec!["wake-back", "first-mood", "pre-quiet", "reminders", "plan"],
        );
    }

    #[test]
    fn proactive_rules_contextual_count_matches_label_count() {
        // The match-by-label refactor (Iter 87) means the number of contextual rules
        // pushed must equal env_labels.len() + data_labels.len(). If a future helper
        // returns a label proactive_rules doesn't recognize, the fallback "(规则文本待补)"
        // catches it but this test pins down the healthy path.
        let mut inputs = base_inputs();
        // Trip every contextual rule.
        inputs.wake_hint = "（用户的电脑在大约 60 秒前刚从休眠唤醒。）";
        inputs.is_first_mood = true;
        inputs.pre_quiet_minutes = Some(10);
        inputs.reminders_hint = "你有以下到期的用户提醒：\n· something";
        inputs.plan_hint = "你今天的小目标：\n· something";
        inputs.proactive_history_count = 0;
        inputs.today_speech_count = inputs.chatty_day_threshold;
        inputs.env_spoke_total = 12;
        inputs.env_spoke_with_any = 1;
        let rules = proactive_rules(&inputs);
        // 6 always-pushed base rules + 5 env labels + 3 data labels = 14.
        assert_eq!(rules.len(), 6 + 5 + 3, "rules: {:#?}", rules);
        // No "TODO" fallback for any active label.
        assert!(!rules.iter().any(|r| r.contains("规则文本待补")));
    }

    #[test]
    fn proactive_rules_baseline_only_pushes_always_on_rules() {
        // With a neutral base_inputs (no contextual triggers), only the 5 always-pushed
        // rules should appear — proves the contextual loop adds nothing when labels are empty.
        let rules = proactive_rules(&base_inputs());
        // Strip the 6 always-on rules: silent / speak / single-line / tools / cache / motion.
        // Wait — there are actually 6 always-on (counted directly from the code). Use that.
        assert_eq!(rules.len(), 6, "expected exactly 6 always-on rules in neutral state");
    }

    #[test]
    fn env_awareness_corrective_rule_absent_when_healthy() {
        let mut inputs = base_inputs();
        inputs.env_spoke_total = 12;
        inputs.env_spoke_with_any = 8; // ~67%
        let rules = proactive_rules(&inputs);
        assert!(!rules.iter().any(|r| r.contains("最近你开口前几乎都没看环境")));
    }

    #[test]
    fn chatty_mode_tag_disabled_when_threshold_zero() {
        assert_eq!(chatty_mode_tag(0, 0), None);
        assert_eq!(chatty_mode_tag(99, 0), None);
    }

    #[test]
    fn chatty_mode_tag_below_threshold_is_none() {
        assert_eq!(chatty_mode_tag(0, 5), None);
        assert_eq!(chatty_mode_tag(4, 5), None);
    }

    #[test]
    fn chatty_mode_tag_at_or_above_threshold_formats() {
        assert_eq!(chatty_mode_tag(5, 5), Some("chatty=5/5".to_string()));
        assert_eq!(chatty_mode_tag(7, 5), Some("chatty=7/5".to_string()));
    }

    #[test]
    fn chatty_day_threshold_is_user_tunable() {
        // Custom threshold of 10 should hold its boundary regardless of the const.
        let mut inputs = base_inputs();
        inputs.chatty_day_threshold = 10;
        inputs.today_speech_count = 9;
        assert!(!proactive_rules(&inputs).iter().any(|r| r.contains("今天已经聊了不少")));
        inputs.today_speech_count = 10;
        assert!(proactive_rules(&inputs).iter().any(|r| r.contains("今天已经聊了不少")));
    }

    #[test]
    fn plan_hint_appears_in_full_prompt() {
        let mut inputs = base_inputs();
        inputs.plan_hint = "你今天的小目标 / 计划：\n· 关心用户工作进展 [0/2]";
        let p = build_proactive_prompt(&inputs);
        assert!(p.contains("关心用户工作进展"));
    }

    #[test]
    fn pre_quiet_rule_appears_when_set() {
        let mut inputs = base_inputs();
        inputs.pre_quiet_minutes = Some(10);
        let rules = proactive_rules(&inputs);
        assert_eq!(rules.len(), 7);
        assert!(rules.iter().any(|r| r.contains("快进入安静时段") && r.contains("10 分钟")));
    }
}

#[cfg(test)]
mod reminder_tests {
    use super::{is_reminder_due, parse_reminder_prefix, ReminderTarget};
    use chrono::{NaiveDate, NaiveDateTime};

    fn ndt(y: i32, m: u32, d: u32, hh: u32, mm: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d).unwrap().and_hms_opt(hh, mm, 0).unwrap()
    }

    #[test]
    fn parse_today_form() {
        let (target, topic) = parse_reminder_prefix("[remind: 23:00] 吃药").unwrap();
        assert_eq!(target, ReminderTarget::TodayHour(23, 0));
        assert_eq!(topic, "吃药");
    }

    #[test]
    fn parse_absolute_form() {
        let (target, topic) =
            parse_reminder_prefix("[remind: 2026-05-04 09:00] 项目早会").unwrap();
        assert_eq!(target, ReminderTarget::Absolute(ndt(2026, 5, 4, 9, 0)));
        assert_eq!(topic, "项目早会");
    }

    #[test]
    fn parse_tolerates_extra_whitespace() {
        let (target, topic) =
            parse_reminder_prefix("  [remind:  9:30  ]   去开会  ").unwrap();
        assert_eq!(target, ReminderTarget::TodayHour(9, 30));
        assert_eq!(topic, "去开会");
    }

    #[test]
    fn parse_rejects_empty_topic() {
        assert!(parse_reminder_prefix("[remind: 12:00]").is_none());
        assert!(parse_reminder_prefix("[remind: 2026-05-04 09:00]").is_none());
    }

    #[test]
    fn parse_rejects_invalid_time() {
        assert!(parse_reminder_prefix("[remind: 25:00] hi").is_none());
        assert!(parse_reminder_prefix("[remind: 9:60] hi").is_none());
        assert!(parse_reminder_prefix("[remind: x:y] hi").is_none());
        assert!(parse_reminder_prefix("[remind: 2026-13-01 09:00] hi").is_none());
        assert!(parse_reminder_prefix("[remind: 2026-05-04 25:00] hi").is_none());
    }

    #[test]
    fn parse_no_prefix_returns_none() {
        assert!(parse_reminder_prefix("just a regular note").is_none());
        assert!(parse_reminder_prefix("[other] not a reminder").is_none());
    }

    // ---- TodayHour due semantics ----

    #[test]
    fn today_hour_within_window() {
        let target = ReminderTarget::TodayHour(12, 0);
        assert!(is_reminder_due(&target, ndt(2026, 5, 3, 12, 5), 30));
    }

    #[test]
    fn today_hour_at_exact_target() {
        let target = ReminderTarget::TodayHour(12, 0);
        assert!(is_reminder_due(&target, ndt(2026, 5, 3, 12, 0), 30));
    }

    #[test]
    fn today_hour_future_not_due() {
        let target = ReminderTarget::TodayHour(12, 0);
        assert!(!is_reminder_due(&target, ndt(2026, 5, 3, 11, 55), 30));
    }

    #[test]
    fn today_hour_too_far_past_not_due() {
        let target = ReminderTarget::TodayHour(8, 0);
        assert!(!is_reminder_due(&target, ndt(2026, 5, 3, 11, 0), 30));
    }

    #[test]
    fn today_hour_wraps_midnight() {
        // target 23:55 yesterday-relative; now 00:05 today → 10 min past → due.
        let target = ReminderTarget::TodayHour(23, 55);
        assert!(is_reminder_due(&target, ndt(2026, 5, 3, 0, 5), 30));
    }

    // ---- Absolute due semantics ----

    #[test]
    fn absolute_within_window() {
        let target = ReminderTarget::Absolute(ndt(2026, 5, 4, 9, 0));
        assert!(is_reminder_due(&target, ndt(2026, 5, 4, 9, 10), 30));
    }

    #[test]
    fn absolute_future_not_due() {
        let target = ReminderTarget::Absolute(ndt(2026, 5, 4, 9, 0));
        assert!(!is_reminder_due(&target, ndt(2026, 5, 3, 23, 0), 30));
    }

    #[test]
    fn absolute_far_past_not_due() {
        let target = ReminderTarget::Absolute(ndt(2026, 5, 1, 9, 0));
        assert!(!is_reminder_due(&target, ndt(2026, 5, 4, 9, 0), 30));
    }

    #[test]
    fn absolute_does_not_wrap_midnight() {
        // Absolute is anchored to a specific date — no wrap. 23:55 May 1 vs now 00:05 May 3
        // is over a day late, must be False.
        let target = ReminderTarget::Absolute(ndt(2026, 5, 1, 23, 55));
        assert!(!is_reminder_due(&target, ndt(2026, 5, 3, 0, 5), 30));
    }

    // ---- stale reminder ----

    #[test]
    fn absolute_stale_after_cutoff() {
        // Target was May 1 09:00; now is May 2 10:00 = 25h past, cutoff 24h → stale.
        let target = ReminderTarget::Absolute(ndt(2026, 5, 1, 9, 0));
        assert!(super::is_stale_reminder(&target, ndt(2026, 5, 2, 10, 0), 24));
    }

    #[test]
    fn absolute_within_cutoff_not_stale() {
        // Target May 1 09:00; now May 2 08:00 = 23h past → not stale at 24h cutoff.
        let target = ReminderTarget::Absolute(ndt(2026, 5, 1, 9, 0));
        assert!(!super::is_stale_reminder(&target, ndt(2026, 5, 2, 8, 0), 24));
    }

    #[test]
    fn absolute_in_future_not_stale() {
        let target = ReminderTarget::Absolute(ndt(2026, 5, 4, 9, 0));
        assert!(!super::is_stale_reminder(&target, ndt(2026, 5, 3, 12, 0), 24));
    }

    #[test]
    fn today_hour_never_stale() {
        // TodayHour is intentionally recurring-friendly — never auto-purged.
        let target = ReminderTarget::TodayHour(9, 0);
        assert!(!super::is_stale_reminder(&target, ndt(2026, 5, 3, 12, 0), 24));
    }
}

#[cfg(test)]
mod pre_quiet_tests {
    use super::minutes_until_quiet_start;

    #[test]
    fn within_window_returns_minutes() {
        // 22:50 + quiet 23..7 → 10 min until quiet, look-ahead 15 → Some(10)
        assert_eq!(minutes_until_quiet_start(22, 50, 23, 7, 15), Some(10));
    }

    #[test]
    fn at_window_edge_15_min() {
        assert_eq!(minutes_until_quiet_start(22, 45, 23, 7, 15), Some(15));
    }

    #[test]
    fn outside_window_returns_none() {
        // 16 min before — outside the 15-min look-ahead.
        assert_eq!(minutes_until_quiet_start(22, 44, 23, 7, 15), None);
    }

    #[test]
    fn already_in_quiet_returns_none() {
        // 03:00 inside 23..7 quiet window.
        assert_eq!(minutes_until_quiet_start(3, 0, 23, 7, 15), None);
        // 23:30 also inside.
        assert_eq!(minutes_until_quiet_start(23, 30, 23, 7, 15), None);
    }

    #[test]
    fn disabled_when_start_equals_end() {
        assert_eq!(minutes_until_quiet_start(22, 50, 0, 0, 15), None);
    }

    #[test]
    fn same_day_window() {
        // 13:55 + quiet 14..15 → 5 min.
        assert_eq!(minutes_until_quiet_start(13, 55, 14, 15, 15), Some(5));
    }

    #[test]
    fn past_today_uses_tomorrow() {
        // 07:00 + quiet 23..7 → not in quiet (7 is exclusive end), and quiet_start
        // tomorrow is 23:00, 16h away — way outside look-ahead.
        assert_eq!(minutes_until_quiet_start(7, 0, 23, 7, 15), None);
    }
}

#[cfg(test)]
mod cadence_tests {
    use super::idle_tier;

    #[test]
    fn each_tier_has_a_representative_minute() {
        assert_eq!(idle_tier(0), "刚说过话，话题还热");
        assert_eq!(idle_tier(8), "刚说过话，话题还热");
        assert_eq!(idle_tier(30), "聊过一会儿了");
        assert_eq!(idle_tier(120), "几小时没说话");
        assert_eq!(idle_tier(720), "已经隔了大半天");
        assert_eq!(idle_tier(2000), "上次聊已经是昨天或更早");
    }

    #[test]
    fn boundaries_land_on_expected_side() {
        assert_eq!(idle_tier(15), "刚说过话，话题还热");
        assert_eq!(idle_tier(16), "聊过一会儿了");
        assert_eq!(idle_tier(60), "聊过一会儿了");
        assert_eq!(idle_tier(61), "几小时没说话");
        assert_eq!(idle_tier(360), "几小时没说话");
        assert_eq!(idle_tier(361), "已经隔了大半天");
        assert_eq!(idle_tier(1440), "已经隔了大半天");
        assert_eq!(idle_tier(1441), "上次聊已经是昨天或更早");
    }
}

#[cfg(test)]
mod period_tests {
    use super::period_of_day;

    #[test]
    fn each_bucket_has_a_representative_hour() {
        assert_eq!(period_of_day(6), "清晨");
        assert_eq!(period_of_day(9), "上午");
        assert_eq!(period_of_day(12), "中午");
        assert_eq!(period_of_day(15), "下午");
        assert_eq!(period_of_day(18), "傍晚");
        assert_eq!(period_of_day(20), "晚上");
        assert_eq!(period_of_day(23), "深夜");
        assert_eq!(period_of_day(2), "深夜");
    }

    #[test]
    fn boundaries_land_on_expected_side() {
        // 5:00 transitions night → 清晨; 8:00 transitions 清晨 → 上午; etc.
        assert_eq!(period_of_day(4), "深夜");
        assert_eq!(period_of_day(5), "清晨");
        assert_eq!(period_of_day(7), "清晨");
        assert_eq!(period_of_day(8), "上午");
        assert_eq!(period_of_day(10), "上午");
        assert_eq!(period_of_day(11), "中午");
        assert_eq!(period_of_day(13), "下午");
        assert_eq!(period_of_day(16), "下午");
        assert_eq!(period_of_day(17), "傍晚");
        assert_eq!(period_of_day(19), "晚上");
        assert_eq!(period_of_day(21), "晚上");
        assert_eq!(period_of_day(22), "深夜");
        assert_eq!(period_of_day(0), "深夜");
    }
}

#[cfg(test)]
mod gate_tests {
    use super::*;
    use crate::commands::settings::ProactiveConfig;

    fn cfg() -> ProactiveConfig {
        ProactiveConfig {
            enabled: true,
            interval_seconds: 60,
            idle_threshold_seconds: 60,
            input_idle_seconds: 60,
            cooldown_seconds: 0, // off by default in tests so we can hit other gates
            // Quiet hours disabled by default in tests (start == end). Cases that need
            // it active set the values explicitly.
            quiet_hours_start: 0,
            quiet_hours_end: 0,
            respect_focus_mode: true,
            chatty_day_threshold: 5,
        }
    }

    fn snap(idle: u64, awaiting: bool, since_proactive: Option<u64>) -> ClockSnapshot {
        ClockSnapshot {
            idle_seconds: idle,
            awaiting_user_reply: awaiting,
            since_last_proactive_seconds: since_proactive,
        }
    }

    /// Wall clock hour known to be outside any quiet window we configure in tests.
    const NOON: u8 = 12;

    #[test]
    fn disabled_returns_silent() {
        let mut c = cfg();
        c.enabled = false;
        let action = evaluate_pre_input_idle(&c, &snap(9999, false, None), NOON, None, None);
        assert_eq!(action.unwrap_err(), LoopAction::Silent { reason: "disabled" });
    }

    #[test]
    fn awaiting_user_reply_skips_with_log() {
        let action =
            evaluate_pre_input_idle(&cfg(), &snap(9999, true, None), NOON, None, None).unwrap_err();
        match action {
            LoopAction::Skip(msg) => assert!(msg.contains("awaiting user reply")),
            other => panic!("expected Skip, got {:?}", other),
        }
    }

    #[test]
    fn cooldown_active_skips() {
        let mut c = cfg();
        c.cooldown_seconds = 1800;
        let action =
            evaluate_pre_input_idle(&c, &snap(9999, false, Some(60)), NOON, None, None).unwrap_err();
        match action {
            LoopAction::Skip(msg) => {
                assert!(msg.contains("cooldown"));
                assert!(msg.contains("60s < 1800s"));
            }
            other => panic!("expected Skip, got {:?}", other),
        }
    }

    #[test]
    fn cooldown_zero_disables_gate() {
        // Even with a recent proactive turn, cooldown=0 means "no cooldown gate".
        let mut c = cfg();
        c.cooldown_seconds = 0;
        // idle is high enough to pass the next gate, so we should reach Ok.
        let result = evaluate_pre_input_idle(&c, &snap(9999, false, Some(0)), NOON, None, None);
        assert!(result.is_ok(), "expected Ok, got {:?}", result.unwrap_err());
    }

    #[test]
    fn cooldown_elapsed_passes() {
        let mut c = cfg();
        c.cooldown_seconds = 1800;
        let result = evaluate_pre_input_idle(&c, &snap(9999, false, Some(2000)), NOON, None, None);
        assert!(result.is_ok());
    }

    #[test]
    fn idle_below_threshold_silent() {
        let c = cfg(); // threshold=60
        let action = evaluate_pre_input_idle(&c, &snap(30, false, None), NOON, None, None).unwrap_err();
        assert_eq!(action, LoopAction::Silent { reason: "idle_below_threshold" });
    }

    #[test]
    fn idle_threshold_clamped_to_60_minimum() {
        let mut c = cfg();
        c.idle_threshold_seconds = 10; // user-set absurdly low, should clamp up to 60
        let action = evaluate_pre_input_idle(&c, &snap(30, false, None), NOON, None, None).unwrap_err();
        assert_eq!(action, LoopAction::Silent { reason: "idle_below_threshold" },
            "30s should still be below the clamped 60s");
    }

    #[test]
    fn all_sync_gates_pass_returns_ok() {
        let result = evaluate_pre_input_idle(&cfg(), &snap(9999, false, None), NOON, None, None);
        assert!(result.is_ok());
    }

    // ---- in_quiet_hours pure helper ----

    #[test]
    fn quiet_hours_disabled_when_start_equals_end() {
        assert!(!in_quiet_hours(0, 0, 0));
        assert!(!in_quiet_hours(12, 23, 23));
    }

    #[test]
    fn quiet_hours_same_day_window() {
        // 13–15 quiet
        assert!(!in_quiet_hours(12, 13, 15));
        assert!(in_quiet_hours(13, 13, 15));
        assert!(in_quiet_hours(14, 13, 15));
        assert!(!in_quiet_hours(15, 13, 15), "end is exclusive");
    }

    #[test]
    fn quiet_hours_wraps_midnight() {
        // 23–7 quiet (the default)
        assert!(in_quiet_hours(23, 23, 7));
        assert!(in_quiet_hours(0, 23, 7));
        assert!(in_quiet_hours(3, 23, 7));
        assert!(in_quiet_hours(6, 23, 7));
        assert!(!in_quiet_hours(7, 23, 7));
        assert!(!in_quiet_hours(12, 23, 7));
        assert!(!in_quiet_hours(22, 23, 7));
    }

    // ---- quiet hours gate inside evaluate_pre_input_idle ----

    #[test]
    fn quiet_hours_silent_during_window() {
        let mut c = cfg();
        c.quiet_hours_start = 23;
        c.quiet_hours_end = 7;
        // 03:00 — squarely inside the night window.
        let action = evaluate_pre_input_idle(&c, &snap(9999, false, None), 3, None, None).unwrap_err();
        assert_eq!(action, LoopAction::Silent { reason: "quiet_hours" });
    }

    #[test]
    fn quiet_hours_passes_outside_window() {
        let mut c = cfg();
        c.quiet_hours_start = 23;
        c.quiet_hours_end = 7;
        let result = evaluate_pre_input_idle(&c, &snap(9999, false, None), 14, None, None);
        assert!(result.is_ok());
    }

    #[test]
    fn quiet_hours_disabled_does_not_block() {
        let c = cfg(); // both = 0 → disabled
        let result = evaluate_pre_input_idle(&c, &snap(9999, false, None), 3, None, None);
        assert!(result.is_ok(), "disabled quiet hours shouldn't gate");
    }

    // ---- focus-mode gate ----

    #[test]
    fn focus_mode_active_skips_when_respected() {
        let action = evaluate_pre_input_idle(&cfg(), &snap(9999, false, None), NOON, Some(true), None)
            .unwrap_err();
        match action {
            LoopAction::Skip(msg) => assert!(msg.contains("Focus")),
            other => panic!("expected Skip, got {:?}", other),
        }
    }

    #[test]
    fn focus_mode_active_passes_when_disabled_in_settings() {
        let mut c = cfg();
        c.respect_focus_mode = false;
        let result = evaluate_pre_input_idle(&c, &snap(9999, false, None), NOON, Some(true), None);
        assert!(result.is_ok(), "user opted out of focus respect");
    }

    #[test]
    fn focus_mode_inactive_passes() {
        let result = evaluate_pre_input_idle(&cfg(), &snap(9999, false, None), NOON, Some(false), None);
        assert!(result.is_ok());
    }

    #[test]
    fn focus_mode_unknown_passes() {
        // Non-macOS or unreadable file → None → don't block (fail open).
        let result = evaluate_pre_input_idle(&cfg(), &snap(9999, false, None), NOON, None, None);
        assert!(result.is_ok());
    }

    // ---- wake-from-sleep softening ----

    #[test]
    fn wake_recent_skips_cooldown_gate() {
        // Recent proactive utterance + non-zero cooldown would normally skip, but a fresh
        // wake makes the cooldown irrelevant.
        let mut c = cfg();
        c.cooldown_seconds = 1800;
        let result = evaluate_pre_input_idle(
            &c,
            &snap(9999, false, Some(60)),
            NOON,
            None,
            Some(120), // wake 120s ago — within grace window
        );
        assert!(result.is_ok(), "wake should soften cooldown");
    }

    #[test]
    fn wake_does_not_soften_after_grace_window() {
        let mut c = cfg();
        c.cooldown_seconds = 1800;
        let action = evaluate_pre_input_idle(
            &c,
            &snap(9999, false, Some(60)),
            NOON,
            None,
            Some(700), // > 600 grace window
        )
        .unwrap_err();
        match action {
            LoopAction::Skip(msg) => assert!(msg.contains("cooldown")),
            other => panic!("expected Skip after grace, got {:?}", other),
        }
    }

    #[test]
    fn wake_recent_halves_idle_threshold() {
        // idle_threshold defaults to 60 (clamped); halved still 60. Bump it up to see the
        // halving effect: threshold = 200 → halved 100 → idle 120 should pass.
        let mut c = cfg();
        c.idle_threshold_seconds = 200;
        let result = evaluate_pre_input_idle(
            &c,
            &snap(120, false, None),
            NOON,
            None,
            Some(60),
        );
        assert!(result.is_ok(), "wake should halve threshold so idle 120 passes 200/2 = 100");
    }

    #[test]
    fn wake_idle_floor_60s() {
        // Even with wake softening, threshold floors at 60s. idle 30 still fails.
        let mut c = cfg();
        c.idle_threshold_seconds = 100;
        let action = evaluate_pre_input_idle(
            &c,
            &snap(30, false, None),
            NOON,
            None,
            Some(60),
        )
        .unwrap_err();
        assert_eq!(action, LoopAction::Silent { reason: "idle_below_threshold" });
    }

    #[test]
    fn wake_does_not_bypass_awaiting() {
        // Even after a wake, if there's an unanswered proactive message we still wait.
        // Awaiting is about respect, not time.
        let action = evaluate_pre_input_idle(
            &cfg(),
            &snap(9999, true, None),
            NOON,
            None,
            Some(60),
        )
        .unwrap_err();
        match action {
            LoopAction::Skip(msg) => assert!(msg.contains("awaiting user reply")),
            other => panic!("expected awaiting Skip, got {:?}", other),
        }
    }

    #[test]
    fn wake_does_not_bypass_quiet_hours() {
        // Wake during 03:00 still respects quiet_hours — user explicitly opted into "let
        // me sleep at night".
        let mut c = cfg();
        c.quiet_hours_start = 23;
        c.quiet_hours_end = 7;
        let action = evaluate_pre_input_idle(
            &c,
            &snap(9999, false, None),
            3,
            None,
            Some(60),
        )
        .unwrap_err();
        assert_eq!(action, LoopAction::Silent { reason: "quiet_hours" });
    }

    // ---- input-idle gate ----

    #[test]
    fn input_idle_zero_disables_gate_runs() {
        let mut c = cfg();
        c.input_idle_seconds = 0;
        let action = evaluate_input_idle_gate(&c, &snap(9999, false, None), Some(1));
        assert!(matches!(action, LoopAction::Run { .. }));
    }

    #[test]
    fn input_idle_none_treats_as_pass() {
        // Non-macOS: user_input_idle_seconds returns None, gate should not block.
        let action = evaluate_input_idle_gate(&cfg(), &snap(9999, false, None), None);
        match action {
            LoopAction::Run { input_idle_seconds, .. } => {
                assert_eq!(input_idle_seconds, None);
            }
            other => panic!("expected Run, got {:?}", other),
        }
    }

    #[test]
    fn input_idle_below_min_skips() {
        // input_idle_min=60, observed=10 — user is actively typing.
        let action = evaluate_input_idle_gate(&cfg(), &snap(9999, false, None), Some(10));
        match action {
            LoopAction::Skip(msg) => {
                assert!(msg.contains("user active"));
                assert!(msg.contains("input_idle=10s < 60s"));
            }
            other => panic!("expected Skip, got {:?}", other),
        }
    }

    #[test]
    fn input_idle_above_min_runs() {
        let action = evaluate_input_idle_gate(&cfg(), &snap(9999, false, None), Some(120));
        match action {
            LoopAction::Run { idle_seconds, input_idle_seconds } => {
                assert_eq!(idle_seconds, 9999);
                assert_eq!(input_idle_seconds, Some(120));
            }
            other => panic!("expected Run, got {:?}", other),
        }
    }
}
