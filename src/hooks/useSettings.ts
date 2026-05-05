import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";

export interface McpServerConfig {
  transport: "stdio" | "sse" | "http";
  command: string;
  args: string[];
  url: string;
  headers: Record<string, string>;
  env: Record<string, string>;
  enabled: boolean;
}

export interface TgCustomCommand {
  name: string;
  description: string;
}

export interface TelegramConfig {
  bot_token: string;
  allowed_username: string;
  enabled: boolean;
  persona_layer_enabled: boolean;
  /** 用户自定义命令名 + 描述，bot 启动时与硬编码合并注册到 TG 客户端补全
   * 表。调用时不绑定具体 tool —— 直接走 chat pipeline 让 LLM 自由处理。 */
  custom_commands: TgCustomCommand[];
  /** TG 客户端补全表里硬编码命令的描述语种：`"zh"`（默认）/ `"en"`。
   * 自定义命令描述用户自填，**不**翻译。其它运行时反馈仍中文。 */
  command_lang: string;
}

export interface ProactiveConfig {
  enabled: boolean;
  interval_seconds: number;
  idle_threshold_seconds: number;
  input_idle_seconds: number;
  cooldown_seconds: number;
  quiet_hours_start: number;
  quiet_hours_end: number;
  respect_focus_mode: boolean;
  chatty_day_threshold: number;
  /** Iter R13 / R29: high-level dial layered over cooldown_seconds and
   * chatty_day_threshold. "balanced" = no change, "chatty" = ×0.5 cooldown
   * + ×2 chatty threshold (talkative day), "quiet" = ×2 cooldown + ×0.5
   * chatty (low-key day). Backend default is "balanced". */
  companion_mode: string;
  /** 长任务心跳阈值（分钟）。pending 的 butler_tasks 条目若被宠物触
   * 碰过且距离上次更新 ≥ 该阈值，会在下一次 proactive prompt 里被
   * 「[心跳]」段点名提醒。0 = 关闭。后端默认 30。配置目前只能在 yaml
   * 里改，UI 暂未暴露。 */
  task_heartbeat_minutes: number;
}

/** 早安简报：每日固定时刻让宠物主动播报天气/日程/未读提醒/昨日回顾的合
 * 成段落。开关与时刻独立于常规 proactive — 用户可能关闭常规主动发言但
 * 仍保留每日早安。后端默认 `enabled: true, hour: 8, minute: 30`。 */
export interface MorningBriefingConfig {
  enabled: boolean;
  hour: number;
  minute: number;
}

export interface MemoryConsolidateConfig {
  enabled: boolean;
  interval_hours: number;
  min_total_items: number;
  stale_reminder_hours: number;
  stale_plan_hours: number;
  stale_once_butler_hours: number;
  /** Iter R17 / R30: how many days a `daily_review_YYYY-MM-DD` entry
   * lingers in `ai_insights` before consolidate prunes it. 0 disables
   * pruning (entries kept indefinitely). Default 30. */
  stale_daily_review_days: number;
  /** 周报合成的"周日 closing 时刻"（小时 0-23）。该时刻之后下次
   * consolidate loop 唤醒触发周报合成。0 = 关闭。默认 20。与 enabled
   * 解耦 — 即便 consolidate 整体被禁用仍按时合成周报。 */
  weekly_summary_closing_hour: number;
}

export interface ChatConfig {
  max_context_messages: number;
}

export interface PrivacyConfig {
  redaction_patterns: string[];
  regex_patterns: string[];
}

export interface AppSettings {
  live_2d_model_path: string;
  api_base: string;
  api_key: string;
  model: string;
  mcp_servers: Record<string, McpServerConfig>;
  telegram: TelegramConfig;
  proactive: ProactiveConfig;
  morning_briefing: MorningBriefingConfig;
  memory_consolidate: MemoryConsolidateConfig;
  chat: ChatConfig;
  privacy: PrivacyConfig;
  user_name: string;
  /** 工具审核覆盖：键是工具名，值是 "auto" / "always_review" /
   * "always_approve"。未列出的工具按 auto。值字符串而非联合类型，
   * 让前向兼容自然成立 — 后端 parse_mode 不识别值会退回 auto。 */
  tool_review_overrides: Record<string, string>;
  /** Live2D motion 自定义映射：键是语义键（Tap / Flick / Flick3 / Idle），
   * 值是当前模型的实际 motion group 名。空 / 缺省 = 用语义键当 group 名。
   * 在 useMoodAnimation 触发动画时翻译；LLM 协议不动。 */
  motion_mapping: Record<string, string>;
}

const DEFAULT_SETTINGS: AppSettings = {
  live_2d_model_path: "/models/miku/miku.model3.json",
  api_base: "https://api.openai.com/v1",
  api_key: "",
  model: "gpt-4o-mini",
  mcp_servers: {},
  telegram: { bot_token: "", allowed_username: "", enabled: false, persona_layer_enabled: true, custom_commands: [], command_lang: "zh" },
  proactive: {
    enabled: false,
    interval_seconds: 300,
    idle_threshold_seconds: 900,
    input_idle_seconds: 60,
    cooldown_seconds: 1800,
    quiet_hours_start: 23,
    quiet_hours_end: 7,
    respect_focus_mode: true,
    chatty_day_threshold: 5,
    companion_mode: "balanced",
    task_heartbeat_minutes: 30,
  },
  morning_briefing: {
    enabled: true,
    hour: 8,
    minute: 30,
  },
  memory_consolidate: {
    enabled: false,
    interval_hours: 6,
    min_total_items: 12,
    stale_reminder_hours: 24,
    stale_plan_hours: 24,
    stale_once_butler_hours: 48,
    stale_daily_review_days: 30,
    weekly_summary_closing_hour: 20,
  },
  chat: {
    max_context_messages: 50,
  },
  privacy: {
    redaction_patterns: [],
    regex_patterns: [],
  },
  user_name: "",
  tool_review_overrides: {},
  motion_mapping: {},
};

export function useSettings() {
  const [settings, setSettings] = useState<AppSettings>(DEFAULT_SETTINGS);
  const [soul, setSoul] = useState("");
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    Promise.all([
      invoke<AppSettings>("get_settings"),
      invoke<string>("get_soul"),
    ])
      .then(([s, soulContent]) => {
        setSettings(s);
        setSoul(soulContent);
        setLoaded(true);
      })
      .catch((e) => {
        console.error("Failed to load settings:", e);
        setLoaded(true);
      });
  }, []);

  const updateSettings = useCallback(async (newSettings: AppSettings) => {
    await invoke("save_settings", { settings: newSettings });
    setSettings(newSettings);
  }, []);

  const updateSoul = useCallback(async (content: string) => {
    await invoke("save_soul", { content });
    setSoul(content);
  }, []);

  return { settings, soul, loaded, updateSettings, updateSoul };
}
