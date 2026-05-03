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

export interface TelegramConfig {
  bot_token: string;
  allowed_username: string;
  enabled: boolean;
  persona_layer_enabled: boolean;
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
}

export interface MemoryConsolidateConfig {
  enabled: boolean;
  interval_hours: number;
  min_total_items: number;
  stale_reminder_hours: number;
  stale_plan_hours: number;
  stale_once_butler_hours: number;
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
  memory_consolidate: MemoryConsolidateConfig;
  chat: ChatConfig;
  privacy: PrivacyConfig;
  user_name: string;
}

const DEFAULT_SETTINGS: AppSettings = {
  live_2d_model_path: "/models/miku/miku.model3.json",
  api_base: "https://api.openai.com/v1",
  api_key: "",
  model: "gpt-4o-mini",
  mcp_servers: {},
  telegram: { bot_token: "", allowed_username: "", enabled: false, persona_layer_enabled: true },
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
  },
  memory_consolidate: {
    enabled: false,
    interval_hours: 6,
    min_total_items: 12,
    stale_reminder_hours: 24,
    stale_plan_hours: 24,
    stale_once_butler_hours: 48,
  },
  chat: {
    max_context_messages: 50,
  },
  privacy: {
    redaction_patterns: [],
    regex_patterns: [],
  },
  user_name: "",
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
