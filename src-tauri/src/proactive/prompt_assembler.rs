//! Proactive-prompt assembler (Iter QG5c2 extraction from `proactive.rs`).
//!
//! Where `prompt_rules.rs` produces *which* rule labels are active, this
//! module renders the actual prompt text:
//! - `PromptInputs`: the typed bag of every signal the prompt consumes.
//! - `proactive_rules`: maps active labels → rule-text bullets, with
//!   per-rule formatting that interpolates input numbers.
//! - `build_proactive_prompt`: composes the full prompt sections (time
//!   line, mood, companionship, hints, rules block) in stable order.
//!
//! Plus two pure prompt-section formatters (`format_proactive_mood_hint`,
//! `format_plan_hint`) and the silent-reply marker the LLM uses to opt out
//! of speaking on a turn.
//!
//! Tests stay in `proactive::prompt_tests` — they continue to resolve
//! everything moved here via the glob `pub use self::prompt_assembler::*`
//! at the top of `proactive.rs`.

use super::{
    active_composite_rule_labels, active_data_driven_rule_labels, active_environmental_rule_labels,
    companionship_milestone, format_companionship_line, ENV_AWARENESS_LOW_RATE_PCT,
    LONG_ABSENCE_MINUTES, LONG_IDLE_MINUTES,
};
use crate::mood::{MOOD_CATEGORY, MOOD_TITLE};

/// Reply marker the LLM emits when it judges the right move is to stay
/// silent this turn. The proactive pipeline checks the trimmed reply for
/// this string — anything containing it is treated as "no speech" and the
/// pet skips the turn without writing to speech_history.
pub const SILENT_MARKER: &str = "<silent>";

/// Bag of every signal the proactive prompt depends on. Adding a new prompt
/// hint is now: (a) extend this struct, (b) push it via `push_if_nonempty` in
/// `build_proactive_prompt`. Tests use `base_inputs()` (in `prompt_tests`)
/// to mint a neutral default and override only the fields they exercise, so
/// the builder function has a clean signature and tests can inject specific
/// values without threading individual arguments.
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
    /// Iter R1: feedback from the previous proactive turn. Empty when there's
    /// no prior turn or the prior is still ambiguous; non-empty carries a
    /// one-line nudge ("上次你说『...』，用户没回应 — ...") so the LLM
    /// learns from outcomes round to round. Built by
    /// `feedback_history::format_feedback_hint`.
    pub feedback_hint: &'a str,
    /// Iter R26: aggregate of recent feedback (last 20 entries) — gives the
    /// LLM a "trend" picture to complement `feedback_hint`'s last-event
    /// signal. Empty when fewer than `FEEDBACK_AGGREGATE_MIN_SAMPLES`
    /// entries (signal too thin to bias prompt). Built by
    /// `feedback_history::format_feedback_aggregate_hint`.
    pub feedback_aggregate_hint: &'a str,
    /// Iter R33: trailing-silent streak nudge — "你已经连续 N 次选择沉默
    /// 了。..." Empty when streak < `SILENT_STREAK_THRESHOLD`. Built by
    /// `telemetry::format_consecutive_silent_hint(count_trailing_silent(...))`.
    /// Breaks perpetual-silence loops where LLM keeps choosing silent
    /// because nothing feels noteworthy enough.
    pub consecutive_silent_hint: &'a str,
    /// Iter R35: mirror on user-feedback side — trailing-negative streak
    /// ("你最近连续 N 次开口都被忽略或点掉"). Empty when streak <
    /// `NEGATIVE_STREAK_THRESHOLD`. Built by
    /// `feedback_history::format_consecutive_negative_hint(count_trailing_negative(...))`.
    /// Differs from R26 aggregate hint: R26 = 20-window ratio (smoothed),
    /// R35 = uninterrupted recent run (urgency).
    pub consecutive_negative_hint: &'a str,
    /// Iter R55: transient instruction note from user. Pre-formatted with
    /// "[临时指示]" prefix when active; empty when no note set or expired.
    /// Built by `gate::transient_note_active()` + caller-side wrapping.
    pub transient_note_hint: &'a str,
    /// Iter R3: current local hour (0-23). Used by composite rules that need
    /// time-of-day specificity beyond the coarse `period` label — currently
    /// the late-night-wellness rule (0:00-3:59 + active idle).
    pub hour: u8,
    /// Iter R8: caller-side flag indicating the late-night-wellness rule
    /// fired within `LATE_NIGHT_WELLNESS_MIN_GAP_SECONDS`. When true, the
    /// rule label is suppressed even if hour/idle still satisfy the trigger,
    /// preventing repeat 30-min "该睡了" pings during a single overnight
    /// session. Production code computes via `late_night_wellness_in_cooldown()`;
    /// tests pass false unless deliberately exercising the gate.
    pub recently_fired_wellness: bool,
    /// Iter R11: when the speech-redundancy detector flags a topic
    /// repeated across recent utterances, this carries the nudge text
    /// (e.g. "你最近多次提到「工作进展」，这次换个角度"). Empty when no
    /// repetition is detected. Built by run_proactive_turn from
    /// `speech_history::detect_repeated_topic`.
    pub repeated_topic_hint: &'a str,
    /// Iter R14: cross-day continuity hint — at the first proactive turn
    /// of a new day, surfaces yesterday's last utterance(s) so the pet
    /// can pick up a thread instead of starting cold every morning.
    /// Empty when (a) it's not the first turn of today, or (b) yesterday
    /// has no recorded speeches. Built by run_proactive_turn from
    /// `speech_history::speeches_for_date_async`.
    pub cross_day_hint: &'a str,
    /// Iter R15: active-app duration hint — "用户在「Cursor」里已经待了 N 分钟"
    /// when the user has been on the same foreground app for ≥
    /// `MIN_DURATION_MINUTES`. Empty when below threshold, app unchanged
    /// less than that, or active-window read failed. Built from
    /// `active_app::update_and_format_active_app_hint`.
    pub active_app_hint: &'a str,
    /// Iter R16: yesterday-recap hint — "[昨日总览] 我们昨天主动开口 N 次，
    /// 计划 X/Y。" Sourced from yesterday's `daily_review_YYYY-MM-DD`
    /// memory description (written by R12 the previous evening) and
    /// reframed past-tense by `format_yesterday_recap_hint`. Fires only
    /// at first-of-day (alongside `cross_day_hint`); empty when no
    /// review exists or the description isn't `[review]`-prefixed.
    pub yesterday_recap_hint: &'a str,
    /// Iter R19: length-register variance nudge — "你最近 N 句开口都偏长
    /// （平均 X 字）..." or "...偏短..." when recent speeches are stuck
    /// in one register. Empty when fewer than 3 samples or the recent
    /// register is mixed (already varying). Built from
    /// `speech_history::format_speech_length_hint`.
    pub length_register_hint: &'a str,
    /// Iter R63: deep-focus recovery hint — "[刚结束深度专注] 用户刚从
    /// 「X」的 N 分钟连续专注里切出来..." Fires on the first proactive
    /// turn that runs within RECOVERY_HINT_GRACE_SECS (10 min) after a
    /// hard-block stretch ends. Empty when no recent block or already
    /// taken (single-shot per block stretch). Built from
    /// `active_app::take_recovery_hint`.
    pub deep_focus_recovery_hint: &'a str,
}

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
        inputs.hour,
        inputs.recently_fired_wellness,
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
            "late-night-wellness" => format!(
                "- **深夜还在用电脑**：现在已经 {} 点了，用户键鼠还活跃（{} 分钟内有动作）。这是 wellness 优先的时刻——这次开口请直接关心 ta 该休息了（「哎，{} 点了还在忙啊？该睡了」「夜深了，再不睡明天会累」之类）。语气暖但坚定，**不要**起新话题、不要追问工作进展、不要长篇——一句关心 + 一句「该睡了」就好。如果 ta 已经在做明显是收尾的事（关掉 IDE、回邮件等），可以更轻盈地说一声晚安。",
                inputs.hour, inputs.idle_minutes, inputs.hour
            ),
            // Unknown label means a helper added something proactive_rules doesn't know
            // about — log defensively rather than panic, so the prompt still ships.
            other => format!("- **[{}]**: (规则文本待补)", other),
        };
        rules.push(rule);
    }
    rules
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
    push_if_nonempty(&mut s, inputs.feedback_hint);
    push_if_nonempty(&mut s, inputs.feedback_aggregate_hint);
    push_if_nonempty(&mut s, inputs.consecutive_silent_hint);
    push_if_nonempty(&mut s, inputs.consecutive_negative_hint);
    push_if_nonempty(&mut s, inputs.transient_note_hint);
    push_if_nonempty(&mut s, inputs.repeated_topic_hint);
    push_if_nonempty(&mut s, inputs.yesterday_recap_hint);
    push_if_nonempty(&mut s, inputs.cross_day_hint);
    push_if_nonempty(&mut s, inputs.active_app_hint);
    push_if_nonempty(&mut s, inputs.deep_focus_recovery_hint);
    push_if_nonempty(&mut s, inputs.length_register_hint);
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

/// Pure formatter for the proactive prompt's mood-hint line. Empty / whitespace text
/// emits the "first time" placeholder; otherwise the recorded mood text is wrapped
/// in 「…」 quotes after passing through `redact`. Symmetric with chat.rs's
/// `inject_mood_note`, which does its own (already-redacted) mood injection for
/// reactive chats — keeps the two entry points consistent.
///
/// Iter QG4: previously inline in `run_proactive_turn` and emitted raw mood text,
/// allowing privacy-pattern terms to re-leak whenever the proactive loop fired.
pub fn format_proactive_mood_hint(text: &str, redact: &dyn Fn(&str) -> String) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        "（还没有记录过你自己的心情/状态。这是第一次。）".to_string()
    } else {
        format!("你上次记录的心情/状态：「{}」。", redact(trimmed))
    }
}

/// Pure formatter for the daily-plan hint block. `redact` is applied to the plan
/// description before it's wrapped in the header line, so plan items the LLM wrote
/// (which may have absorbed user-private terms during reactive turns) don't leak
/// back into subsequent proactive prompts. Empty / whitespace-only description
/// returns empty string.
///
/// Iter QG4: gained the redaction pass — previously the description was inserted
/// verbatim, the only one of the three reinjection sites where the LLM itself was
/// the original author of the unredacted content.
pub fn format_plan_hint(description: &str, redact: &dyn Fn(&str) -> String) -> String {
    let trimmed = description.trim();
    if trimmed.is_empty() {
        String::new()
    } else {
        format!("你今天的小目标 / 计划：\n{}", redact(trimmed))
    }
}
