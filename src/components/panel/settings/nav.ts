import type { ComponentType } from "react";
import type { TKey } from "../../../i18n";
import {
  ImageIcon,
  GearIcon,
  WrenchIcon,
  FileTextIcon,
  TerminalIcon,
  GlobeIcon,
  SearchIcon,
} from "../../Icons";

/** Agent tabs are keyed by id so every agent gets its own rail entry. */
export const AGENT_PREFIX = "agent:";

/**
 * The left rail's tabs. Two top-level groups (Pet / AI); the agent group is
 * built from the agent list (one tab per agent); the general group (language,
 * raw config) follows the others.
 */
export type TabId =
  | "pet-visual"
  | "ai-models"
  | "ai-mcp"
  | "ai-prompts"
  | "ai-tools"
  | "ai-search"
  | "ai-skills"
  | "general-language"
  | "raw"
  | `agent:${string}`;

export interface NavEntry {
  id: TabId;
  label: TKey;
  icon: ComponentType<{ className?: string }>;
}

export const NAV_GROUPS: { title: TKey; items: NavEntry[] }[] = [
  {
    title: "settings.group.pet",
    items: [{ id: "pet-visual", label: "settings.pet.title", icon: ImageIcon }],
  },
  {
    title: "settings.group.ai",
    items: [
      { id: "ai-models", label: "settings.models.title", icon: GearIcon },
      { id: "ai-mcp", label: "settings.mcp.title", icon: WrenchIcon },
      { id: "ai-prompts", label: "settings.prompts.title", icon: FileTextIcon },
      { id: "ai-tools", label: "settings.tools.title", icon: TerminalIcon },
      { id: "ai-search", label: "settings.search.title", icon: GlobeIcon },
      { id: "ai-skills", label: "settings.skills.title", icon: SearchIcon },
    ],
  },
];

/** General settings (language, raw config), rendered after the agent group. */
export const GENERAL_ITEMS: NavEntry[] = [
  { id: "general-language", label: "settings.language", icon: GlobeIcon },
  { id: "raw", label: "settings.tab.file", icon: FileTextIcon },
];