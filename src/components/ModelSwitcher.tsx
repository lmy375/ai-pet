import { invoke } from "@tauri-apps/api/core";
import { useSettings } from "../hooks/useSettings";
import { useI18n } from "../i18n";
import { Select } from "./ui/fields";

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
  const pool = Object.keys(settings.models);
  // A dangling reference stays visible instead of being silently swapped out.
  const names = current && !pool.includes(current) ? [current, ...pool] : pool;
  const config = settings.models[current];

  const options = [
    ...(names.length === 0 ? [{ value: "", label: t("chat.model.none") }] : []),
    ...(!current && names.length > 0 ? [{ value: "", label: "—" }] : []),
    ...names.map((m) => ({ value: m, label: m })),
  ];

  const switchModel = (model: string) => {
    if (!model || model === current) return;
    invoke("set_agent_model", { id: agent.id, model })
      .catch((e) => console.error("Failed to switch model:", e));
  };

  return (
    <Select
      size="sm"
      className={`min-w-0 shrink ${className}`}
      value={current}
      onChange={switchModel}
      placeholder={t("chat.model.none")}
      title={config ? `${config.provider || "auto"} · ${config.model}` : t("chat.model.switch")}
      options={options}
    />
  );
}
