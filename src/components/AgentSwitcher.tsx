import { invoke } from "@tauri-apps/api/core";
import { useSettings } from "../hooks/useSettings";
import { useI18n } from "../i18n";
import { Select } from "./ui/fields";

/**
 * Compact dropdown to switch the active agent (the one answering the desktop
 * chat). Available in any chat window; switching just rewrites the global
 * `active_agent` (chat history is shared, so it stays put). Both windows reload
 * via the `settings-changed` event that `set_active_agent` emits.
 */
export function AgentSwitcher({ className = "" }: { className?: string }) {
  const { settings, loaded } = useSettings();
  const { t } = useI18n();

  if (!loaded || settings.agents.length === 0) return null;

  const switchAgent = (id: string) => {
    if (id === settings.active_agent) return;
    invoke("set_active_agent", { id }).catch((e) => console.error("Failed to switch agent:", e));
  };

  return (
    <Select
      size="sm"
      className={`shrink-0 ${className}`}
      value={settings.active_agent}
      onChange={switchAgent}
      title={t("chat.agent.switch")}
      options={settings.agents.map((a) => ({ value: a.id, label: a.name }))}
    />
  );
}
