import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTauriEvent } from "./useTauriEvent";

/** One MCP server in the global pool. Agents reference it by name. */
export interface McpServerConfig {
  transport: "stdio" | "sse" | "http";
  command: string;
  args: string[];
  url: string;
  headers: Record<string, string>;
  env: Record<string, string>;
  /** Off = not connected for anyone, without editing the agents that list it. */
  enabled: boolean;
}

/** One entry of the global model pool: everything needed to reach a model. */
export interface ModelConfig {
  /** Wire protocol to speak; "" = auto-detect from the model name. Defaults to "openai". */
  provider: string;
  api_base: string;
  api_key: string;
  /** The model id sent on the wire (the pool key is just the display name). */
  model: string;
  context_window: number;
  /** Reasoning control: "" (off) or an effort keyword (minimal/low/medium/high/xhigh/max),
   *  or a plain token count for an explicit thinking budget. */
  reasoning: string;
}

export interface TelegramConfig {
  bot_token: string;
  allowed_username: string;
  enabled: boolean;
}

/** One configurable agent: a persona/memory plus references into the global pools. */
export interface AgentConfig {
  id: string;
  name: string;
  /** Name of the `models` entry this agent talks through. */
  model: string;
  /** Names of the `mcp_servers` entries this agent may call. */
  mcp: string[];
  telegram: TelegramConfig;
  heartbeat_enabled: boolean;
  heartbeat_interval: number;
  heartbeat_context_turns: number;
}

/** Tool names the owner switched off. Descriptions live in `<config>/prompts/`. */
export interface ToolSettings {
  disabled: string[];
}

export interface AppSettings {
  /** The global model pool, keyed by display name. */
  models: Record<string, ModelConfig>;
  /** The global MCP server pool, keyed by name. */
  mcp_servers: Record<string, McpServerConfig>;
  /** Path to the .model3.json. Empty = not set (pet window shows a notice). */
  live_2d_model_path: string;
  /** Path to live2dcubismcore.min.js. Empty = not set (same notice). */
  live_2d_core_path: string;
  language: string;
  /** Which visual the pet window shows: "live2d" | "gallery" | "image". */
  pet_kind: string;
  /** Directory the image pet reads its art from (idle/thinking/happy files).
   *  Empty = the bundled public/pet art. */
  pet_image_dir: string;
  gallery_dir: string;
  gallery_interval: number;
  /** Tavily API key for the web_search tool (shared by all agents). Empty = disabled. */
  search_api_key: string;
  /** Directory scanned for Agent Skills (shared by all agents). Empty = ~/.agents/skills. */
  skills_dir: string;
  /** Tools switched off for every agent (built-in or MCP, by name). */
  tools: ToolSettings;
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

/** A fresh agent for the settings "add agent" button. Default VALUES live on
 *  the Rust side (`AgentConfig::default` in pet-core) — the UI keeps no copy
 *  that could drift; it only supplies the identity (id + display name). */
export const newAgent = (id: string, name: string) =>
  invoke<AgentConfig>("new_agent", { id, name });

/** A fresh model-pool entry — same single-source rule as `newAgent`. */
export const newModel = () => invoke<ModelConfig>("new_model");

/** A fresh MCP server entry — same single-source rule as `newAgent`. */
export const newMcpServer = () => invoke<McpServerConfig>("new_mcp_server");

/** Structural shell for the pre-load instant only — App renders nothing until
 *  settings arrive (`loaded`), so no meaningful default lives here. All default
 *  VALUES live in pet-core `settings.rs` (`Default` impls + serde defaults),
 *  served to the UI through `get_settings` and the `new_*` templates. */
export const EMPTY_SETTINGS: AppSettings = {
  models: {},
  mcp_servers: {},
  live_2d_model_path: "",
  live_2d_core_path: "",
  language: "",
  pet_kind: "",
  pet_image_dir: "",
  gallery_dir: "",
  gallery_interval: 0,
  search_api_key: "",
  skills_dir: "",
  tools: { disabled: [] },
  active_agent: "",
  agents: [],
};

export function useSettings() {
  const [settings, setSettings] = useState<AppSettings>(EMPTY_SETTINGS);
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
