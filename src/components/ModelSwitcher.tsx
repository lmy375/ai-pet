import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useSettings } from "../hooks/useSettings";
import { useI18n } from "../i18n";

/**
 * Compact dropdown to switch the active agent's model from inside the chat view,
 * without opening settings. Options come from the agent's `/models` endpoint
 * (`list_models`); the current model is always present even while that list is
 * loading or failed to load. Picking one writes `agent.model` via
 * `set_agent_model`, which emits `settings-changed` so both windows reload.
 */
export function ModelSwitcher({ className = "" }: { className?: string }) {
  const { settings, loaded } = useSettings();
  const { t } = useI18n();
  const [models, setModels] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);

  const agent = settings.agents.find((a) => a.id === settings.active_agent);
  const apiBase = agent?.api_base ?? "";
  const apiKey = agent?.api_key ?? "";
  const current = agent?.model ?? "";

  // Refetch the model list whenever the active agent (or its credentials) change.
  // Failures leave the list empty — the current model still shows via the fallback
  // option below, so the switcher never renders blank.
  useEffect(() => {
    if (!apiBase.trim()) {
      setModels([]);
      return;
    }
    let cancelled = false;
    setLoading(true);
    invoke<string[]>("list_models", { apiBase, apiKey })
      .then((list) => !cancelled && setModels(list))
      .catch(() => !cancelled && setModels([]))
      .finally(() => !cancelled && setLoading(false));
    return () => { cancelled = true; };
  }, [apiBase, apiKey]);

  if (!loaded || !agent) return null;

  const switchModel = (model: string) => {
    if (!model || model === current) return;
    invoke("set_agent_model", { id: agent.id, model })
      .catch((e) => console.error("Failed to switch model:", e));
  };

  // Ensure the current model is selectable even if it's absent from (or ahead of)
  // the fetched list.
  const options = current && !models.includes(current) ? [current, ...models] : models;

  return (
    <select
      value={current}
      onChange={(e) => switchModel(e.target.value)}
      title={loading ? t("chat.model.loading") : t("chat.model.switch")}
      className={`min-w-0 shrink rounded-lg border border-slate-200 bg-white px-2 py-1 text-[12px] font-medium text-slate-700 outline-none transition-colors hover:border-slate-300 focus:border-accent ${className}`}
    >
      {options.length === 0 && <option value="">—</option>}
      {options.map((m) => (
        <option key={m} value={m}>{m}</option>
      ))}
    </select>
  );
}
