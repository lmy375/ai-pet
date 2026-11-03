import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { AppSettings, McpStatus } from "../../../hooks/useSettings";
import { useI18n } from "../../../i18n";
import type { useStatusMessage } from "../../../hooks/useStatusMessage";

interface Args {
  form: AppSettings;
  setForm: (next: AppSettings) => void;
  status: ReturnType<typeof useStatusMessage>;
}

/**
 * Live state of the global MCP pool. Connections are shared by every agent that
 * lists a server, so this is one status list and one busy flag for the page.
 */
export function useMcpStatus({ form, setForm, status }: Args) {
  const { t } = useI18n();
  const [mcpStatuses, setMcpStatuses] = useState<McpStatus[]>([]);
  // One busy flag for the MCP card: both the reconnect button and the per-server
  // switch talk to the same connection pool.
  const [mcpBusy, setMcpBusy] = useState(false);

  // MCP connections are global (one process per server, shared by every agent
  // that lists it), so there is one status list for the whole settings page.
  const loadMcpStatuses = () => {
    invoke<McpStatus[]>("get_mcp_status").then(setMcpStatuses).catch(() => setMcpStatuses([]));
  };

  /** Save first, then reconnect every referenced server (connections are global). */
  const handleReconnectMcp = async () => {
    setMcpBusy(true);
    status.clear();
    try {
      await invoke("save_settings", { settings: form });
      const statuses = await invoke<McpStatus[]>("reconnect_mcp");
      setMcpStatuses(statuses);
      const connected = statuses.filter((s) => s.connected).length;
      status.ok(t("settings.mcp.reconnected", { connected, total: statuses.length }));
    } catch (e: any) {
      status.fail(t("settings.mcp.reconnectFailed", { error: e }));
    } finally {
      setMcpBusy(false);
    }
  };

  /** Flip one server's switch and apply it to the running pool immediately:
   *  persist the flag, then let the backend start or stop just that server. */
  const toggleMcpServer = async (name: string, enabled: boolean) => {
    const next = {
      ...form,
      mcp_servers: { ...form.mcp_servers, [name]: { ...form.mcp_servers[name], enabled } },
    };
    setForm(next);
    setMcpBusy(true);
    status.clear();
    try {
      await invoke("save_settings", { settings: next });
      setMcpStatuses(await invoke<McpStatus[]>("sync_mcp_server", { name }));
      status.ok(enabled ? t("settings.mcp.started", { name }) : t("settings.mcp.stopped", { name }));
    } catch (e: any) {
      status.fail(t("settings.mcp.reconnectFailed", { error: e }));
    } finally {
      setMcpBusy(false);
    }
  };

  return { mcpStatuses, mcpBusy, loadMcpStatuses, handleReconnectMcp, toggleMcpServer };
}