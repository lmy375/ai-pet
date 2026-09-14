import { useEffect, useState } from "react";
import type { AppSettings, McpServerConfig, McpStatus } from "../../../hooks/useSettings";
import { emptyMcpServer } from "../../../hooks/useSettings";
import { Card } from "../../ui/Card";
import { Button } from "../../ui/Button";
import { Badge } from "../../ui/Badge";
import { ChipTabs } from "../../ui/ChipTabs";
import { IconActionButton } from "../../ui/IconButton";
import { ErrorBox, HintText } from "../../ui/feedback";
import { Label, SavedTextInput, TextArea, Select } from "../../ui/fields";
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
  reconnecting: boolean;
}

/**
 * The global MCP server pool. One connection per server, shared by every agent
 * that lists it in its `mcp` — so reconnecting here reconnects for everyone.
 */
export function McpCard({ settings, onDraft, onCommit, statuses, onReconnect, reconnecting }: Props) {
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
  const statusLabel = status?.connected
    ? t("settings.mcp.connected")
    : status?.error
      ? t("settings.mcp.connFailed")
      : t("settings.mcp.disconnected");

  return (
    <Card
      title={t("settings.mcp.title")}
      action={
        <Button size="sm" onClick={onReconnect} disabled={reconnecting}>
          {reconnecting ? t("settings.connecting") : t("settings.saveConnect")}
        </Button>
      }
    >
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

      {!config ? (
        <HintText className="mt-3">{t("settings.mcp.empty")}</HintText>
      ) : (
        <div className="mt-3 border-t border-line pt-3">
          <Label className="flex items-center gap-2">
            <span>{t("settings.models.name")}</span>
            <span className={`font-normal text-[11px] ${toneText(connTone(status?.connected, hasError))}`}>
              {statusLabel}
              {status?.connected && ` · ${t("settings.mcp.toolsSuffix", { count: status.tool_count })}`}
            </span>
          </Label>
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

          {hasError && <ErrorBox className="mt-2">{status!.error}</ErrorBox>}

          <Label className="mt-3">{t("settings.mcp.transport")}</Label>
          <Select
            value={config.transport}
            onChange={(e) => update({ transport: e.target.value as McpServerConfig["transport"] }, true)}
            className="mb-2"
          >
            <option value="stdio">{t("settings.mcp.transport.stdio")}</option>
            <option value="sse">{t("settings.mcp.transport.sse")}</option>
            <option value="http">{t("settings.mcp.transport.http")}</option>
          </Select>

          {config.transport === "stdio" ? (
            <>
              <Label>{t("settings.mcp.command")}</Label>
              <SavedTextInput
                value={config.command}
                onChange={(e) => update({ command: e.target.value }, false)}
                onCommit={() => update({}, true)}
                className="mb-1.5 font-mono !text-[12px]"
                placeholder="npx"
              />
              <Label>{t("settings.mcp.args")}</Label>
              <TextArea
                value={config.args.join("\n")}
                onChange={(e) => update({ args: e.target.value.split("\n") }, false)}
                onBlur={() => update({}, true)}
                rows={3}
                className="mb-1.5 font-mono !text-[12px]"
                placeholder={"-y\n@modelcontextprotocol/server-filesystem\n/tmp"}
              />
              <Label>{t("settings.mcp.env")}</Label>
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
            </>
          ) : (
            <>
              <Label>URL</Label>
              <SavedTextInput
                value={config.url}
                onChange={(e) => update({ url: e.target.value }, false)}
                onCommit={() => update({}, true)}
                className="mb-1.5 font-mono !text-[12px]"
                placeholder="http://localhost:3000/mcp"
              />
              <Label>{t("settings.mcp.headers")}</Label>
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
            </>
          )}

          {status?.connected && status.tool_names.length > 0 && (
            <div className="mt-3">
              <Label>{t("settings.mcp.registeredTools", { count: status.tool_count })}</Label>
              <div className="flex flex-wrap gap-1">
                {status.tool_names.map((name) => (
                  <Badge key={name} color="sky" className="font-mono">{name}</Badge>
                ))}
              </div>
            </div>
          )}
        </div>
      )}
    </Card>
  );
}
