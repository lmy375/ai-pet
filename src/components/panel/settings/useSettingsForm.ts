import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { AgentConfig, AppSettings } from "../../../hooks/useSettings";
import { EMPTY_SETTINGS, newAgent } from "../../../hooks/useSettings";
import { useI18n } from "../../../i18n";
import type { useStatusMessage } from "../../../hooks/useStatusMessage";

interface Args {
  /** The agent tab currently shown, so deleting it can jump to another. */
  agentTab: string;
  status: ReturnType<typeof useStatusMessage>;
  /** Called once the initial settings read resolves (or fails). */
  onLoaded: (settings: AppSettings | null) => void;
  /** Tab bookkeeping for a freshly added agent. */
  onAgentAdded: (id: string) => void;
  /** Tab bookkeeping when the edited agent is deleted. */
  onAgentRemoved: (removedId: string, fallbackId: string) => void;
}

/**
 * The settings form itself plus every edit of an agent entry: load / save, the
 * in-memory vs persisted split, and add / remove / set-default.
 */
export function useSettingsForm({ agentTab, status, onLoaded, onAgentAdded, onAgentRemoved }: Args) {
  const { t } = useI18n();
  const [form, setForm] = useState<AppSettings>(EMPTY_SETTINGS);
  const [loaded, setLoaded] = useState(false);

  const loadInitial = () => {
    invoke<AppSettings>("get_settings")
      .then((s) => {
        setForm(s);
        onLoaded(s);
        setLoaded(true);
      })
      .catch((e) => {
        console.error("Failed to load settings:", e);
        onLoaded(null);
        setLoaded(true);
      });
  };

  // Re-read from disk (used when leaving the raw YAML tab: edits may have
  // changed the form there).
  const loadForm = async () => {
    try { setForm(await invoke<AppSettings>("get_settings")); } catch {}
  };

  // Auto-save current form settings (on blur / Enter). `next` lets callers persist
  // an updated value immediately without waiting for a state flush.
  const saveSettings = async (next?: AppSettings) => {
    try {
      await invoke("save_settings", { settings: next ?? form });
      status.ok(t("common.saved"));
    } catch (e: any) {
      status.fail(t("common.saveFailed", { error: e }));
    }
  };

  /** Persist a settings object produced by one of the pool cards. */
  const commitSettings = (next: AppSettings) => {
    setForm(next);
    saveSettings(next);
  };

  /* ---------- Editing-agent helpers ---------- */

  const agentIdx = Math.max(0, form.agents.findIndex((a) => a.id === agentTab));

  // Update the edited agent in-memory only (used while typing); persisted on blur.
  const updateAgent = (updates: Partial<AgentConfig>) => {
    setForm((prev) => {
      const agents = [...prev.agents];
      agents[agentIdx] = { ...agents[agentIdx], ...updates };
      return { ...prev, agents };
    });
  };

  // Update the edited agent and persist immediately (for discrete controls).
  const commitAgent = (updates: Partial<AgentConfig>) => {
    const agents = [...form.agents];
    agents[agentIdx] = { ...agents[agentIdx], ...updates };
    commitSettings({ ...form, agents });
  };

  const addAgent = async () => {
    const id = crypto.randomUUID();
    const next = {
      ...form,
      agents: [...form.agents, await newAgent(id, t("settings.agent.newName"))],
    };
    setForm(next);
    onAgentAdded(id);
    status.clear();
    saveSettings(next);
  };

  const removeAgent = (id: string) => {
    if (form.agents.length <= 1) return;
    const agents = form.agents.filter((a) => a.id !== id);
    const active_agent = form.active_agent === id ? agents[0].id : form.active_agent;
    const next = { ...form, agents, active_agent };
    setForm(next);
    if (agentTab === id) {
      onAgentRemoved(id, agents[0].id);
    }
    saveSettings(next);
  };

  const setActiveAgent = (id: string) => {
    commitSettings({ ...form, active_agent: id });
  };

  return {
    form,
    setForm,
    loaded,
    loadInitial,
    loadForm,
    saveSettings,
    commitSettings,
    updateAgent,
    commitAgent,
    addAgent,
    removeAgent,
    setActiveAgent,
  };
}
