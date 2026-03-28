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

export interface AppSettings {
  live_2d_model_path: string;
  api_base: string;
  api_key: string;
  model: string;
  mcp_servers: Record<string, McpServerConfig>;
}

const DEFAULT_SETTINGS: AppSettings = {
  live_2d_model_path: "/models/miku/miku.model3.json",
  api_base: "https://api.openai.com/v1",
  api_key: "",
  model: "gpt-4o-mini",
  mcp_servers: {},
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
