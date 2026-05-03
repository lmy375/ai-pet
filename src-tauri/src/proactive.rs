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

use chrono::{Datelike, Timelike};
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
    /// Combined weekday + weekend/weekday label, e.g. "周日 · 周末" or "周二 · 工作日".
    /// Iter Cβ — appended to the time line so the LLM can pick a register that fits
    /// "周五晚上" vs "周一上午" instead of inferring it from the date string. Built
    /// from `format_day_of_week_hint(now.weekday())` at the callsite.
    pub day_of_week: &'a str,
    pub idle_minutes: u64,
    /// Iter Cμ: human-readable cue for how long the user has been away. Distinct
    /// from `idle_tier` (pet-side cadence) and from the raw `idle_minutes` number —
    /// gives the LLM language to register "用户走开有一两小时了" vs "用户刚刚还在"
    /// without doing arithmetic itself. Built from `user_absence_tier(idle_minutes)`.
    pub idle_register: &'a str,
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
    /// Minutes since the pet's last proactive utterance, or `None` when it has never
    /// spoken proactively this session. Numeric form (alongside the textual
    /// `cadence_hint`) so composite rules can compare against thresholds without
    /// re-parsing the hint string.
    pub since_last_proactive_minutes: Option<u64>,
    /// Days since the pet was first installed (Iter 101). 0 on install day. Lets the
    /// LLM modulate language: a freshly-met pet on day 0 should sound less familiar
    /// than one that's been around for a year. Persisted in `install_date.txt`.
    pub companionship_days: u64,
    /// Self-authored persona summary the pet's own consolidate loop has written about
    /// itself (Iter 102). Empty until the first consolidate produces enough signal.
    /// When non-empty, surfaces a "what I've noticed about my own voice and how I
    /// interact with this user" header so each proactive turn can lean on the
    /// reflection rather than starting from a static SOUL.md alone.
    pub persona_hint: &'a str,
    /// Long-term mood-trend hint from `mood_history.log` (Iter 103). Format like
    /// "你最近 N 次心情记录里：Tap × 12、Idle × 8、Flick × 3"; empty when there's
    /// not enough recorded mood history to summarize. Sits next to `persona_hint`
    /// so the LLM gets both "how I see myself" + "how I've been feeling lately".
    pub mood_trend_hint: &'a str,
    /// Compact digest of the `user_profile` memory category — what the pet has
    /// learned about the user's habits/preferences (Iter Cα). Empty when the
    /// category has no entries. Surfaces ambient context so the LLM doesn't
    /// have to fire a `memory_search` tool call to know basic things it has
    /// already written down. Already redacted via `redact_with_settings`.
    pub user_profile_hint: &'a str,
    /// Owner-assigned task queue (Iter Cγ). Reads from the `butler_tasks` memory
    /// category. Surfaced every proactive turn so the pet remembers what the user
    /// has asked it to do — info gathering, scheduled reports, recurring chores.
    /// Empty until the first task is created. This is the seed of the 宠物管家
    /// direction: the pet's "to-do FOR you" list, distinct from `reminders_hint`
    /// (the user's own due nudges).
    pub butler_tasks_hint: &'a str,
    /// Iter Cυ: owner's display name from settings.user_name. Empty when not set —
    /// builder skips the line and the LLM keeps using 「你」. Non-empty: a short
    /// "你的主人是「X」" line is pushed near the top of the prompt so the LLM
    /// can occasionally call the user by name. Mirrors the persona_layer
    /// injection added in Iter Cτ but lives in proactive's own prompt path.
    pub user_name: &'a str,
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
/// Minutes-since-last-proactive threshold for the `long-idle-no-restraint` composite
/// rule. 60 = "you haven't spoken for an hour" — distinct from the existing cadence
/// tiers used in `cadence_hint`, which top out at "haven't talked in ages" without a
/// numeric anchor. None (= never spoken) is treated as long-idle so the rule helps
/// fresh sessions where 0 prior speech is the same problem as silent for an hour.
pub const LONG_IDLE_MINUTES: u64 = 60;

/// Iter Cν: idle_minutes (since *user* last interacted) threshold for the
/// `long-absence-reunion` composite rule. 240 = 4 hours — distinct from the
/// system-sleep-driven `wake-back` (which fires on a discrete sleep wake event).
/// Long absence covers cases where the laptop stayed on but the user was gone:
/// out for lunch / in a meeting / asleep on a desktop / etc.
pub const LONG_ABSENCE_MINUTES: u64 = 240;

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
    if !inputs.butler_tasks_hint.trim().is_empty() {
        rules.push(
            "- **你也是用户的小管家**：上面「管家任务」段列出了用户委托给你的事，你可以用 \
`read_file` / `write_file` / `edit_file` / `bash` 真去执行（读 ta 的某个文件、写一份日报、\
整理目录都行），完成后用 `memory_edit update` 在 `butler_tasks` 里记录这次执行时间和结果。\
一次开口推进一项就够，不必一次清空。如果当下不是合适执行时机，也可以只做轻提及。"
                .into(),
        );
    }
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
        inputs.today_speech_count == 0,
    );
    let data_labels = active_data_driven_rule_labels(
        inputs.proactive_history_count,
        inputs.today_speech_count,
        inputs.chatty_day_threshold,
        inputs.env_spoke_total,
        inputs.env_spoke_with_any,
        inputs.companionship_days,
    );
    let composite_labels = active_composite_rule_labels(
        !inputs.wake_hint.trim().is_empty(),
        !inputs.plan_hint.trim().is_empty(),
        inputs.since_last_proactive_minutes,
        inputs.today_speech_count,
        inputs.chatty_day_threshold,
        inputs.pre_quiet_minutes.is_some(),
        inputs.idle_minutes,
    );
    for label in env_labels
        .iter()
        .chain(data_labels.iter())
        .chain(composite_labels.iter())
    {
        let rule = match *label {
            "wake-back" => {
                "- **用户刚从离开桌子回来**：问候要简短克制，先轻打招呼或简短关心一句，不要立刻提日程/工作类信息密集的话题。".to_string()
            }
            "first-mood" => format!(
                "- **第一次开口**：你还没有写过 `{}/{}` 记忆条目，开口后应当用 `memory_edit create` 而非 `update` 来初始化它（按上面格式）。",
                MOOD_CATEGORY, MOOD_TITLE
            ),
            "first-of-day" => {
                "- **今天的第一次开口**：今天还没主动开过口。如果决定开口，请用当下时段对应的问候打底（清晨/上午→「早」「早安」；中午/下午→「下午好」「忙不忙」；傍晚/晚上→「晚上好」「今天怎么样」；深夜→简短关心或不打扰）。一句暖场就够，再决定要不要带话题。和 `wake-back`（系统刚唤醒）/ `long-absence-reunion`（用户长别）正交——这只关乎日界节奏。".to_string()
            }
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
            "companionship-milestone" => {
                let label = companionship_milestone(inputs.companionship_days).unwrap_or("纪念日");
                format!(
                    "- **今天是和用户相处的「{}」**（共 {} 天）。如果决定开口，可以轻轻提一句这种相处的感受——不是郑重宣告，更像顺口提一下「啊，今天好像满 X 了」；不要要求 ta 回应这个话题，就当作语气底色。如果其它高优先级信号（到期任务/重要提醒）也在，让那个先说，纪念日只做个底色。",
                    label, inputs.companionship_days
                )
            }
            "env-awareness" => format!(
                "- **最近你开口前几乎都没看环境**：过去 {} 次主动开口里只有 {} 次调用了 `get_active_window` / `get_weather` / `get_upcoming_events` / `memory_search` 之一（< {}%）。如果决定开口，**这次先调一次 `get_active_window` 看看用户在用什么 app**，再据此说一句贴合当下的话；别凭空起话题。",
                inputs.env_spoke_total, inputs.env_spoke_with_any, ENV_AWARENESS_LOW_RATE_PCT
            ),
            "engagement-window" => {
                "- **此刻是开新话题的好时机**：用户刚从离开桌子回来 + 你今天有 plan 在执行——是把「先简短关心 ta 一下，再点一下 plan 进度」自然串起来的复合时机。一句话里带一句关心 + 一句和 plan 相关的，避免硬切话题，也别只问候不带行动。".to_string()
            }
            "long-idle-no-restraint" => format!(
                "- **沉默已久但没在克制状态**：距上次你主动开口已经 ≥ {} 分钟（或一直没说过），今天聊得也不多，又不在安静时段——是开口找个新话题的安全窗口。建议先 `get_active_window` 看看用户在做什么，再据此抛一个和 ta 当下场景相关的轻话题（不是问候、不是问感受，是真的「看到 ta 在做 X 想到 Y」）。",
                LONG_IDLE_MINUTES
            ),
            "long-absence-reunion" => format!(
                "- **用户离开了不短的时间**：约 {} 分钟没和你互动了（≥ {} 分钟阈值）。和 `wake-back`（系统刚唤醒）不同，这是用户那一侧的久别——开口要带「重逢感」：先简短关心一句、问一句轻松的归来话题（如「刚回来呀」「下午顺利吗」），不要立刻抛日程/工作类信息密集内容；语气比 wake-back 近一档但别热络过头。",
                inputs.idle_minutes, LONG_ABSENCE_MINUTES
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
    companionship_days: u64,
) -> Vec<&'static str> {
    let mut labels: Vec<&'static str> = Vec::with_capacity(4);
    if proactive_history_count < 3 {
        labels.push("icebreaker");
    }
    if chatty_day_threshold > 0 && today_speech_count >= chatty_day_threshold {
        labels.push("chatty");
    }
    if companionship_milestone(companionship_days).is_some() {
        labels.push("companionship-milestone");
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
    first_of_day: bool,
) -> Vec<&'static str> {
    let mut labels: Vec<&'static str> = Vec::with_capacity(6);
    if wake_back {
        labels.push("wake-back");
    }
    if first_mood {
        labels.push("first-mood");
    }
    if first_of_day {
        labels.push("first-of-day");
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

/// Returns labels for *composite* rules — those that fire only when multiple individual
/// signals coincide. Most existing rules are restraints ("be quiet because X"); the
/// composite group makes room for *positive* prompts ("right now is a good moment to
/// open up because X+Y") that the singletons can't express. Members:
///
/// - `engagement-window`: user just came back to the desk AND the pet has an in-flight
///   daily plan — a natural moment to weave concern + plan progress into one line.
/// - `long-idle-no-restraint`: it's been ≥ `LONG_IDLE_MINUTES` since the last proactive
///   AND the pet hasn't been chatty today AND we're not approaching quiet hours — a
///   safe window to surface a fresh topic instead of letting the silence drag on.
pub fn active_composite_rule_labels(
    wake_back: bool,
    has_plan: bool,
    since_last_proactive_minutes: Option<u64>,
    today_speech_count: u64,
    chatty_day_threshold: u64,
    pre_quiet: bool,
    idle_minutes: u64,
) -> Vec<&'static str> {
    let mut labels: Vec<&'static str> = Vec::with_capacity(3);
    if wake_back && has_plan {
        labels.push("engagement-window");
    }
    let long_idle = match since_last_proactive_minutes {
        Some(m) => m >= LONG_IDLE_MINUTES,
        // Never-spoken is treated as long-idle: no recent speech to defer to.
        None => true,
    };
    let under_chatty = chatty_day_threshold == 0 || today_speech_count < chatty_day_threshold;
    if long_idle && under_chatty && !pre_quiet {
        labels.push("long-idle-no-restraint");
    }
    // Iter Cν: long-absence-reunion fires when the user themselves has been away
    // ≥ LONG_ABSENCE_MINUTES, regardless of pet-side cadence. Gates on
    // under_chatty (don't pile on if today's already chatty) and !pre_quiet
    // (don't add an opener register right before quiet hours kick in).
    if idle_minutes >= LONG_ABSENCE_MINUTES && under_chatty && !pre_quiet {
        labels.push("long-absence-reunion");
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
        "现在是 {}（{}，{}）。距离上次和用户互动已经过去约 {} 分钟（{}）。{}",
        inputs.time,
        inputs.period,
        inputs.day_of_week,
        inputs.idle_minutes,
        inputs.idle_register,
        inputs.input_hint
    ));
    s.push(inputs.cadence_hint.to_string());
    s.push(String::new());
    s.push(inputs.mood_hint.to_string());
    s.push(format_companionship_line(inputs.companionship_days));
    // Iter Cυ: optional owner-name line. Reuses the same wording as
    // format_persona_layer (Iter Cτ) so reactive chat and proactive give
    // consistent "你的主人是「X」" framing.
    if !inputs.user_name.trim().is_empty() {
        s.push(format!(
            "你的主人是「{}」——开口时可以用这个称呼或「你」自然交替，不必每句都喊名字。",
            inputs.user_name.trim()
        ));
    }
    push_if_nonempty(&mut s, inputs.persona_hint);
    push_if_nonempty(&mut s, inputs.mood_trend_hint);
    push_if_nonempty(&mut s, inputs.user_profile_hint);
    push_if_nonempty(&mut s, inputs.focus_hint);
    push_if_nonempty(&mut s, inputs.wake_hint);
    push_if_nonempty(&mut s, inputs.speech_hint);
    push_if_nonempty(&mut s, inputs.reminders_hint);
    push_if_nonempty(&mut s, inputs.plan_hint);
    push_if_nonempty(&mut s, inputs.butler_tasks_hint);
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

/// Render the "companionship duration" section of the proactive prompt. Day 0 gets a
/// "this is the first day" framing — encouraging the LLM to use a getting-acquainted
/// register — while N >= 1 just states the count, letting the LLM choose whether and
/// how to draw on the accumulated familiarity. Pure / testable.
pub fn format_companionship_line(days: u64) -> String {
    if days == 0 {
        "你和用户今天才正式认识，是你陪伴 ta 的第一天——语气可以保留一点点初识的客气感。".to_string()
    } else {
        format!(
            "你和用户已经一起走过 {} 天——可以让这份相处时长自然渗进语气，比如对 ta 偏好的预判、共同回忆的暗指（不必硬塞，时机对就用）。",
            days
        )
    }
}

/// Iter Cρ: pure helper — return a Chinese milestone label if `days` is a relationship
/// milestone, else None. Called by both the rule body (which formats the label into
/// the prompt) and the data-driven label helper (which pushes the rule label when
/// non-None). Milestones: 7, 30, 100, 180, 365 fixed; every 365 thereafter is
/// "又一个周年". Returns None on day 0 (already covered by the always-rendered
/// companionship_line's "第一天" framing).
pub fn companionship_milestone(days: u64) -> Option<&'static str> {
    match days {
        7 => Some("刚好一周"),
        30 => Some("满一个月"),
        100 => Some("百日纪念"),
        180 => Some("满半年"),
        365 => Some("满一年"),
        d if d > 365 && d % 365 == 0 => Some("又一个周年"),
        _ => None,
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
    /// Iter D1: weekday + weekend/weekday combined label, e.g. "周二 · 工作日".
    /// Same value the proactive prompt's time line uses (Cβ).
    pub day_of_week: String,
    /// Iter D1: human-readable user-absence cue, e.g. "用户离开了一小会儿". Same value
    /// the proactive prompt's time line uses (Cμ). Surfaced so the panel can render
    /// the register the LLM is currently reading.
    pub idle_register: String,
    /// Iter D1: minutes since last user interaction. Pairs with `idle_register` —
    /// register is the human cue, this is the precise number for tooltip / debug.
    pub idle_minutes: u64,
    /// Iter D2: companionship milestone label when today is one (Cρ — 7 / 30 /
    /// 100 / 180 / 365 / yearly). None otherwise. Surfaced so the panel can show
    /// a celebration cue on the same days the proactive prompt's milestone rule
    /// fires.
    pub companionship_milestone: Option<String>,
    /// Iter D2: companionship days (lifetime count). Already in PanelStatsCard
    /// via a separate Tauri command, but bundling it here lets the strip render
    /// the milestone cue without a second IPC.
    pub companionship_days: u64,
    /// Iter D3: macOS Focus mode label when active, None otherwise. Same signal
    /// the proactive engine reads via `focus_mode::focus_status` to decide
    /// whether to gate. Surfaced so the panel can show 🎯 「work」 chip and
    /// the user can immediately see why the pet may be especially quiet.
    pub focus_mode: Option<String>,
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
    // Iter Cν: idle_minutes (since user last interacted) drives the
    // long-absence-reunion composite rule. Distinct from cadence_min above
    // (which tracks the pet's own last utterance).
    let idle_min_for_rules: u64 = snap.idle_seconds / 60;
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
        today_count_for_rules == 0,
    );
    let companionship_days_for_rules = crate::companionship::companionship_days().await;
    let data_labels = active_data_driven_rule_labels(
        proactive_count as usize,
        today_count_for_rules,
        chatty_day_threshold,
        env_total,
        env_with_any,
        companionship_days_for_rules,
    );
    let composite_labels = active_composite_rule_labels(
        wake_back,
        has_plan,
        cadence_min,
        today_count_for_rules,
        chatty_day_threshold,
        pre_quiet,
        idle_min_for_rules,
    );
    let active_prompt_rules: Vec<String> = env_labels
        .iter()
        .chain(data_labels.iter())
        .chain(composite_labels.iter())
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
        day_of_week: format_day_of_week_hint(now.weekday()),
        idle_register: user_absence_tier(idle_min_for_rules).to_string(),
        idle_minutes: idle_min_for_rules,
        // Iter D2: surface the same milestone label that drives the
        // companionship-milestone prompt rule (Cρ) so the panel can flag the
        // day visually.
        companionship_milestone: companionship_milestone(companionship_days_for_rules)
            .map(|s| s.to_string()),
        companionship_days: companionship_days_for_rules,
        // Iter D3: macOS Focus state — same source as the gate path uses.
        // Returns None on non-macOS or when no Focus is active.
        focus_mode: match crate::focus_mode::focus_status().await {
            Some(s) if s.active => s.name.or_else(|| Some("active".to_string())),
            _ => None,
        },
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

/// Iter Cμ: map idle_minutes (since the *user* last interacted with the pet) into
/// a register cue distinct from `idle_tier`. The pet's cadence ("我刚说过话")
/// vs user absence ("用户刚走开几小时") are different axes — the prompt benefits
/// from both. Used in the time line so the LLM can lean into "终于回来了" /
/// "想你了一下" registers when warranted, instead of treating 5 minutes and 5 hours
/// of absence the same way.
pub fn user_absence_tier(idle_minutes: u64) -> &'static str {
    match idle_minutes {
        0..=15 => "用户刚刚还在",
        16..=60 => "用户离开了一小会儿",
        61..=180 => "用户走开有一两小时了",
        181..=480 => "用户已经离开了大半天",
        481..=1440 => "用户一整天没出现",
        _ => "用户至少一天没和你互动",
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

/// Chinese label for a weekday. Used by `format_day_of_week_hint` so the LLM sees
/// "今天是周X（工作日/周末）" instead of just date arithmetic; weekday vs weekend
/// shifts what topics make sense (Friday-night slack vs Monday-morning ramp-up).
pub fn weekday_zh(wd: chrono::Weekday) -> &'static str {
    use chrono::Weekday::*;
    match wd {
        Mon => "周一",
        Tue => "周二",
        Wed => "周三",
        Thu => "周四",
        Fri => "周五",
        Sat => "周六",
        Sun => "周日",
    }
}

/// "周末" (Sat / Sun) vs "工作日" (Mon–Fri). Distinct from `weekday_zh` because the
/// prompt phrases both — the LLM benefits from being told the category explicitly
/// instead of inferring it from "周六" alone (less robust across model versions).
pub fn weekday_kind_zh(wd: chrono::Weekday) -> &'static str {
    use chrono::Weekday::*;
    match wd {
        Sat | Sun => "周末",
        _ => "工作日",
    }
}

/// Format the combined day-of-week hint that the proactive time line embeds.
/// Pure for testability — the `run_proactive_turn` callsite passes `now_local.weekday()`.
/// Output example: "周日 · 周末" / "周二 · 工作日". Joined by `·` to read naturally
/// when concatenated into "（下午，周二 · 工作日）".
pub fn format_day_of_week_hint(wd: chrono::Weekday) -> String {
    format!("{} · {}", weekday_zh(wd), weekday_kind_zh(wd))
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
            let companionship_days_for_rules =
                crate::companionship::companionship_days().await;
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
            let since_last_for_rules =
                snap_for_rules.since_last_proactive_seconds.map(|s| s / 60);
            let idle_min_for_rules: u64 = snap_for_rules.idle_seconds / 60;
            let composite_label_set = active_composite_rule_labels(
                wake_back_for_rules,
                has_plan_for_rules,
                since_last_for_rules,
                chatty_today,
                chatty_threshold,
                pre_quiet_for_rules,
                idle_min_for_rules,
            );
            let active_labels: Vec<&'static str> = env_label_set
                .iter()
                .chain(data_label_set.iter())
                .chain(composite_label_set.iter())
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
                    // Record long-running prompt tilt (Iter 96): bump exactly one of four
                    // buckets based on the active label set we computed for this Run. Done
                    // here rather than after the LLM call so the count tracks "Run with
                    // these rules dispatched" — the prompt was sent regardless of outcome.
                    app.state::<crate::commands::debug::ProcessCountersStore>()
                        .inner()
                        .prompt_tilt
                        .record_dispatch(&active_labels);
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
    let (cadence_hint, since_last_proactive_minutes) = {
        let snap = clock.snapshot().await;
        let mins = snap.since_last_proactive_seconds.map(|s| s / 60);
        let hint = match mins {
            Some(m) => format!("距上次你主动开口约 {} 分钟（{}）。", m, idle_tier(m)),
            None => "你还没有主动开过口，这是第一次。".to_string(),
        };
        (hint, mins)
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
            // Iter Cy: redact each line before re-injecting into the prompt. The pet's
            // own past utterances may have referenced private terms (the LLM doesn't
            // know to self-redact); redacting at read-time prevents re-leak even
            // though the on-disk history file stays pristine.
            let bullets: Vec<String> = recent
                .iter()
                .map(|line| {
                    let stripped = crate::speech_history::strip_timestamp(line);
                    format!("· {}", crate::redaction::redact_with_settings(stripped))
                })
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
    // Iter Cβ: weekday/weekend label, e.g. "周日 · 周末". Joins with period in the
    // time line so the LLM can lean on "周五晚上"-flavor cues without parsing dates.
    let day_of_week = format_day_of_week_hint(now_local.weekday());
    // Iter Cμ: register cue derived from idle_minutes — "用户刚刚还在" vs
    // "用户至少一天没和你互动". Lets the LLM differentiate 5-min idle from 5-hour idle.
    let idle_register = user_absence_tier(idle_minutes);

    // Scan the `todo` memory category for user-set reminders that have just come due.
    // Each becomes a bullet line. The whole hint is empty when nothing's due.
    let reminders_hint = build_reminders_hint(now_local.naive_local());

    // Pull the pet's own short-term plan from ai_insights/daily_plan, if it has written one.
    let plan_hint = build_plan_hint();
    let persona_hint = build_persona_hint();
    // Iter 103: read mood-trend summary from mood_history.log (window=50, min=5).
    // Window is generous because mood is deduped against the last entry, so 50 lines
    // typically span 1-2 weeks of distinct mood changes. min=5 avoids early-day noise.
    let mood_trend_hint = crate::mood_history::build_trend_hint(50, 5).await;
    // Iter Cα: surface user_profile memory as ambient context so the LLM sees
    // basic user habits without firing memory_search every turn. Empty until
    // the pet has written at least one user_profile entry.
    let user_profile_hint = build_user_profile_hint();
    // Iter Cγ: surface owner-assigned butler tasks each proactive turn so the
    // pet's task queue stays visible — the pet shouldn't forget the user asked
    // it to "每天早上发日历" between turns. Iter Cζ adds schedule-awareness
    // (`[every: HH:MM]` / `[once: ...]` prefixes); `now` is passed so due tasks
    // bubble to the top with a "⏰ 到期" marker.
    let butler_tasks_hint = build_butler_tasks_hint(now_local.naive_local());
    // Iter Cυ: owner-name from settings, empty when unset.
    let user_name = get_settings()
        .map(|s| s.user_name)
        .unwrap_or_default();

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
        day_of_week: &day_of_week,
        idle_minutes,
        idle_register,
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
        since_last_proactive_minutes,
        companionship_days,
        persona_hint: &persona_hint,
        mood_trend_hint: &mood_trend_hint,
        user_profile_hint: &user_profile_hint,
        butler_tasks_hint: &butler_tasks_hint,
        user_name: &user_name,
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

/// Cap on how many `butler_tasks` entries to surface in the proactive prompt. Above
/// this the block dominates the prompt; the LLM can still call `memory_list` to see
/// the full backlog if it needs to triage.
pub const BUTLER_TASKS_HINT_MAX_ITEMS: usize = 6;
/// Per-task description char cap. Long task specs become noisy when stacked; the
/// detail.md is one read_file call away when the LLM actually picks one up.
pub const BUTLER_TASKS_HINT_DESC_CHARS: usize = 100;

/// Pure formatter for the butler-tasks block. Items are `(title, description, updated_at)`.
/// Items with a `[every: HH:MM]` / `[once: ...]` prefix that are *due now* (per
/// `is_butler_due`) bubble to the top with a "⏰ 到期" marker; the rest follow sorted by
/// `updated_at` ascending so the oldest pending tasks aren't lost at the bottom.
/// Empty list / zero cap → empty string.
///
/// Iter Cγ introduced the block; Iter Cζ added schedule-awareness via `now`. Distinct
/// from reminders_hint (user's nudges) — this is the pet's own assignment queue.
pub fn format_butler_tasks_block(
    items: &[(String, String, String)],
    max_items: usize,
    max_desc_chars: usize,
    now: chrono::NaiveDateTime,
) -> String {
    if items.is_empty() || max_items == 0 {
        return String::new();
    }
    // Compute due-ness + error state once per item and stable-sort.
    let mut annotated: Vec<(&(String, String, String), bool, bool)> = items
        .iter()
        .map(|i| {
            let due = parse_butler_schedule_prefix(&i.1)
                .map(|(sched, _)| is_butler_due(&sched, now, &i.2))
                .unwrap_or(false);
            let errored = has_butler_error(&i.1);
            (i, due, errored)
        })
        .collect();
    // Due → not-due primary, updated_at ascending secondary. Errored items keep
    // their primary slot — they're often also due (last execution failed) so they
    // bubble up naturally; if not due, they stay in normal order so the user
    // doesn't drown in stale errors.
    annotated.sort_by(|(a, a_due, _), (b, b_due, _)| match (a_due, b_due) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.2.cmp(&b.2),
    });
    let n = annotated.len().min(max_items);
    let due_count = annotated.iter().take(n).filter(|(_, d, _)| *d).count();
    let err_count = annotated.iter().take(n).filter(|(_, _, e)| *e).count();
    let mut lines: Vec<String> = Vec::with_capacity(n + 2);
    let header = match (due_count, err_count) {
        (0, 0) => format!("用户委托给你的管家任务（共 {} 条，按最早委托排在前）：", n),
        (d, 0) => format!(
            "用户委托给你的管家任务（共 {} 条，其中 {} 条到期，按到期 → 最早委托排在前）：",
            n, d
        ),
        (0, e) => format!(
            "用户委托给你的管家任务（共 {} 条，其中 {} 条上次执行失败需要复查）：",
            n, e
        ),
        (d, e) => format!(
            "用户委托给你的管家任务（共 {} 条，{} 条到期、{} 条上次失败）：",
            n, d, e
        ),
    };
    lines.push(header);
    for ((title, desc, _), due, errored) in annotated.iter().take(n) {
        let trimmed = desc.trim();
        let truncated: String = if trimmed.chars().count() <= max_desc_chars {
            trimmed.to_string()
        } else {
            let head: String = trimmed.chars().take(max_desc_chars).collect();
            format!("{}…", head)
        };
        // Marker order: error first (most urgent) → due second. Both can co-occur.
        let mut marker = String::new();
        if *errored {
            marker.push_str("❌ 错误 · ");
        }
        if *due {
            marker.push_str("⏰ 到期 · ");
        }
        lines.push(format!("- {}{}：{}", marker, title.trim(), truncated));
    }
    lines.push(
        "执行完一项后用 `memory_edit update` 更新进度（标题前加 [done] / 写最后执行时间），\
完全不需要的用 `memory_edit delete` 移除。带 `[every: HH:MM]` 或 `[once: ...]` 前缀的任务标记了到期窗口——\
看到「⏰ 到期」就该这一轮优先处理它。\n\
**执行失败处理**：如果你这一轮调用 read_file / write_file / edit_file / bash 时失败（文件不存在、权限不够、命令报错等），\
用 `memory_edit update` 在 description 里加一段 `[error: 简短原因]`（保留原有 `[every:]` / `[once:]` 前缀，error 段贴在它后面）。\
下次重试成功时记得移除这段 error 标记。看到「❌ 错误」标记的任务说明上次失败了，请检查描述里的失败原因再决定要不要重试。"
            .to_string(),
    );
    lines.join("\n")
}

/// Iter Cπ: detect whether a butler task description is currently flagged as errored.
/// Convention: LLM prepends or embeds `[error: brief reason]` after a tool failure
/// during execution. We only check the substring `[error` — case-sensitive, no
/// regex — to keep this cheap and tolerant of `[error:`, `[error :`, `[error]`
/// variants the LLM might write.
pub fn has_butler_error(desc: &str) -> bool {
    desc.contains("[error")
}

/// Schedule for a butler task (Iter Cζ). Distinct from `ReminderTarget` semantically:
/// reminders are nudges *for the user*, schedules tell *the pet* when to act on a task.
/// Both share the time-arithmetic shape, but the firing logic and "already done" check
/// differ — schedules need to know whether the most recent fire already triggered work.
#[derive(Debug, PartialEq, Eq)]
pub enum ButlerSchedule {
    /// Daily recurring at HH:MM local. Implicit window — see `is_butler_due` for how it
    /// resolves "already executed today".
    Every(u8, u8),
    /// Single-fire at the absolute moment.
    Once(chrono::NaiveDateTime),
}

/// Parse a schedule prefix from a butler_tasks description. Conventions:
///   - `[every: HH:MM] topic`              — daily recurring
///   - `[once: YYYY-MM-DD HH:MM] topic`    — one-shot
/// Returns `(schedule, topic)` on clean parse, `None` otherwise. Tasks without a prefix
/// are unscheduled — the LLM picks them up on its own judgment, not by clock.
pub fn parse_butler_schedule_prefix(desc: &str) -> Option<(ButlerSchedule, String)> {
    let trimmed = desc.trim_start();
    if let Some(after_open) = trimmed.strip_prefix("[every:") {
        let close_idx = after_open.find(']')?;
        let inside = after_open[..close_idx].trim();
        let topic = after_open[close_idx + 1..].trim().to_string();
        if topic.is_empty() {
            return None;
        }
        let (hh, mm) = inside.split_once(':')?;
        let hour: u8 = hh.trim().parse().ok()?;
        let minute: u8 = mm.trim().parse().ok()?;
        if hour > 23 || minute > 59 {
            return None;
        }
        return Some((ButlerSchedule::Every(hour, minute), topic));
    }
    if let Some(after_open) = trimmed.strip_prefix("[once:") {
        let close_idx = after_open.find(']')?;
        let inside = after_open[..close_idx].trim();
        let topic = after_open[close_idx + 1..].trim().to_string();
        if topic.is_empty() {
            return None;
        }
        let (date_str, time_str) = inside.split_once(' ')?;
        let date = chrono::NaiveDate::parse_from_str(date_str.trim(), "%Y-%m-%d").ok()?;
        let time = chrono::NaiveTime::parse_from_str(time_str.trim(), "%H:%M").ok()?;
        return Some((ButlerSchedule::Once(date.and_time(time)), topic));
    }
    None
}

/// Parse a stored `updated_at` string ("YYYY-MM-DDTHH:MM:SS+HH:MM") to a local
/// `NaiveDateTime`. Returns `None` on malformed input — caller decides what that
/// means (typically "treat as never updated").
fn parse_updated_at_local(s: &str) -> Option<chrono::NaiveDateTime> {
    chrono::DateTime::parse_from_rfc3339(s.trim())
        .ok()
        .map(|dt| dt.with_timezone(&chrono::Local).naive_local())
}

/// Decide whether a scheduled butler task is *due now* — past its most recent fire AND
/// not yet executed since that fire. `last_updated` is the task's `updated_at`; an
/// unparseable / empty value is treated as "never executed" (always due if past target).
///
/// Semantics:
/// - `Every(h, m)`: most-recent-fire = today h:m if `now >= today h:m` else yesterday h:m.
///   Due iff `last_updated < most-recent-fire`. So a task touched after today's fire is
///   suppressed until tomorrow's; a task touched before today's fire is due now.
/// - `Once(dt)`: due iff `now >= dt && last_updated < dt`. Past + unexecuted.
pub fn is_butler_due(
    schedule: &ButlerSchedule,
    now: chrono::NaiveDateTime,
    last_updated: &str,
) -> bool {
    let last = parse_updated_at_local(last_updated);
    match schedule {
        ButlerSchedule::Once(dt) => {
            if now < *dt {
                return false;
            }
            match last {
                Some(u) => u < *dt,
                None => true, // unparseable → never executed → due
            }
        }
        ButlerSchedule::Every(h, m) => {
            let today = now.date();
            let target_today = match today.and_hms_opt(*h as u32, *m as u32, 0) {
                Some(t) => t,
                None => return false, // shouldn't happen — parser bounds-checks
            };
            let most_recent_fire = if now >= target_today {
                target_today
            } else {
                target_today - chrono::Duration::days(1)
            };
            match last {
                Some(u) => u < most_recent_fire,
                None => true, // never updated → due (will need first execution)
            }
        }
    }
}

/// Iter Cλ: pure decider — given a butler task's description, updated_at, current
/// time, and grace hours, return true iff this is a `[once: ...]` task that has
/// been executed (updated_at >= target) AND is now safely past the configured
/// retention grace period. Used by `sweep_completed_once_butler_tasks` so the
/// consolidate loop can auto-clean finished one-shot tasks the way it already
/// cleans stale reminders. Recurring `[every: ...]` tasks return false — they
/// re-fire and shouldn't be deleted.
pub fn is_completed_once(
    desc: &str,
    last_updated: &str,
    now: chrono::NaiveDateTime,
    grace_hours: u64,
) -> bool {
    let Some((sched, _)) = parse_butler_schedule_prefix(desc) else {
        return false;
    };
    let target = match sched {
        ButlerSchedule::Once(dt) => dt,
        ButlerSchedule::Every(_, _) => return false,
    };
    let Some(last) = parse_updated_at_local(last_updated) else {
        return false;
    };
    if last < target {
        return false; // executed before target = invalid; treat as not-yet-done
    }
    let grace_end = target + chrono::Duration::hours(grace_hours as i64);
    now >= grace_end
}

/// Read butler_tasks memory entries and format the prompt-side digest. `now` is
/// injected so the call site (run_proactive_turn) shares one clock anchor with the
/// rest of the prompt build. Returns "" when the category is empty. Output is redacted
/// via `redact_with_settings` for the same reason as `build_user_profile_hint`.
pub fn build_butler_tasks_hint(now: chrono::NaiveDateTime) -> String {
    let Ok(index) = crate::commands::memory::memory_list(Some("butler_tasks".to_string())) else {
        return String::new();
    };
    let Some(cat) = index.categories.get("butler_tasks") else {
        return String::new();
    };
    let tuples: Vec<(String, String, String)> = cat
        .items
        .iter()
        .map(|i| {
            (
                i.title.clone(),
                i.description.clone(),
                i.updated_at.clone(),
            )
        })
        .collect();
    let block = format_butler_tasks_block(
        &tuples,
        BUTLER_TASKS_HINT_MAX_ITEMS,
        BUTLER_TASKS_HINT_DESC_CHARS,
        now,
    );
    if block.is_empty() {
        return String::new();
    }
    crate::redaction::redact_with_settings(&block)
}

/// Tauri command returning the raw persona-summary description (Iter 105) — without
/// the "你最近一次自我反思的画像（来自 consolidate）：" header `build_persona_hint`
/// adds. The Persona panel surfaces this directly so users can read what the pet
/// has written about itself. Empty when no summary exists yet.
#[tauri::command]
pub fn get_persona_summary() -> String {
    let Ok(index) = crate::commands::memory::memory_list(Some("ai_insights".to_string())) else {
        return String::new();
    };
    let Some(cat) = index.categories.get("ai_insights") else {
        return String::new();
    };
    cat.items
        .iter()
        .find(|i| i.title == "persona_summary")
        .map(|i| i.description.trim().to_string())
        .unwrap_or_default()
}

/// Read the pet's self-authored persona summary from `ai_insights/persona_summary`.
/// Iter 102: this is what the consolidate loop generates by reflecting on recent
/// speech_history + user_profile. Returns the description verbatim with a header line,
/// or empty when no summary has been written yet (fresh installs / not enough signal).
///
/// `pub` since Iter 104 — reactive chat reuses this to inject the same persona layer
/// into its system prompt, so the long-term identity isn't proactive-only.
pub fn build_persona_hint() -> String {
    let Ok(index) = crate::commands::memory::memory_list(Some("ai_insights".to_string())) else {
        return String::new();
    };
    let Some(cat) = index.categories.get("ai_insights") else {
        return String::new();
    };
    let summary = cat.items.iter().find(|i| i.title == "persona_summary");
    match summary {
        Some(item) if !item.description.trim().is_empty() => {
            // Iter Cw: redact the persona summary before re-injecting into the
            // proactive prompt. The LLM-authored description may have echoed private
            // terms (active_window app names / user_profile entries it didn't know
            // were sensitive when it wrote them); redacting here ensures the same
            // user-configured patterns cover this self-loop input too. The on-disk
            // memory file stays pristine — the panel's `get_persona_summary` command
            // intentionally returns the unredacted text since that view is local.
            let redacted = crate::redaction::redact_with_settings(item.description.trim());
            format!(
                "你最近一次自我反思的画像（来自 consolidate）：\n{}",
                redacted
            )
        }
        _ => String::new(),
    }
}

/// Cap on how many `user_profile` entries to surface in the proactive prompt. Above
/// this the digest gets long enough to dominate the prompt and dilute its other
/// signals; the LLM can still call `memory_search` for the older ones if a topic
/// asks for them.
pub const USER_PROFILE_HINT_MAX_ITEMS: usize = 6;
/// Per-entry description char cap. Long bios become noisy when stacked 6 deep, so
/// the prompt sees a one-liner per habit; the full body is one tool call away.
pub const USER_PROFILE_HINT_DESC_CHARS: usize = 80;

/// Pure helper — formats a list of `(title, description, updated_at)` tuples into
/// the user-profile prompt block. Sorted by `updated_at` descending so the most
/// recently-touched habits surface first. Returns "" when items is empty so the
/// prompt builder's `push_if_nonempty` skips the line cleanly.
///
/// Extracted from `build_user_profile_hint` so the truncation / sort / header logic
/// is unit-testable without going through `memory_list`'s on-disk index.
pub fn format_user_profile_block(
    items: &[(String, String, String)],
    max_items: usize,
    max_desc_chars: usize,
) -> String {
    if items.is_empty() || max_items == 0 {
        return String::new();
    }
    let mut sorted: Vec<&(String, String, String)> = items.iter().collect();
    // updated_at is ISO-8601 with offset → string compare matches chronological
    // order; descending = most recent first.
    sorted.sort_by(|a, b| b.2.cmp(&a.2));
    let n = sorted.len().min(max_items);
    let mut lines: Vec<String> = Vec::with_capacity(n + 1);
    lines.push(format!(
        "你了解的用户习惯（来自 user_profile 记忆，最新 {} 条）：",
        n
    ));
    for (title, desc, _) in sorted.iter().take(n) {
        let trimmed = desc.trim();
        let truncated: String = if trimmed.chars().count() <= max_desc_chars {
            trimmed.to_string()
        } else {
            let head: String = trimmed.chars().take(max_desc_chars).collect();
            format!("{}…", head)
        };
        lines.push(format!("- {}：{}", title.trim(), truncated));
    }
    lines.join("\n")
}

/// Read `user_profile` memory entries and format a compact digest block for the
/// proactive prompt (Iter Cα). Returns empty when the category has no entries —
/// `push_if_nonempty` then skips it cleanly. Output is redacted via
/// `redact_with_settings` so any private terms the LLM stored in habit
/// descriptions don't leak back into a fresh proactive prompt.
pub fn build_user_profile_hint() -> String {
    let Ok(index) = crate::commands::memory::memory_list(Some("user_profile".to_string())) else {
        return String::new();
    };
    let Some(cat) = index.categories.get("user_profile") else {
        return String::new();
    };
    let tuples: Vec<(String, String, String)> = cat
        .items
        .iter()
        .map(|i| {
            (
                i.title.clone(),
                i.description.clone(),
                i.updated_at.clone(),
            )
        })
        .collect();
    let block = format_user_profile_block(
        &tuples,
        USER_PROFILE_HINT_MAX_ITEMS,
        USER_PROFILE_HINT_DESC_CHARS,
    );
    if block.is_empty() {
        return String::new();
    }
    crate::redaction::redact_with_settings(&block)
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
            // 2026-05-03 is a Sunday — matches the 周末 register tests assert.
            day_of_week: "周日 · 周末",
            idle_minutes: 20,
            // 20 min idle = 用户离开了一小会儿 (16-60 band) — keeps existing tests stable.
            idle_register: "用户离开了一小会儿",
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
            // Default 1 = above the first-of-day trigger (Iter Cξ requires == 0)
            // and still below the default chatty_day_threshold of 5 — keeps existing
            // tests neutral on both rules. Tests for either bump this explicitly.
            today_speech_count: 1,
            // Mirrors the production default (5) — keeps existing tests stable while
            // letting chatty-day tests assert behavior at exact threshold boundary.
            chatty_day_threshold: 5,
            // Default 0/0 = below ENV_AWARENESS_MIN_SAMPLES → rule won't fire. Tests that
            // exercise the corrective rule bump these explicitly.
            env_spoke_total: 0,
            env_spoke_with_any: 0,
            // Default Some(8) — well below LONG_IDLE_MINUTES, matches the cadence_hint
            // string ("约 8 分钟（刚说过话，话题还热）"). Tests for long-idle bump this
            // above 60 explicitly.
            since_last_proactive_minutes: Some(8),
            // Default 5 — past day-0 special framing, but not a milestone (30 / 100 /
            // 365 / etc.) so the Iter Cρ companionship-milestone rule stays silent
            // by default. Tests that need either the day-0 framing or a milestone
            // set this explicitly.
            companionship_days: 5,
            // Default empty — base inputs simulate the pre-Iter 102 state where no
            // persona summary has been written yet. Tests for the persona hint set this
            // to a non-empty string to assert injection.
            persona_hint: "",
            // Default empty — pre-Iter 103 state, no mood trend yet. Tests for the
            // trend hint set this explicitly.
            mood_trend_hint: "",
            // Default empty — pre-Iter Cα state, no user_profile entries surfaced.
            // Tests for the user-profile hint set this explicitly.
            user_profile_hint: "",
            // Default empty — pre-Iter Cγ state, no owner-assigned butler tasks yet.
            // Tests for the butler_tasks hint set this explicitly.
            butler_tasks_hint: "",
            // Default empty — pre-Iter Cυ state, no owner name set in settings.
            // Tests for the user_name line set this explicitly.
            user_name: "",
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
        assert!(active_data_driven_rule_labels(100, 0, 5, 0, 0, 0).is_empty());
    }

    #[test]
    fn active_data_driven_rule_labels_picks_up_each_rule_independently() {
        // Only icebreaker.
        assert_eq!(
            active_data_driven_rule_labels(0, 0, 5, 0, 0, 0),
            vec!["icebreaker"],
        );
        // Only chatty.
        assert_eq!(
            active_data_driven_rule_labels(100, 5, 5, 0, 0, 0),
            vec!["chatty"],
        );
        // Only env-awareness.
        assert_eq!(
            active_data_driven_rule_labels(100, 0, 5, 12, 2, 0),
            vec!["env-awareness"],
        );
    }

    #[test]
    fn active_data_driven_rule_labels_combine_in_firing_order() {
        // All three at once: should appear in the same order proactive_rules pushes them.
        let labels = active_data_driven_rule_labels(0, 6, 5, 12, 1, 0);
        assert_eq!(labels, vec!["icebreaker", "chatty", "env-awareness"]);
    }

    #[test]
    fn active_data_driven_rule_labels_zero_threshold_disables_chatty() {
        // chatty threshold == 0 means the user opted out — even today_count=99 shouldn't
        // surface the chatty label.
        assert!(active_data_driven_rule_labels(100, 99, 0, 0, 0, 0).is_empty());
    }

    #[test]
    fn active_environmental_rule_labels_empty_when_all_false() {
        assert!(active_environmental_rule_labels(false, false, false, false, false, false).is_empty());
    }

    #[test]
    fn active_environmental_rule_labels_picks_each_independently() {
        assert_eq!(
            active_environmental_rule_labels(true, false, false, false, false, false),
            vec!["wake-back"],
        );
        assert_eq!(
            active_environmental_rule_labels(false, true, false, false, false, false),
            vec!["first-mood"],
        );
        assert_eq!(
            active_environmental_rule_labels(false, false, true, false, false, false),
            vec!["pre-quiet"],
        );
        assert_eq!(
            active_environmental_rule_labels(false, false, false, true, false, false),
            vec!["reminders"],
        );
        assert_eq!(
            active_environmental_rule_labels(false, false, false, false, true, false),
            vec!["plan"],
        );
        assert_eq!(
            active_environmental_rule_labels(false, false, false, false, false, true),
            vec!["first-of-day"],
        );
    }

    #[test]
    fn companionship_milestone_fixed_thresholds() {
        assert_eq!(companionship_milestone(7), Some("刚好一周"));
        assert_eq!(companionship_milestone(30), Some("满一个月"));
        assert_eq!(companionship_milestone(100), Some("百日纪念"));
        assert_eq!(companionship_milestone(180), Some("满半年"));
        assert_eq!(companionship_milestone(365), Some("满一年"));
    }

    #[test]
    fn companionship_milestone_returns_none_for_non_milestones() {
        assert_eq!(companionship_milestone(0), None);
        assert_eq!(companionship_milestone(1), None);
        assert_eq!(companionship_milestone(6), None);
        assert_eq!(companionship_milestone(8), None);
        assert_eq!(companionship_milestone(29), None);
        assert_eq!(companionship_milestone(31), None);
        assert_eq!(companionship_milestone(99), None);
        assert_eq!(companionship_milestone(364), None);
        assert_eq!(companionship_milestone(366), None);
    }

    #[test]
    fn companionship_milestone_yearly_after_first_year() {
        assert_eq!(companionship_milestone(730), Some("又一个周年"));
        assert_eq!(companionship_milestone(1095), Some("又一个周年"));
        assert_eq!(companionship_milestone(1460), Some("又一个周年"));
        // Off-by-one days near anniversaries should not fire.
        assert_eq!(companionship_milestone(729), None);
        assert_eq!(companionship_milestone(731), None);
    }

    #[test]
    fn companionship_milestone_rule_fires_through_proactive_rules() {
        let mut inputs = base_inputs();
        inputs.companionship_days = 100;
        let rules = proactive_rules(&inputs);
        assert!(rules.iter().any(|r| r.contains("百日纪念")));
        assert!(rules.iter().any(|r| r.contains("今天是和用户相处的")));

        inputs.companionship_days = 5; // not a milestone
        let rules = proactive_rules(&inputs);
        assert!(!rules.iter().any(|r| r.contains("今天是和用户相处的")));
    }

    #[test]
    fn prompt_includes_user_name_line_when_set() {
        // Iter Cυ: when settings.user_name is non-empty, the proactive prompt
        // gets a "你的主人是「X」" line right after the companionship line so
        // the LLM can address the owner by name.
        let mut inputs = base_inputs();
        inputs.user_name = "moon";
        let p = build_proactive_prompt(&inputs);
        assert!(p.contains("你的主人是「moon」"));
        // Ordering: companionship line precedes user_name line precedes the
        // optional persona/profile blocks (those are empty in base_inputs).
        let p_companion = p.find("一起走过").unwrap();
        let p_user = p.find("你的主人是").unwrap();
        assert!(p_companion < p_user, "companionship before user_name");
    }

    #[test]
    fn prompt_omits_user_name_line_when_empty_or_whitespace() {
        let inputs = base_inputs(); // default user_name = ""
        let p = build_proactive_prompt(&inputs);
        assert!(!p.contains("你的主人是"));

        let mut inputs2 = base_inputs();
        inputs2.user_name = "   \t  ";
        let p2 = build_proactive_prompt(&inputs2);
        assert!(!p2.contains("你的主人是"), "whitespace-only must be skipped");
    }

    #[test]
    fn prompt_trims_user_name_whitespace() {
        let mut inputs = base_inputs();
        inputs.user_name = "  moon  ";
        let p = build_proactive_prompt(&inputs);
        assert!(p.contains("你的主人是「moon」"));
        assert!(!p.contains("「  moon"));
    }

    #[test]
    fn first_of_day_only_fires_when_today_count_is_zero() {
        // Iter Cξ: first-of-day is wired in proactive_rules from
        // `inputs.today_speech_count == 0`. Pin the integration here so a future
        // refactor (e.g., changing how the env_labels callsite reads the count)
        // doesn't silently break the trigger.
        let mut inputs = base_inputs();
        inputs.today_speech_count = 0;
        let rules_zero = proactive_rules(&inputs);
        assert!(rules_zero.iter().any(|r| r.contains("今天的第一次开口")));

        inputs.today_speech_count = 1;
        let rules_one = proactive_rules(&inputs);
        assert!(!rules_one.iter().any(|r| r.contains("今天的第一次开口")));
    }

    #[test]
    fn active_environmental_rule_labels_combine_in_firing_order() {
        let labels = active_environmental_rule_labels(true, true, true, true, true, true);
        // Iter Cξ: first-of-day slots between first-mood and pre-quiet so the
        // greeting register comes after mood bootstrap but before quiet-hours
        // wind-down.
        assert_eq!(
            labels,
            vec!["wake-back", "first-mood", "first-of-day", "pre-quiet", "reminders", "plan"],
        );
    }

    #[test]
    fn format_companionship_line_day_zero_uses_first_day_framing() {
        let line = format_companionship_line(0);
        assert!(line.contains("第一天"));
        assert!(line.contains("初识"));
    }

    #[test]
    fn format_companionship_line_after_day_zero_states_count() {
        let line = format_companionship_line(42);
        assert!(line.contains("42 天"));
        // Mentions familiarity / shared time so the LLM is invited to use it.
        assert!(line.contains("相处时长"));
    }

    #[test]
    fn prompt_includes_companionship_line() {
        let mut inputs = base_inputs();
        inputs.companionship_days = 7;
        let p = build_proactive_prompt(&inputs);
        assert!(p.contains("7 天"));
    }

    #[test]
    fn prompt_includes_first_day_framing_at_day_zero() {
        let mut inputs = base_inputs();
        inputs.companionship_days = 0;
        let p = build_proactive_prompt(&inputs);
        assert!(p.contains("第一天"));
    }

    #[test]
    fn prompt_includes_persona_hint_when_set() {
        let mut inputs = base_inputs();
        inputs.persona_hint =
            "你最近一次自我反思的画像（来自 consolidate）：\n我倾向短句，话题偏当下场景。";
        let p = build_proactive_prompt(&inputs);
        assert!(p.contains("自我反思的画像"));
        assert!(p.contains("我倾向短句"));
    }

    #[test]
    fn prompt_omits_persona_hint_when_empty() {
        let inputs = base_inputs(); // default persona_hint = ""
        let p = build_proactive_prompt(&inputs);
        assert!(!p.contains("自我反思的画像"));
    }

    #[test]
    fn prompt_includes_mood_trend_hint_when_set() {
        let mut inputs = base_inputs();
        inputs.mood_trend_hint =
            "你最近 30 次心情记录里：Tap × 12、Idle × 10、Flick × 5（按出现次数排序）。这是你长期的情绪谱——可以让 ta 渗进当下语气，但不必生硬带出。";
        let p = build_proactive_prompt(&inputs);
        assert!(p.contains("长期的情绪谱"));
        assert!(p.contains("Tap × 12"));
    }

    #[test]
    fn prompt_omits_mood_trend_hint_when_empty() {
        let inputs = base_inputs(); // default mood_trend_hint = ""
        let p = build_proactive_prompt(&inputs);
        assert!(!p.contains("长期的情绪谱"));
    }

    #[test]
    fn prompt_includes_user_profile_hint_when_set() {
        let mut inputs = base_inputs();
        inputs.user_profile_hint =
            "你了解的用户习惯（来自 user_profile 记忆，最新 2 条）：\n- 起床时间：通常 8:30 起床\n- 喜欢：偏好 dark theme 编辑器";
        let p = build_proactive_prompt(&inputs);
        assert!(p.contains("用户习惯"));
        assert!(p.contains("起床时间"));
        assert!(p.contains("dark theme"));
    }

    #[test]
    fn prompt_omits_user_profile_hint_when_empty() {
        let inputs = base_inputs(); // default user_profile_hint = ""
        let p = build_proactive_prompt(&inputs);
        assert!(!p.contains("用户习惯"));
    }

    #[test]
    fn format_user_profile_block_empty_returns_empty() {
        assert_eq!(
            format_user_profile_block(&[], 6, 80),
            String::new(),
            "no items → no block",
        );
    }

    #[test]
    fn format_user_profile_block_zero_max_returns_empty() {
        let items = vec![("habit".into(), "desc".into(), "2026-05-03T10:00:00+08:00".into())];
        assert_eq!(format_user_profile_block(&items, 0, 80), String::new());
    }

    #[test]
    fn format_user_profile_block_sorts_by_updated_at_desc() {
        let items = vec![
            ("旧习惯".into(), "desc-a".into(), "2026-04-01T10:00:00+08:00".into()),
            ("新习惯".into(), "desc-b".into(), "2026-05-03T10:00:00+08:00".into()),
            ("中习惯".into(), "desc-c".into(), "2026-04-20T10:00:00+08:00".into()),
        ];
        let out = format_user_profile_block(&items, 6, 80);
        let new_idx = out.find("新习惯").unwrap();
        let mid_idx = out.find("中习惯").unwrap();
        let old_idx = out.find("旧习惯").unwrap();
        assert!(new_idx < mid_idx, "newest should be first");
        assert!(mid_idx < old_idx, "middle should precede oldest");
    }

    #[test]
    fn format_user_profile_block_caps_item_count() {
        let items: Vec<(String, String, String)> = (0..10)
            .map(|i| {
                (
                    format!("habit-{i}"),
                    format!("desc-{i}"),
                    format!("2026-05-{:02}T10:00:00+08:00", i + 1),
                )
            })
            .collect();
        let out = format_user_profile_block(&items, 3, 80);
        assert!(out.contains("最新 3 条"));
        // Top-3 by date = day 10, 9, 8 (i = 9, 8, 7).
        assert!(out.contains("habit-9"));
        assert!(out.contains("habit-8"));
        assert!(out.contains("habit-7"));
        assert!(!out.contains("habit-6"), "4th-newest should be excluded");
    }

    #[test]
    fn format_user_profile_block_truncates_long_descriptions() {
        let long_desc: String = "字".repeat(120);
        let items = vec![("habit".into(), long_desc, "2026-05-03T10:00:00+08:00".into())];
        let out = format_user_profile_block(&items, 6, 20);
        assert!(out.contains("…"), "should append ellipsis when truncated");
        // Body line should be 1 title + ：+ 20 chars + … = well under the 120.
        let body_chars = out
            .lines()
            .nth(1)
            .unwrap()
            .chars()
            .filter(|c| *c == '字')
            .count();
        assert_eq!(body_chars, 20, "should keep exactly 20 chars before ellipsis");
    }

    #[test]
    fn prompt_includes_butler_tasks_hint_when_set() {
        let mut inputs = base_inputs();
        inputs.butler_tasks_hint =
            "用户委托给你的管家任务（共 1 条，按最早委托排在前）：\n- 早报：每天 8 点把日历发给我";
        let p = build_proactive_prompt(&inputs);
        assert!(p.contains("管家任务"));
        assert!(p.contains("早报"));
        assert!(p.contains("把日历发给我"));
    }

    #[test]
    fn prompt_omits_butler_tasks_hint_when_empty() {
        let inputs = base_inputs();
        let p = build_proactive_prompt(&inputs);
        assert!(!p.contains("管家任务"));
    }

    fn fixed_now() -> chrono::NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(2026, 5, 3)
            .unwrap()
            .and_hms_opt(14, 30, 0)
            .unwrap()
    }

    #[test]
    fn format_butler_tasks_block_empty_returns_empty() {
        assert_eq!(format_butler_tasks_block(&[], 6, 100, fixed_now()), String::new());
    }

    #[test]
    fn format_butler_tasks_block_zero_max_returns_empty() {
        let items = vec![("t".into(), "d".into(), "2026-05-03T10:00:00+08:00".into())];
        assert_eq!(format_butler_tasks_block(&items, 0, 100, fixed_now()), String::new());
    }

    #[test]
    fn format_butler_tasks_block_sorts_oldest_first() {
        let items = vec![
            ("新任务".into(), "d-new".into(), "2026-05-03T10:00:00+08:00".into()),
            ("老任务".into(), "d-old".into(), "2026-04-01T10:00:00+08:00".into()),
            ("中任务".into(), "d-mid".into(), "2026-04-20T10:00:00+08:00".into()),
        ];
        let out = format_butler_tasks_block(&items, 6, 100, fixed_now());
        let old_idx = out.find("老任务").unwrap();
        let mid_idx = out.find("中任务").unwrap();
        let new_idx = out.find("新任务").unwrap();
        assert!(old_idx < mid_idx, "oldest should be first (don't let tasks rot)");
        assert!(mid_idx < new_idx);
    }

    #[test]
    fn format_butler_tasks_block_caps_count_and_includes_footer() {
        let items: Vec<(String, String, String)> = (0..10)
            .map(|i| {
                (
                    format!("task-{i}"),
                    format!("desc-{i}"),
                    format!("2026-05-{:02}T10:00:00+08:00", i + 1),
                )
            })
            .collect();
        let out = format_butler_tasks_block(&items, 3, 100, fixed_now());
        assert!(out.contains("共 3 条"));
        // Top-3 oldest = days 01, 02, 03 = task-0, task-1, task-2.
        assert!(out.contains("task-0"));
        assert!(out.contains("task-2"));
        assert!(!out.contains("task-3"), "4th-oldest should be excluded");
        // Footer instructs how to mark done — important for the user trust path.
        assert!(
            out.contains("memory_edit update") || out.contains("memory_edit delete"),
            "footer should tell LLM how to retire completed tasks"
        );
    }

    #[test]
    fn has_butler_error_detects_marker() {
        assert!(has_butler_error("[error: file not found] write report"));
        assert!(has_butler_error("[every: 09:00] [error: permission denied] morning"));
        assert!(has_butler_error("some text [error] more text"));
        assert!(has_butler_error("[error :spaced] x"));
    }

    #[test]
    fn has_butler_error_negative_cases() {
        assert!(!has_butler_error(""));
        assert!(!has_butler_error("normal task description"));
        assert!(!has_butler_error("[every: 09:00] write daily.md"));
        assert!(!has_butler_error("[once: 2026-05-10 14:00] one-shot"));
        // Word "error" alone must not trigger — the marker is `[error`.
        assert!(!has_butler_error("had an error earlier but recovered"));
    }

    #[test]
    fn format_butler_tasks_block_marks_errored_tasks() {
        let items = vec![(
            "morning-report".into(),
            "[every: 09:00] [error: file not found] write today.md".into(),
            "2026-05-03T09:30:00+08:00".into(),
        )];
        let out = format_butler_tasks_block(&items, 6, 200, fixed_now());
        // Error marker should appear on the task line; header should mention 失败.
        assert!(out.contains("❌ 错误"));
        let header = out.lines().next().unwrap();
        assert!(header.contains("上次执行失败"), "header: {}", header);
    }

    #[test]
    fn format_butler_tasks_block_due_and_errored_co_occur() {
        // [every: 09:00] not yet served today (updated_at = yesterday) AND errored.
        // Marker order: 错误 first, then 到期.
        let items = vec![(
            "report".into(),
            "[every: 09:00] [error: prev fail] write today.md".into(),
            "2026-05-02T08:00:00+08:00".into(),
        )];
        let out = format_butler_tasks_block(&items, 6, 200, fixed_now());
        let body = out.lines().nth(1).unwrap();
        let err_idx = body.find("❌ 错误").unwrap();
        let due_idx = body.find("⏰ 到期").unwrap();
        assert!(err_idx < due_idx, "error marker should precede due marker");
        // Header lists both counts.
        let header = out.lines().next().unwrap();
        assert!(header.contains("到期"));
        assert!(header.contains("失败"));
    }

    #[test]
    fn format_butler_tasks_block_truncates_long_descriptions() {
        let long = "条".repeat(150);
        let items = vec![("task".into(), long, "2026-05-03T10:00:00+08:00".into())];
        let out = format_butler_tasks_block(&items, 6, 30, fixed_now());
        assert!(out.contains("…"));
        let body_chars = out
            .lines()
            .nth(1)
            .unwrap()
            .chars()
            .filter(|c| *c == '条')
            .count();
        assert_eq!(body_chars, 30);
    }

    #[test]
    fn format_butler_tasks_block_due_task_bubbles_to_top_with_marker() {
        // Mid-day fixed_now is 14:30. An [every: 09:00] task whose updated_at is
        // before today 09:00 should be flagged due. A plain (non-scheduled) task
        // older than that should rank below despite its older updated_at.
        let items = vec![
            (
                "plain-old".into(),
                "do something whenever".into(),
                // This is older but unscheduled — should drop below the due task.
                "2026-04-01T08:00:00+08:00".into(),
            ),
            (
                "morning-report".into(),
                "[every: 09:00] write today.md".into(),
                // Updated yesterday, so today's 09:00 fire hasn't been served.
                "2026-05-02T09:30:00+08:00".into(),
            ),
        ];
        let out = format_butler_tasks_block(&items, 6, 100, fixed_now());
        assert!(out.contains("⏰ 到期"), "due task should carry ⏰ 到期 marker");
        assert!(out.contains("其中 1 条到期"));
        let due_idx = out.find("morning-report").unwrap();
        let plain_idx = out.find("plain-old").unwrap();
        assert!(due_idx < plain_idx, "due task ranks above plain older one");
    }

    fn count_task_lines_with_marker(out: &str) -> usize {
        // Marker only ever appears as the leading "⏰ 到期 · " on a "- " task line.
        // The footer text mentions "⏰ 到期" verbatim as instruction, so we filter
        // strictly to bullet lines.
        out.lines()
            .filter(|l| l.starts_with("- ") && l.contains("⏰ 到期 · "))
            .count()
    }

    #[test]
    fn format_butler_tasks_block_already_done_today_not_due() {
        // Same task as above but updated_at is today after 09:00 → considered served.
        let items = vec![(
            "morning-report".into(),
            "[every: 09:00] write today.md".into(),
            "2026-05-03T09:15:00+08:00".into(),
        )];
        let out = format_butler_tasks_block(&items, 6, 100, fixed_now());
        assert_eq!(count_task_lines_with_marker(&out), 0, "no task line should carry the marker");
        // Header should use the simple form (no "其中 N 条到期" segment).
        let header = out.lines().next().unwrap();
        assert!(!header.contains("条到期"), "header: {}", header);
        assert!(out.contains("morning-report"));
    }

    #[test]
    fn parse_butler_schedule_prefix_parses_every() {
        let (sched, topic) =
            parse_butler_schedule_prefix("[every: 09:00] write today.md").unwrap();
        assert_eq!(sched, ButlerSchedule::Every(9, 0));
        assert_eq!(topic, "write today.md");
    }

    #[test]
    fn parse_butler_schedule_prefix_parses_once() {
        let (sched, topic) =
            parse_butler_schedule_prefix("[once: 2026-05-10 14:00] one-shot").unwrap();
        let expected = chrono::NaiveDate::from_ymd_opt(2026, 5, 10)
            .unwrap()
            .and_hms_opt(14, 0, 0)
            .unwrap();
        assert_eq!(sched, ButlerSchedule::Once(expected));
        assert_eq!(topic, "one-shot");
    }

    #[test]
    fn parse_butler_schedule_prefix_rejects_malformed() {
        assert!(parse_butler_schedule_prefix("no prefix").is_none());
        assert!(parse_butler_schedule_prefix("[every: 25:00] x").is_none());
        assert!(parse_butler_schedule_prefix("[every: 09:60] x").is_none());
        assert!(parse_butler_schedule_prefix("[once: not-a-date] x").is_none());
        assert!(parse_butler_schedule_prefix("[every: 09:00]").is_none(), "empty topic");
        assert!(parse_butler_schedule_prefix("[remind: 09:00] reminder").is_none());
    }

    #[test]
    fn is_butler_due_every_basic_window() {
        let now = fixed_now(); // 2026-05-03 14:30
        // Updated yesterday before 09:00 → today's 09:00 fire hasn't been served → due.
        assert!(is_butler_due(
            &ButlerSchedule::Every(9, 0),
            now,
            "2026-05-02T08:00:00+08:00"
        ));
        // Updated yesterday at 10:00 → already served yesterday's 09:00 fire, but
        // today's 09:00 fire is the most recent (since now=14:30), and updated_at
        // is < today 09:00 → due.
        assert!(is_butler_due(
            &ButlerSchedule::Every(9, 0),
            now,
            "2026-05-02T10:00:00+08:00"
        ));
        // Updated today at 09:30 → already served today's fire → NOT due.
        assert!(!is_butler_due(
            &ButlerSchedule::Every(9, 0),
            now,
            "2026-05-03T09:30:00+08:00"
        ));
    }

    #[test]
    fn is_butler_due_every_before_today_target() {
        // now = 2026-05-03 08:00, target every 09:00 — today's fire hasn't happened yet,
        // so most_recent_fire = yesterday 09:00.
        let now = chrono::NaiveDate::from_ymd_opt(2026, 5, 3)
            .unwrap()
            .and_hms_opt(8, 0, 0)
            .unwrap();
        // Updated yesterday at 09:30 → already served yesterday's fire → not due.
        assert!(!is_butler_due(
            &ButlerSchedule::Every(9, 0),
            now,
            "2026-05-02T09:30:00+08:00"
        ));
        // Updated 2 days ago → before yesterday's fire → due (catching up).
        assert!(is_butler_due(
            &ButlerSchedule::Every(9, 0),
            now,
            "2026-05-01T08:00:00+08:00"
        ));
    }

    #[test]
    fn is_butler_due_once_semantics() {
        let now = fixed_now(); // 2026-05-03 14:30
        let target = ButlerSchedule::Once(
            chrono::NaiveDate::from_ymd_opt(2026, 5, 3)
                .unwrap()
                .and_hms_opt(10, 0, 0)
                .unwrap(),
        );
        // Past target, never updated → due.
        assert!(is_butler_due(&target, now, ""));
        // Past target, updated before target → due.
        assert!(is_butler_due(&target, now, "2026-05-03T09:00:00+08:00"));
        // Past target, updated after target → already done.
        assert!(!is_butler_due(&target, now, "2026-05-03T11:00:00+08:00"));
        // Future target → not yet due regardless of update.
        let future = ButlerSchedule::Once(
            chrono::NaiveDate::from_ymd_opt(2026, 5, 10)
                .unwrap()
                .and_hms_opt(10, 0, 0)
                .unwrap(),
        );
        assert!(!is_butler_due(&future, now, ""));
    }

    #[test]
    fn is_completed_once_basic_flow() {
        // Target was 2026-05-03 10:00. Grace 48h means safe-to-delete at 2026-05-05 10:00.
        let desc = "[once: 2026-05-03 10:00] do something";
        let target_done = "2026-05-03T10:30:00+08:00"; // executed at 10:30, after target
        // 1 hour after target → done but inside grace → keep.
        let now1 = chrono::NaiveDate::from_ymd_opt(2026, 5, 3)
            .unwrap()
            .and_hms_opt(11, 30, 0)
            .unwrap();
        assert!(!is_completed_once(desc, target_done, now1, 48));
        // 49 hours after target → past grace → sweep.
        let now2 = chrono::NaiveDate::from_ymd_opt(2026, 5, 5)
            .unwrap()
            .and_hms_opt(11, 0, 0)
            .unwrap();
        assert!(is_completed_once(desc, target_done, now2, 48));
    }

    #[test]
    fn is_completed_once_not_yet_executed() {
        // Past target but updated_at is before target → not done → keep (still due).
        let desc = "[once: 2026-05-03 10:00] do something";
        let last = "2026-05-02T08:00:00+08:00";
        let now = chrono::NaiveDate::from_ymd_opt(2026, 5, 6).unwrap()
            .and_hms_opt(0, 0, 0).unwrap();
        assert!(!is_completed_once(desc, last, now, 48));
    }

    #[test]
    fn is_completed_once_skips_every_tasks() {
        // every is recurring — sweep must never delete it.
        let desc = "[every: 09:00] daily report";
        let last = "2026-05-03T09:30:00+08:00";
        let now = chrono::NaiveDate::from_ymd_opt(2026, 5, 10).unwrap()
            .and_hms_opt(15, 0, 0).unwrap();
        assert!(!is_completed_once(desc, last, now, 48));
    }

    #[test]
    fn is_completed_once_skips_unprefixed_tasks() {
        let desc = "no schedule prefix here";
        let last = "2026-05-03T09:30:00+08:00";
        let now = chrono::NaiveDate::from_ymd_opt(2026, 5, 10).unwrap()
            .and_hms_opt(0, 0, 0).unwrap();
        assert!(!is_completed_once(desc, last, now, 48));
    }

    #[test]
    fn is_completed_once_unparseable_updated_at_keeps_task() {
        // Bad updated_at → treat as not-yet-executed → keep so user notices.
        let desc = "[once: 2026-05-03 10:00] x";
        let now = chrono::NaiveDate::from_ymd_opt(2026, 6, 1).unwrap()
            .and_hms_opt(0, 0, 0).unwrap();
        assert!(!is_completed_once(desc, "garbage", now, 48));
        assert!(!is_completed_once(desc, "", now, 48));
    }

    #[test]
    fn is_butler_due_unparseable_updated_at_treated_as_never() {
        let now = fixed_now();
        // Garbage updated_at → treat as never-updated → past target = due.
        assert!(is_butler_due(
            &ButlerSchedule::Every(9, 0),
            now,
            "not-a-timestamp"
        ));
        assert!(is_butler_due(&ButlerSchedule::Every(9, 0), now, ""));
    }

    #[test]
    fn prompt_includes_day_of_week_in_time_line() {
        let mut inputs = base_inputs();
        inputs.day_of_week = "周一 · 工作日";
        let p = build_proactive_prompt(&inputs);
        // Time line is the first non-header line; assert the new label is right
        // after the period.
        assert!(p.contains("（下午，周一 · 工作日）"));
    }

    #[test]
    fn user_absence_tier_maps_each_band() {
        assert_eq!(user_absence_tier(0), "用户刚刚还在");
        assert_eq!(user_absence_tier(15), "用户刚刚还在");
        assert_eq!(user_absence_tier(16), "用户离开了一小会儿");
        assert_eq!(user_absence_tier(60), "用户离开了一小会儿");
        assert_eq!(user_absence_tier(61), "用户走开有一两小时了");
        assert_eq!(user_absence_tier(180), "用户走开有一两小时了");
        assert_eq!(user_absence_tier(181), "用户已经离开了大半天");
        assert_eq!(user_absence_tier(480), "用户已经离开了大半天");
        assert_eq!(user_absence_tier(481), "用户一整天没出现");
        assert_eq!(user_absence_tier(1440), "用户一整天没出现");
        assert_eq!(user_absence_tier(1441), "用户至少一天没和你互动");
        assert_eq!(user_absence_tier(99999), "用户至少一天没和你互动");
    }

    #[test]
    fn prompt_includes_idle_register_in_time_line() {
        let mut inputs = base_inputs();
        inputs.idle_register = "用户走开有一两小时了";
        inputs.idle_minutes = 90;
        let p = build_proactive_prompt(&inputs);
        // The register sits inside the parenthetical right after the minute count.
        assert!(p.contains("约 90 分钟（用户走开有一两小时了）"));
    }

    #[test]
    fn weekday_zh_maps_each_weekday() {
        use chrono::Weekday::*;
        assert_eq!(weekday_zh(Mon), "周一");
        assert_eq!(weekday_zh(Tue), "周二");
        assert_eq!(weekday_zh(Wed), "周三");
        assert_eq!(weekday_zh(Thu), "周四");
        assert_eq!(weekday_zh(Fri), "周五");
        assert_eq!(weekday_zh(Sat), "周六");
        assert_eq!(weekday_zh(Sun), "周日");
    }

    #[test]
    fn weekday_kind_zh_distinguishes_weekend() {
        use chrono::Weekday::*;
        for wd in [Mon, Tue, Wed, Thu, Fri] {
            assert_eq!(weekday_kind_zh(wd), "工作日", "{:?} should be 工作日", wd);
        }
        assert_eq!(weekday_kind_zh(Sat), "周末");
        assert_eq!(weekday_kind_zh(Sun), "周末");
    }

    #[test]
    fn format_day_of_week_hint_combines_label_and_kind() {
        use chrono::Weekday::*;
        assert_eq!(format_day_of_week_hint(Sun), "周日 · 周末");
        assert_eq!(format_day_of_week_hint(Mon), "周一 · 工作日");
        assert_eq!(format_day_of_week_hint(Fri), "周五 · 工作日");
        assert_eq!(format_day_of_week_hint(Sat), "周六 · 周末");
    }

    #[test]
    fn format_user_profile_block_no_truncation_when_under_cap() {
        let items = vec![(
            "habit".into(),
            "短描述".into(),
            "2026-05-03T10:00:00+08:00".into(),
        )];
        let out = format_user_profile_block(&items, 6, 80);
        assert!(out.contains("- habit：短描述"));
        assert!(!out.contains("…"));
    }

    #[test]
    fn active_composite_rule_labels_engagement_window_requires_both_signals() {
        // engagement-window: wake_back AND has_plan.
        // long-idle-no-restraint: long_idle (None == long-idle) AND under_chatty AND !pre_quiet.
        // Default for long-idle inputs: Some(8) (short idle), today=0, threshold=5, pre_quiet=false.
        // idle_min = 0 → long-absence-reunion silent throughout this test.
        let short_idle = Some(8u64);
        assert!(active_composite_rule_labels(false, false, short_idle, 0, 5, false, 0).is_empty());
        assert!(active_composite_rule_labels(true, false, short_idle, 0, 5, false, 0).is_empty());
        assert!(active_composite_rule_labels(false, true, short_idle, 0, 5, false, 0).is_empty());
        assert_eq!(
            active_composite_rule_labels(true, true, short_idle, 0, 5, false, 0),
            vec!["engagement-window"],
        );
    }

    #[test]
    fn active_composite_rule_labels_long_idle_requires_three_signals() {
        // long_idle yes, under_chatty yes, !pre_quiet yes → fires.
        assert_eq!(
            active_composite_rule_labels(false, false, Some(LONG_IDLE_MINUTES), 0, 5, false, 0),
            vec!["long-idle-no-restraint"],
        );
        // None (never spoken) is treated as long-idle.
        assert_eq!(
            active_composite_rule_labels(false, false, None, 0, 5, false, 0),
            vec!["long-idle-no-restraint"],
        );
        // Short idle (< threshold) → no fire.
        assert!(active_composite_rule_labels(false, false, Some(LONG_IDLE_MINUTES - 1), 0, 5, false, 0).is_empty());
        // chatty (today >= threshold) → no fire.
        assert!(active_composite_rule_labels(false, false, Some(120), 5, 5, false, 0).is_empty());
        // pre_quiet active → no fire.
        assert!(active_composite_rule_labels(false, false, Some(120), 0, 5, true, 0).is_empty());
        // Threshold == 0 disables chatty gate, so under_chatty is always true.
        assert_eq!(
            active_composite_rule_labels(false, false, Some(120), 9999, 0, false, 0),
            vec!["long-idle-no-restraint"],
        );
    }

    #[test]
    fn active_composite_rule_labels_both_can_fire_together() {
        // wake_back + has_plan + long_idle + under_chatty + !pre_quiet → both labels.
        let labels = active_composite_rule_labels(true, true, Some(LONG_IDLE_MINUTES), 0, 5, false, 0);
        assert_eq!(labels, vec!["engagement-window", "long-idle-no-restraint"]);
    }

    #[test]
    fn active_composite_rule_labels_long_absence_reunion_gates() {
        // Threshold boundary — at threshold + under_chatty + !pre_quiet → fires.
        assert_eq!(
            active_composite_rule_labels(
                false,
                false,
                Some(8),
                0,
                5,
                false,
                LONG_ABSENCE_MINUTES,
            ),
            vec!["long-absence-reunion"],
        );
        // Just below threshold → silent.
        assert!(active_composite_rule_labels(
            false,
            false,
            Some(8),
            0,
            5,
            false,
            LONG_ABSENCE_MINUTES - 1,
        )
        .is_empty());
        // chatty active → silent (don't pile reunion onto a chatty day).
        assert!(active_composite_rule_labels(
            false,
            false,
            Some(8),
            5,
            5,
            false,
            LONG_ABSENCE_MINUTES + 60,
        )
        .is_empty());
        // pre_quiet active → silent (don't open new register before quiet hours).
        assert!(active_composite_rule_labels(
            false,
            false,
            Some(8),
            0,
            5,
            true,
            LONG_ABSENCE_MINUTES + 60,
        )
        .is_empty());
    }

    #[test]
    fn active_composite_rule_labels_three_can_coexist() {
        // wake_back + has_plan + long_idle + under_chatty + !pre_quiet + long_absence
        // → all three labels emit in order: engagement-window, long-idle-no-restraint,
        //   long-absence-reunion.
        let labels = active_composite_rule_labels(
            true,
            true,
            Some(LONG_IDLE_MINUTES),
            0,
            5,
            false,
            LONG_ABSENCE_MINUTES + 60,
        );
        assert_eq!(
            labels,
            vec!["engagement-window", "long-idle-no-restraint", "long-absence-reunion"],
        );
    }

    /// Read panelTypes.ts and return the kebab/snake keys defined in
    /// PROMPT_RULE_DESCRIPTIONS. Used by both alignment tests so the parsing logic
    /// only lives in one place. Plain string scanning — no regex dep — and tolerant
    /// of both quoted (`"wake-back": {`) and bare-identifier (`plan: {`) keys.
    ///
    /// Iter 98: relocated from PanelDebug.tsx to panelTypes.ts so PanelDebug owns only
    /// state + layout. If the dict moves again, update this path and the panic
    /// messages below in lockstep.
    fn parse_prompt_rule_dict_keys() -> Vec<String> {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
            .expect("CARGO_MANIFEST_DIR is set during cargo test runs");
        let panel_path = std::path::PathBuf::from(manifest_dir)
            .join("..")
            .join("src")
            .join("components")
            .join("panel")
            .join("panelTypes.ts");
        let panel_src = std::fs::read_to_string(&panel_path)
            .unwrap_or_else(|e| panic!("read panelTypes.ts: {}", e));
        let mut keys: Vec<String> = Vec::new();
        let mut in_dict = false;
        for line in panel_src.lines() {
            // Accept both `const ...` and `export const ...` forms — Iter 97 made the
            // dict an export so a sibling component can import it.
            if line.starts_with("const PROMPT_RULE_DESCRIPTIONS")
                || line.starts_with("export const PROMPT_RULE_DESCRIPTIONS")
            {
                in_dict = true;
                continue;
            }
            if !in_dict {
                continue;
            }
            let trimmed = line.trim();
            if trimmed == "};" {
                break;
            }
            // Each top-level entry starts with `<key>: {` (key value is itself a {…}
            // object literal). Inner lines use `title: "..."` / `summary: "..."` and
            // don't match `: {` so they're naturally skipped.
            if let Some(idx) = trimmed.find(": {") {
                let raw = trimmed[..idx].trim().trim_matches('"');
                if !raw.is_empty()
                    && raw
                        .chars()
                        .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
                {
                    keys.push(raw.to_string());
                }
            }
        }
        keys
    }

    #[test]
    fn proactive_rules_has_match_arm_for_every_backend_label() {
        // Iter 91 / Iter 93 — guard the proactive_rules match against the helper label
        // set. If a label ships from a helper but proactive_rules has no arm for it,
        // the fallback `(规则文本待补)` slips into the prompt — visible in panel logs
        // but easy to miss in CI. This test fails immediately instead.
        //
        // Some labels are mutually exclusive (chatty vs long-idle-no-restraint share the
        // chatty threshold; pre-quiet excludes long-idle). We run two scenarios and
        // require each fingerprint to fire in at least one of them — combined coverage
        // still pins down the full match arm set.
        let fingerprints: &[(&str, &str)] = &[
            ("wake-back", "用户刚从离开桌子回来"),
            ("first-mood", "第一次开口"),
            ("first-of-day", "今天的第一次开口"),
            ("pre-quiet", "快进入安静时段"),
            ("reminders", "有到期的用户提醒"),
            ("plan", "你有今日计划在执行中"),
            ("icebreaker", "你和用户还不熟"),
            ("chatty", "今天已经聊了不少"),
            ("companionship-milestone", "今天是和用户相处的"),
            ("env-awareness", "最近你开口前几乎都没看环境"),
            ("engagement-window", "此刻是开新话题的好时机"),
            ("long-idle-no-restraint", "沉默已久但没在克制状态"),
            ("long-absence-reunion", "用户离开了不短的时间"),
        ];

        // Sanity: the fingerprint table must cover every label the helpers emit. Use
        // each helper's max-trigger inputs to enumerate the universe.
        let backend_labels: std::collections::HashSet<&'static str> =
            active_environmental_rule_labels(true, true, true, true, true, true)
                .into_iter()
                .chain(active_data_driven_rule_labels(0, 999, 1, 999, 0, 100))
                .chain(active_composite_rule_labels(true, true, Some(120), 0, 5, false, LONG_ABSENCE_MINUTES + 60))
                .collect();
        let fingerprint_labels: std::collections::HashSet<&str> =
            fingerprints.iter().map(|(l, _)| *l).collect();
        let untested: Vec<&&'static str> = backend_labels
            .iter()
            .filter(|l| !fingerprint_labels.contains(*l))
            .collect();
        assert!(
            untested.is_empty(),
            "fingerprint table missing entries for backend labels: {:?}. Add a row \
             with a substring unique to that label's rule text.",
            untested
        );

        // Scenario 1: short-idle + chatty active + pre-quiet active.
        // Covers: wake-back, first-mood, pre-quiet, reminders, plan, icebreaker, chatty,
        // env-awareness, engagement-window, companionship-milestone.
        let mut s1 = base_inputs();
        s1.wake_hint = "（用户的电脑在大约 60 秒前刚从休眠唤醒。）";
        s1.is_first_mood = true;
        s1.pre_quiet_minutes = Some(10);
        s1.reminders_hint = "你有以下到期的用户提醒：\n· something";
        s1.plan_hint = "你今天的小目标：\n· something";
        s1.proactive_history_count = 0;
        s1.today_speech_count = s1.chatty_day_threshold;
        s1.env_spoke_total = 12;
        s1.env_spoke_with_any = 1;
        s1.companionship_days = 100; // milestone — fires companionship-milestone
        // since_last_proactive_minutes stays at base_inputs default (Some(8)) — short.
        let rules1 = proactive_rules(&s1);

        // Scenario 2: long-idle + under-chatty + !pre-quiet + long-absence.
        // Covers: long-idle-no-restraint, long-absence-reunion, plus everything
        // except chatty / pre-quiet.
        let mut s2 = s1;
        s2.pre_quiet_minutes = None;
        s2.today_speech_count = 0;
        s2.since_last_proactive_minutes = Some(LONG_IDLE_MINUTES);
        s2.idle_minutes = LONG_ABSENCE_MINUTES + 60;
        s2.idle_register = "用户已经离开了大半天";
        let rules2 = proactive_rules(&s2);

        let combined: Vec<&String> = rules1.iter().chain(rules2.iter()).collect();
        assert!(
            !combined.iter().any(|r| r.contains("规则文本待补")),
            "proactive_rules emitted the unknown-label fallback. A helper added a \
             label without a matching arm in proactive_rules.\nRules1: {:#?}\nRules2: {:#?}",
            rules1,
            rules2,
        );
        for (label, fp) in fingerprints {
            assert!(
                combined.iter().any(|r| r.contains(fp)),
                "label '{}' should produce a rule containing '{}' in at least one of \
                 the two scenarios, but neither matched. Either the arm is missing \
                 or its text changed.",
                label,
                fp
            );
        }
    }

    #[test]
    fn frontend_prompt_rule_descriptions_have_no_ghost_labels() {
        // Iter 90 — reverse of Iter 89's coverage check. A "ghost" entry is a key in
        // PROMPT_RULE_DESCRIPTIONS that no backend label helper ever returns; it
        // wastes dictionary space and quietly hides a UI string nobody can ever see.
        // Catches dead translations left behind after a backend rule was renamed or
        // removed.
        let frontend_keys = parse_prompt_rule_dict_keys();
        assert!(
            !frontend_keys.is_empty(),
            "PROMPT_RULE_DESCRIPTIONS dictionary appears empty — parser may be broken \
             or the dict was emptied. Iter 89's test should also be failing."
        );
        // All-true / max-trigger inputs surface every label all backend helpers can
        // ever produce. Same input recipe as Iter 89's test for consistency.
        let env = active_environmental_rule_labels(true, true, true, true, true, true);
        let data = active_data_driven_rule_labels(0, 999, 1, 999, 0, 100);
        let composite = active_composite_rule_labels(true, true, Some(120), 0, 5, false, LONG_ABSENCE_MINUTES + 60);
        let backend: std::collections::HashSet<&'static str> = env
            .iter()
            .chain(data.iter())
            .chain(composite.iter())
            .copied()
            .collect();
        let ghosts: Vec<&str> = frontend_keys
            .iter()
            .filter(|k| !backend.contains(k.as_str()))
            .map(|s| s.as_str())
            .collect();
        assert!(
            ghosts.is_empty(),
            "PROMPT_RULE_DESCRIPTIONS contains ghost keys with no backend producer: \
             {:?}. Either remove them, or add the corresponding backend label.",
            ghosts
        );
    }

    #[test]
    fn frontend_prompt_rule_descriptions_cover_every_backend_label() {
        // Iter 89 — guard the contract between the Rust label helpers and the frontend
        // PROMPT_RULE_DESCRIPTIONS dictionary in PanelDebug.tsx. If either side adds a
        // label without updating the other, the panel falls back to "(label X 暂无中文
        // 描述)" — visible but easy to miss in dev. This test fails CI instead.
        //
        // Iter 90 unified the parsing path: both this test and the ghost-keys test go
        // through `parse_prompt_rule_dict_keys`, so they validate against the same
        // canonical key set extracted from the dictionary block.
        let frontend_keys = parse_prompt_rule_dict_keys();
        assert!(
            !frontend_keys.is_empty(),
            "PROMPT_RULE_DESCRIPTIONS dictionary not found or empty in panelTypes.ts \
             — the frontend may have moved or renamed it. Update parse_prompt_rule_dict_keys."
        );
        let frontend_set: std::collections::HashSet<&str> =
            frontend_keys.iter().map(|s| s.as_str()).collect();
        // All-true inputs surface every possible label all helpers can ever return.
        let env = active_environmental_rule_labels(true, true, true, true, true, true);
        let data = active_data_driven_rule_labels(0, 999, 1, 999, 0, 100);
        let composite = active_composite_rule_labels(true, true, Some(120), 0, 5, false, LONG_ABSENCE_MINUTES + 60);
        let missing: Vec<&'static str> = env
            .iter()
            .chain(data.iter())
            .chain(composite.iter())
            .filter(|l| !frontend_set.contains(*l))
            .copied()
            .collect();
        assert!(
            missing.is_empty(),
            "panelTypes.ts PROMPT_RULE_DESCRIPTIONS missing entries for backend \
             labels: {:?}. Add a {{title, summary, nature}} row for each.",
            missing
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
        // pre-quiet on AND long-idle are mutually exclusive by design (long-idle
        // requires !pre_quiet). Likewise chatty (today_count >= threshold) and
        // long-idle exclude each other. This scenario: pre-quiet on, today=0
        // (no chatty AND first-of-day fires — Iter Cξ added that 6th env label),
        // long-idle won't fire — so we expect:
        // 6 base + 6 env (wake-back/first-mood/first-of-day/pre-quiet/reminders/plan)
        // + 2 data (icebreaker + env-awareness) + 1 composite (engagement-window)
        // = 15. The fingerprint test below covers long-idle / chatty / long-absence
        // in a separate scenario, so combined coverage is still complete.
        inputs.today_speech_count = 0;
        let rules = proactive_rules(&inputs);
        assert_eq!(rules.len(), 6 + 6 + 2 + 1, "rules: {:#?}", rules);
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
