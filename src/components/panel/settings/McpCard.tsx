import { useEffect, useState } from "react";
import type { AppSettings, McpServerConfig, McpStatus } from "../../../hooks/useSettings";
import { emptyMcpServer } from "../../../hooks/useSettings";
import { Button } from "../../ui/Button";
import { Badge } from "../../ui/Badge";
import { ChipTabs } from "../../ui/ChipTabs";
import { IconActionButton } from "../../ui/IconButton";
import { ErrorBox, HintText } from "../../ui/feedback";
import { SavedTextInput, TextArea, Select } from "../../ui/fields";
import { SettingsSection, SettingsRow, Switch } from "../../ui/settings";
import { TrashIcon } from "../../Icons";
import { toneDot, toneText, connTone } from "../../../utils/tone";
import { useI18n } from "../../../i18n";
import { renameKey, uniqueName } from "./pool";

interface Props {
  settings: AppSettings;
  onDraft: (next: AppSettings) => void;
  onCommit: (next: AppSettings) => void;
  /** Live status of every server in the pool (connections are global). */
  statuses: McpStatus[];
  onReconnect: () => void;
  /** Flip one server on/off and apply it to the running pool right away. */
  onToggle: (name: string, enabled: boolean) => void;
  busy: boolean;
}

/**
 * The global MCP server pool. One connection per server, shared by every agent
 * that lists it in its `mcp` — so reconnecting here reconnects for everyone.
 */
export function McpCard({ settings, onDraft, onCommit, statuses, onReconnect, onToggle, busy }: Props) {
  const { t } = useI18n();
  const names = Object.keys(settings.mcp_servers);
  const [selected, setSelected] = useState<string | null>(names[0] ?? null);
  const [nameDraft, setNameDraft] = useState(selected ?? "");

  const config: McpServerConfig | undefined = selected ? settings.mcp_servers[selected] : undefined;
  const status = statuses.find((s) => s.name === selected);

  useEffect(() => {
    if (selected && settings.mcp_servers[selected]) return;
    const next = names[0] ?? null;
    setSelected(next);
    setNameDraft(next ?? "");
  }, [names.join(" ")]);

  const withServers = (mcp_servers: Record<string, McpServerConfig>): AppSettings => ({ ...settings, mcp_servers });

  const update = (updates: Partial<McpServerConfig>, commit: boolean) => {
    if (!selected || !config) return;
    const next = withServers({ ...settings.mcp_servers, [selected]: { ...config, ...updates } });
    (commit ? onCommit : onDraft)(next);
  };

  const add = () => {
    const name = uniqueName(t("settings.mcp.newName"), names);
    onCommit(withServers({ ...settings.mcp_servers, [name]: emptyMcpServer() }));
    setSelected(name);
    setNameDraft(name);
  };

  // Renaming re-points every agent that listed the old name, in the same write.
  const rename = (next: string) => {
    const name = next.trim();
    if (!selected || !name || name === selected) { setNameDraft(selected ?? ""); return; }
    if (settings.mcp_servers[name]) { setNameDraft(selected); return; }
    onCommit({
      ...settings,
      mcp_servers: renameKey(settings.mcp_servers, selected, name),
      agents: settings.agents.map((a) => ({ ...a, mcp: a.mcp.map((m) => (m === selected ? name : m)) })),
    });
    setSelected(name);
    setNameDraft(name);
  };

  // Unlike a model, a deleted server just costs its agents some tools — so this
  // cascades instead of blocking, and leaves no dangling references behind.
  const remove = () => {
    if (!selected) return;
    const { [selected]: _, ...rest } = settings.mcp_servers;
    onCommit({
      ...settings,
      mcp_servers: rest,
      agents: settings.agents.map((a) => ({ ...a, mcp: a.mcp.filter((m) => m !== selected) })),
    });
  };

  const hasError = !!status?.error;
  const statusLabel = !config?.enabled
    ? t("settings.mcp.disabled")
    : status?.connected
      ? t("settings.mcp.connected")
      : status?.error
        ? t("settings.mcp.connFailed")
        : t("settings.mcp.disconnected");

  return (
    <SettingsSection

      action={
        <Button size="sm" onClick={onReconnect} disabled={busy}>
          {busy ? t("settings.connecting") : t("settings.saveConnect")}
        </Button>
      }
    >
      <SettingsRow>
        <ChipTabs
          items={names.map((name) => {
            const s = statuses.find((x) => x.name === name);
            return { name, dotClass: toneDot(connTone(s?.connected, s?.error)) };
          })}
          selected={selected}
          onSelect={(name) => { setSelected(name); setNameDraft(name); }}
          onAdd={add}
          addTitle={t("settings.mcp.add")}
        />
      </SettingsRow>

      {!config ? (
        <SettingsRow>
          <HintText className="mt-0">{t("settings.mcp.empty")}</HintText>
        </SettingsRow>
      ) : (
        <>
          <SettingsRow
            label={
              <span className="flex items-center gap-2">
                <span>{t("settings.models.name")}</span>
                <span className={`font-normal text-meta ${toneText(connTone(status?.connected, hasError))}`}>
                  {statusLabel}
                  {status?.connected && ` · ${t("settings.mcp.toolsSuffix", { count: status.tool_count })}`}
                </span>
              </span>
            }
          >
            <div className="flex gap-2">
              <SavedTextInput
                value={nameDraft}
                onChange={(e) => setNameDraft(e.target.value)}
                onCommit={() => rename(nameDraft)}
                className="flex-1"
              />
              <IconActionButton variant="danger" size="sm" onClick={remove} title={t("common.delete")}>
                <TrashIcon className="h-4 w-4" />
              </IconActionButton>
            </div>
          </SettingsRow>

          {/* Global switch: turning it off stops the process for every agent
              that lists this server, without editing any of them. */}
          <SettingsRow
            label={t("settings.mcp.enable")}
            control={
              <Switch
                checked={config.enabled}
                disabled={busy}
                onChange={(on) => onToggle(selected!, on)}
              />
            }
          />

          {hasError && (
            <SettingsRow><ErrorBox className="mt-0">{status!.error}</ErrorBox></SettingsRow>
          )}

          <SettingsRow
            label={t("settings.mcp.transport")}
            control={
              <Select
                className="w-52"
                value={config.transport}
                onChange={(e) => update({ transport: e.target.value as McpServerConfig["transport"] }, true)}
              >
                <option value="stdio">{t("settings.mcp.transport.stdio")}</option>
                <option value="sse">{t("settings.mcp.transport.sse")}</option>
                <option value="http">{t("settings.mcp.transport.http")}</option>
              </Select>
            }
          />

          {config.transport === "stdio" ? (
            <>
              <SettingsRow label={t("settings.mcp.command")}>
                <SavedTextInput
                  value={config.command}
                  onChange={(e) => update({ command: e.target.value }, false)}
                  onCommit={() => update({}, true)}
                  className="font-mono !text-[12px]"
                  placeholder="npx"
                />
              </SettingsRow>
              <SettingsRow label={t("settings.mcp.args")}>
                <TextArea
                  value={config.args.join("\n")}
                  onChange={(e) => update({ args: e.target.value.split("\n") }, false)}
                  onBlur={() => update({}, true)}
                  rows={3}
                  className="font-mono !text-[12px]"
                  placeholder={"-y\n@modelcontextprotocol/server-filesystem\n/tmp"}
                />
              </SettingsRow>
              <SettingsRow label={t("settings.mcp.env")}>
                <TextArea
                  value={Object.entries(config.env || {}).map(([k, v]) => `${k}=${v}`).join("\n")}
                  onChange={(e) => {
                    const env: Record<string, string> = {};
                    e.target.value.split("\n").forEach((line) => {
                      const idx = line.indexOf("=");
                      if (idx > 0) env[line.slice(0, idx)] = line.slice(idx + 1);
                    });
                    update({ env }, false);
                  }}
                  onBlur={() => update({}, true)}
                  rows={2}
                  className="font-mono !text-[12px]"
                  placeholder="GITHUB_TOKEN=ghp_xxx"
                />
              </SettingsRow>
            </>
          ) : (
            <>
              <SettingsRow label="URL">
                <SavedTextInput
                  value={config.url}
                  onChange={(e) => update({ url: e.target.value }, false)}
                  onCommit={() => update({}, true)}
                  className="font-mono !text-[12px]"
                  placeholder="http://localhost:3000/mcp"
                />
              </SettingsRow>
              <SettingsRow label={t("settings.mcp.headers")}>
                <TextArea
                  value={Object.entries(config.headers || {}).map(([k, v]) => `${k}: ${v}`).join("\n")}
                  onChange={(e) => {
                    const headers: Record<string, string> = {};
                    e.target.value.split("\n").forEach((line) => {
                      const idx = line.indexOf(":");
                      if (idx > 0) headers[line.slice(0, idx).trim()] = line.slice(idx + 1).trim();
                    });
                    update({ headers }, false);
                  }}
                  onBlur={() => update({}, true)}
                  rows={2}
                  className="font-mono !text-[12px]"
                  placeholder="Authorization: Bearer xxx"
                />
              </SettingsRow>
            </>
          )}

          {status?.connected && status.tool_names.length > 0 && (
            <SettingsRow label={t("settings.mcp.registeredTools", { count: status.tool_count })}>
              <div className="flex flex-wrap gap-1">
                {status.tool_names.map((name) => (
                  <Badge key={name} color="sky" className="font-mono">{name}</Badge>
                ))}
              </div>
            </SettingsRow>
          )}
        </>
      )}
    </SettingsSection>
  );
}