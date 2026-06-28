import { invoke } from "@tauri-apps/api/core";
import { useSettings } from "../hooks/useSettings";
import { useI18n } from "../i18n";

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
    <select
      value={settings.active_agent}
      onChange={(e) => switchAgent(e.target.value)}
      title={t("chat.agent.switch")}
      className={`shrink-0 rounded-lg border border-slate-200 bg-white px-2 py-1 text-[12px] font-medium text-slate-700 outline-none transition-colors hover:border-slate-300 focus:border-accent ${className}`}
    >
      {settings.agents.map((a) => (
        <option key={a.id} value={a.id}>{a.name}</option>
      ))}
    </select>
  );
}
