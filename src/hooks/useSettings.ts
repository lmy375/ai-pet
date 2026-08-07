import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTauriEvent } from "./useTauriEvent";

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
}

/** One configurable agent: its own model, persona/memory, MCP, telegram, heartbeat. */
export interface AgentConfig {
  id: string;
  name: string;
  api_base: string;
  api_key: string;
  model: string;
  context_window: number;
  /** OpenAI-style reasoning effort ("minimal"/"low"/"medium"/"high"); "" = omit. */
  reasoning_effort: string;
  /** Request Anthropic extended thinking (Claude models). */
  thinking_enabled: boolean;
  /** Token budget for extended thinking when thinking_enabled (>= 1024). */
  thinking_budget_tokens: number;
  mcp_servers: Record<string, McpServerConfig>;
  telegram: TelegramConfig;
  heartbeat_enabled: boolean;
  heartbeat_interval: number;
  heartbeat_context_turns: number;
}

export interface AppSettings {
  live_2d_model_path: string;
  language: string;
  gallery_dir: string;
  gallery_enabled: boolean;
  gallery_interval: number;
  /** Tavily API key for the web_search tool (shared by all agents). Empty = disabled. */
  search_api_key: string;
  /** Directory scanned for Agent Skills (shared by all agents). Empty = ~/.agents/skills. */
  skills_dir: string;
  /** Id of the agent that answers the desktop chat window. */
  active_agent: string;
  agents: AgentConfig[];
}

/** One skill discovered under the skills dir (from the `list_skills` command). */
export interface SkillItem {
  name: string;
  /** Directory name — the identifier behind `/skill:<slug>`. */
  slug: string;
  description: string;
  /** Absolute path to SKILL.md. */
  path: string;
  /** Read/parse failure; such a skill is excluded from the prompt. */
  error: string | null;
}

export interface SkillsInfo {
  /** The directory actually scanned (`~` already expanded). */
  dir: string;
  /** Quick-set candidates for `skills_dir`, in the form stored in config. */
  presets: string[];
  skills: SkillItem[];
}

/** Live MCP server connection status (from the `get_mcp_status` command). */
export interface McpStatus {
  name: string;
  connected: boolean;
  tool_count: number;
  tool_names: string[];
  error: string | null;
}

/** Live Telegram bot status (from the `get_telegram_status` command). */
export interface TelegramStatus {
  running: boolean;
  error: string | null;
}

export function defaultAgent(id = "default", name = "默认"): AgentConfig {
  return {
    id,
    name,
    api_base: "https://api.openai.com/v1",
    api_key: "",
    model: "gpt-4o-mini",
    context_window: 128000,
    reasoning_effort: "",
    thinking_enabled: false,
    thinking_budget_tokens: 1024,
    mcp_servers: {},
    telegram: { bot_token: "", allowed_username: "", enabled: false },
    heartbeat_enabled: false,
    heartbeat_interval: 60,
    heartbeat_context_turns: 10,
  };
}

const DEFAULT_SETTINGS: AppSettings = {
  live_2d_model_path: "/models/miku/miku.model3.json",
  language: "zh",
  gallery_dir: "",
  gallery_enabled: false,
  gallery_interval: 10,
  search_api_key: "",
  skills_dir: "",
  active_agent: "default",
  agents: [defaultAgent()],
};

export function useSettings() {
  const [settings, setSettings] = useState<AppSettings>(DEFAULT_SETTINGS);
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    invoke<AppSettings>("get_settings")
      .then((s) => {
        setSettings(s);
        setLoaded(true);
      })
      .catch((e) => {
        console.error("Failed to load settings:", e);
        setLoaded(true);
      });
  }, []);

  // Settings are saved from the panel window but consumed here in every window
  // (each holds its own in-memory copy). Reload when any window persists a change
  // so e.g. the pet picks up gallery mode without needing a refocus.
  useTauriEvent("settings-changed", () => {
    invoke<AppSettings>("get_settings")
      .then(setSettings)
      .catch((e) => console.error("Failed to reload settings:", e));
  });

  return { settings, loaded };
}
