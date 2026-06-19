import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

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

export interface AppSettings {
  live_2d_model_path: string;
  api_base: string;
  api_key: string;
  model: string;
  mcp_servers: Record<string, McpServerConfig>;
  telegram: TelegramConfig;
  gallery_dir: string;
  gallery_enabled: boolean;
  gallery_interval: number;
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

const DEFAULT_SETTINGS: AppSettings = {
  live_2d_model_path: "/models/miku/miku.model3.json",
  api_base: "https://api.openai.com/v1",
  api_key: "",
  model: "gpt-4o-mini",
  mcp_servers: {},
  telegram: { bot_token: "", allowed_username: "", enabled: false },
  gallery_dir: "",
  gallery_enabled: false,
  gallery_interval: 10,
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

  // Settings are saved from the panel window but consumed here in every window
  // (each holds its own in-memory copy). Reload when any window persists a change
  // so e.g. the pet picks up gallery mode without needing a refocus.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    listen("settings-changed", () => {
      invoke<AppSettings>("get_settings")
        .then(setSettings)
        .catch((e) => console.error("Failed to reload settings:", e));
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
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
