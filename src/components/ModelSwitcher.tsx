import { invoke } from "@tauri-apps/api/core";
import { useSettings } from "../hooks/useSettings";
import { useI18n } from "../i18n";

/**
 * Compact dropdown to switch the active agent's model from inside the chat view.
 * The options are the entries of the global model pool (`models` in config.yaml)
 * — no endpoint call, since switching model here means pointing the agent at
 * another configured entry, which carries its own endpoint, context window and
 * reasoning effort. Picking one writes `agent.model` via `set_agent_model`,
 * which emits `settings-changed` so both windows reload.
 */
export function ModelSwitcher({ className = "" }: { className?: string }) {
  const { settings, loaded } = useSettings();
  const { t } = useI18n();

  const agent = settings.agents.find((a) => a.id === settings.active_agent);
  if (!loaded || !agent) return null;

  const current = agent.model;
  const names = Object.keys(settings.models);
  // A dangling reference stays visible instead of being silently swapped out.
  const options = current && !names.includes(current) ? [current, ...names] : names;
  const config = settings.models[current];

  const switchModel = (model: string) => {
    if (!model || model === current) return;
    invoke("set_agent_model", { id: agent.id, model })
      .catch((e) => console.error("Failed to switch model:", e));
  };

  return (
    <select
      value={current}
      onChange={(e) => switchModel(e.target.value)}
      title={config ? `${config.provider || "auto"} · ${config.model}` : t("chat.model.switch")}
      className={`min-w-0 shrink rounded-field border border-line bg-surface px-2 py-1.5 text-note font-medium text-ink outline-none transition-colors hover:border-accent-line focus:border-accent ${className}`}
    >
      {options.length === 0 && <option value="">{t("chat.model.none")}</option>}
      {!current && options.length > 0 && <option value="">—</option>}
      {options.map((m) => (
        <option key={m} value={m}>{m}</option>
      ))}
    </select>
  );
}
