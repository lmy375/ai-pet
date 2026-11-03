import type { AgentConfig, AppSettings, McpStatus, TelegramStatus } from "../../../hooks/useSettings";
import { Button } from "../../ui/Button";
import { ErrorBox, HintText } from "../../ui/feedback";
import { SavedTextInput, Select, NumberField } from "../../ui/fields";
import { StatusText } from "../../ui/StatusText";
import { SettingsSection, SettingsRow, Switch } from "../../ui/settings";
import { TrashIcon } from "../../Icons";
import { toneText, toneDot, connTone } from "../../../utils/tone";
import { useI18n } from "../../../i18n";
import { AgentMemory } from "../PanelMemory";

interface Props {
  form: AppSettings;
  agent: AgentConfig;
  mcpStatuses: McpStatus[];
  telegramStatus: TelegramStatus;
  telegramReconnecting: boolean;
  updateAgent: (updates: Partial<AgentConfig>) => void;
  commitAgent: (updates: Partial<AgentConfig>) => void;
  saveSettings: () => void;
  removeAgent: (id: string) => void;
  setActiveAgent: (id: string) => void;
  goToPool: (id: "ai-models" | "ai-mcp") => void;
  reconnectTelegram: (agentId: string) => Promise<void>;
}

/** Everything one agent owns: identity, its pool references, Telegram and heartbeat. */
export function AgentPage({
  form,
  agent,
  mcpStatuses,
  telegramStatus,
  telegramReconnecting,
  updateAgent,
  commitAgent,
  saveSettings,
  removeAgent,
  setActiveAgent,
  goToPool,
  reconnectTelegram,
}: Props) {
  const { t } = useI18n();

  const modelNames = Object.keys(form.models);
  const serverNames = Object.keys(form.mcp_servers);
  // A reference that no longer resolves: show it (so it can be seen and fixed)
  // rather than silently selecting something else.
  const danglingModel = !!agent?.model && !form.models[agent.model];

  const toggleAgentServer = (name: string, on: boolean) => {
    const mcp = on ? [...agent.mcp, name] : agent.mcp.filter((m) => m !== name);
    commitAgent({ mcp });
  };

  return (
    <>
      {/* Identity */}
      <SettingsSection title={t("settings.agent.identityTitle")}>
        <SettingsRow label={t("settings.agent.name")}>
          <div className="flex gap-2">
            <SavedTextInput
              value={agent.name}
              onChange={(e) => updateAgent({ name: e.target.value })}
              onCommit={() => saveSettings()}
              className="flex-1"
              placeholder={t("settings.agent.newName")}
            />
            <Button
              variant="secondary"
              onClick={() => removeAgent(agent.id)}
              disabled={form.agents.length <= 1}
              title={t("settings.agent.remove")}
            >
              <TrashIcon className="h-4 w-4" />
              {t("settings.agent.remove")}
            </Button>
          </div>
        </SettingsRow>
        <SettingsRow
          label={<span className="font-normal text-ink-faint">{t("settings.agent.idNote", { id: agent.id })}</span>}
          control={
            agent.id === form.active_agent ? (
              <span className="text-note font-medium text-accent">{t("settings.agent.isDefault")}</span>
            ) : (
              <button onClick={() => setActiveAgent(agent.id)} className="text-note font-medium text-accent hover:underline">
                {t("settings.agent.setDefault")}
              </button>
            )
          }
        />
      </SettingsSection>

      {/* Model */}
      <SettingsSection
        title={t("settings.agent.modelTitle")}
        action={
          <button onClick={() => goToPool("ai-models")} className="text-note font-medium text-accent hover:underline">
            {t("settings.agent.goConfigure")}
          </button>
        }
      >
        {modelNames.length === 0 ? (
          <SettingsRow><HintText className="mt-0">{t("settings.agent.modelEmpty")}</HintText></SettingsRow>
        ) : (
          <SettingsRow
            label={t("settings.agent.modelTitle")}
            description={danglingModel ? undefined : t("settings.agent.modelNote")}
            control={
              <Select
                className="w-56"
                value={agent.model}
                onChange={(model) => commitAgent({ model })}
                options={[
                  { value: "", label: t("settings.agent.modelNone") },
                  ...(danglingModel ? [{ value: agent.model, label: agent.model }] : []),
                  ...modelNames.map((name) => ({ value: name, label: name })),
                ]}
              />
            }
          />
        )}
        {danglingModel && (
          <SettingsRow>
            <StatusText ok={false} className="text-note">{t("settings.agent.modelMissing", { model: agent.model })}</StatusText>
          </SettingsRow>
        )}
      </SettingsSection>

      {/* MCP */}
      <SettingsSection
        title={t("settings.agent.mcpTitle")}
        description={t("settings.agent.mcpNote")}
        action={
          <button onClick={() => goToPool("ai-mcp")} className="text-note font-medium text-accent hover:underline">
            {t("settings.agent.goConfigure")}
          </button>
        }
      >
        {serverNames.length === 0 ? (
          <SettingsRow><HintText className="mt-0">{t("settings.agent.mcpEmpty")}</HintText></SettingsRow>
        ) : (
          serverNames.map((name) => {
            const status = mcpStatuses.find((s) => s.name === name);
            const off = !form.mcp_servers[name].enabled;
            return (
              <SettingsRow
                key={name}
                label={
                  <span className="flex items-center gap-2">
                    <span className={`h-1.5 w-1.5 shrink-0 rounded-full ${toneDot(connTone(status?.connected, status?.error))}`} />
                    {name}
                  </span>
                }
                description={
                  off
                    ? t("settings.mcp.disabled")
                    : status?.connected
                      ? t("settings.mcp.toolsSuffix", { count: status.tool_count })
                      : undefined
                }
                control={
                  <Switch checked={agent.mcp.includes(name)} onChange={(on) => toggleAgentServer(name, on)} />
                }
              />
            );
          })
        )}
      </SettingsSection>

      {/* Telegram */}
      <SettingsSection
        title={t("settings.agent.tgTitle")}
        description={
          <span className={toneText(connTone(telegramStatus.running, telegramStatus.error))}>
            {telegramStatus.running ? t("settings.tg.running") : telegramStatus.error ? t("settings.tg.connFailed") : t("settings.tg.stopped")}
          </span>
        }
        action={
          <Button
            size="sm"
            disabled={telegramReconnecting}
            onClick={() => reconnectTelegram(agent.id)}
          >
            {telegramReconnecting ? t("settings.connecting") : t("settings.saveConnect")}
          </Button>
        }
      >
        {telegramStatus.error && (
          <SettingsRow><ErrorBox className="mt-0">{telegramStatus.error}</ErrorBox></SettingsRow>
        )}
        <SettingsRow
          label={t("settings.tg.enable")}
          control={
            <Switch
              checked={agent.telegram?.enabled ?? false}
              onChange={(on) => commitAgent({ telegram: { ...agent.telegram, enabled: on } })}
            />
          }
        />
        <SettingsRow label="Bot Token">
          <SavedTextInput
            type="password"
            value={agent.telegram?.bot_token ?? ""}
            onChange={(e) => updateAgent({ telegram: { ...agent.telegram, bot_token: e.target.value } })}
            onCommit={() => saveSettings()}
            className="font-mono !text-[12px]"
            placeholder="123456789:ABCdefGhI..."
          />
        </SettingsRow>
        <SettingsRow label={t("settings.tg.allowedUser")}>
          <SavedTextInput
            value={agent.telegram?.allowed_username ?? ""}
            onChange={(e) => updateAgent({ telegram: { ...agent.telegram, allowed_username: e.target.value } })}
            onCommit={() => saveSettings()}
            className="font-mono !text-[12px]"
            placeholder={t("settings.tg.allowedUserPlaceholder")}
          />
        </SettingsRow>
      </SettingsSection>

      {/* Scheduled heartbeat */}
      <SettingsSection title={t("settings.hb.title")}>
        <SettingsRow
          label={t("settings.hb.enable")}
          description={t("settings.hb.note")}
          align="start"
          control={
            <Switch
              checked={agent.heartbeat_enabled}
              onChange={(on) => commitAgent({ heartbeat_enabled: on })}
            />
          }
        />
        <SettingsRow
          label={t("settings.hb.interval")}
          control={
            <NumberField
              className="w-24"
              value={agent.heartbeat_interval}
              fallback={60}
              onChange={(v) => updateAgent({ heartbeat_interval: v })}
              onCommit={(v) => commitAgent({ heartbeat_interval: v })}
              placeholder="60"
            />
          }
        />
        <SettingsRow
          label={t("settings.hb.contextTurns")}
          description={t("settings.hb.contextTurnsNote")}
          align="start"
          control={
            <NumberField
              className="w-24"
              value={agent.heartbeat_context_turns}
              min={0}
              fallback={0}
              onChange={(v) => updateAgent({ heartbeat_context_turns: v })}
              onCommit={(v) => commitAgent({ heartbeat_context_turns: v })}
              placeholder="10"
            />
          }
        />
      </SettingsSection>

      {/* Memory */}
      <AgentMemory key={agent.id} agentId={agent.id} />
    </>
  );
}