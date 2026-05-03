/**
 * Shared types and dictionaries for the panel UI (Iter 98).
 *
 * Originally these lived in PanelDebug.tsx. Pulling them out lets PanelDebug focus on
 * state + layout, lets PanelChipStrip and any future panel components import a single
 * canonical source, and gives the cargo alignment tests (Iter 89/90/91) one stable
 * file to scan for `PROMPT_RULE_DESCRIPTIONS` keys.
 *
 * Adding a new contextual rule remains: (a) add a backend label helper, (b) add a
 * match arm in `proactive_rules`, (c) add a row to PROMPT_RULE_DESCRIPTIONS here.
 */

export interface CacheStats {
  turns: number;
  total_hits: number;
  total_calls: number;
}

export interface ProactiveDecision {
  timestamp: string;
  kind: string;
  reason: string;
}

export interface MoodTagStats {
  with_tag: number;
  without_tag: number;
  no_mood: number;
}

export interface LlmOutcomeStats {
  spoke: number;
  silent: number;
  error: number;
}

export interface EnvToolStats {
  spoke_total: number;
  spoke_with_any: number;
  active_window: number;
  weather: number;
  upcoming_events: number;
  memory_search: number;
}

export interface PromptTiltStats {
  restraint_dominant: number;
  engagement_dominant: number;
  balanced: number;
  neutral: number;
}

export interface RedactionStats {
  calls: number;
  hits: number;
}

export interface PendingReminder {
  time: string;
  topic: string;
  title: string;
  due_now: boolean;
}

export interface ToneSnapshot {
  period: string;
  cadence: string | null;
  since_last_proactive_minutes: number | null;
  wake_seconds_ago: number | null;
  mood_text: string | null;
  mood_motion: string | null;
  pre_quiet_minutes: number | null;
  proactive_count: number;
  chatty_day_threshold: number;
  active_prompt_rules: string[];
}

/**
 * Each backend prompt rule label has a "nature" describing the *kind* of guidance it
 * pushes at the LLM. Lets the panel show "you've got 3 restraint hints + 2 engagement
 * hints active" as an at-a-glance prompt-tilt summary.
 *
 * - restraint: tells the pet to stay quiet, brief, or low-key.
 * - engagement: encourages the pet to open up / take initiative.
 * - corrective: addresses a past behavioral pattern (e.g., ignoring tools).
 * - instructional: prescribes a specific operation when the pet does speak.
 */
export type PromptRuleNature = "restraint" | "engagement" | "corrective" | "instructional";

export const PROMPT_RULE_DESCRIPTIONS: Record<
  string,
  { title: string; summary: string; nature: PromptRuleNature }
> = {
  "wake-back": {
    title: "刚回桌",
    summary: "用户的电脑刚从休眠唤醒；问候要简短克制，先轻打招呼。",
    nature: "restraint",
  },
  "first-mood": {
    title: "首次开口",
    summary: "还没有 mood 记忆条目；开口后用 memory_edit create 初始化。",
    nature: "instructional",
  },
  "pre-quiet": {
    title: "近安静时段",
    summary: "再过几分钟到夜里安静时段；语气往收尾靠，简短晚安/睡前关心。",
    nature: "restraint",
  },
  reminders: {
    title: "到期提醒",
    summary: "用户设置的 todo 到期了；自然带进开口里，并 memory_edit delete。",
    nature: "instructional",
  },
  plan: {
    title: "今日计划",
    summary: "ai_insights/daily_plan 有未完成项；优先推进一条并 update 进度。",
    nature: "instructional",
  },
  icebreaker: {
    title: "破冰阶段",
    summary: "之前主动开口 < 3 次；偏向问简短低压力的了解性问题。",
    nature: "restraint",
  },
  chatty: {
    title: "今日克制",
    summary: "今天已经聊得不少；除非有新信号否则保持安静或极简一句。",
    nature: "restraint",
  },
  "env-awareness": {
    title: "环境感知低",
    summary: "近几次开口很少看环境；本次先调 get_active_window 看用户在做啥。",
    nature: "corrective",
  },
  "engagement-window": {
    title: "积极开口",
    summary: "刚回桌 + 有今日 plan：是「先关心、再带 plan」串起来的复合时机。",
    nature: "engagement",
  },
  "long-idle-no-restraint": {
    title: "久未开口",
    summary: "≥ 60min 没主动说话 + 不在克制态：找个贴合用户当下的轻话题。",
    nature: "engagement",
  },
};

export const NATURE_META: Record<PromptRuleNature, { label: string; color: string }> = {
  restraint: { label: "克制", color: "#dc2626" },
  engagement: { label: "引导", color: "#16a34a" },
  corrective: { label: "校正", color: "#ea580c" },
  instructional: { label: "操作", color: "#0891b2" },
};
