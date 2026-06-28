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
  /** Id of the agent that answers the desktop chat window. */
  active_agent: string;
  agents: AgentConfig[];
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
