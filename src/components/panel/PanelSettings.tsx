import { useState, useEffect, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { SkillsInfo } from "../../hooks/useSettings";
import { LoadingScreen } from "../ui/feedback";
import { SavedTextInput } from "../ui/fields";
import { StatusText } from "../ui/StatusText";
import { SettingsSection, SettingsRow } from "../ui/settings";
import { ModelsCard } from "./settings/ModelsCard";
import { PromptsCard } from "./settings/PromptsCard";
import { ToolsCard } from "./settings/ToolsCard";
import { McpCard } from "./settings/McpCard";
import { useI18n, type TKey } from "../../i18n";
import { useResizableWidth } from "../../hooks/useResizableWidth";
import { useStatusMessage } from "../../hooks/useStatusMessage";
import { ResizeHandle } from "../ui/ResizeHandle";
import { AGENT_PREFIX, GENERAL_ITEMS, NAV_GROUPS, type TabId } from "./settings/nav";
import { NavRail } from "./settings/NavRail";
import { AgentPage } from "./settings/AgentPage";
import { PetVisualCard } from "./settings/PetVisualCard";
import { SkillsPage } from "./settings/SkillsPage";
import { LanguagePage, RawConfigPage } from "./settings/GeneralPages";
import { useSettingsForm } from "./settings/useSettingsForm";
import { useMcpStatus } from "./settings/useMcpStatus";
import { useTelegramStatus } from "./settings/useTelegramStatus";
import { useSettingsPickers } from "./settings/useSettingsPickers";

export function PanelSettings() {
  const { t } = useI18n();
  const settingsNav = useResizableWidth({
    storageKey: "settings-navigation",
    initialWidth: 224,
    minWidth: 180,
    maxWidth: 360,
  });
  const [tab, setTab] = useState<TabId>("ai-models");
  // Which agent the agent tabs edit (derived from the active agent tab).
  const [agentTab, setAgentTab] = useState<string>("default");
  // Status line under the form. `ok` drives the color — derived from the action,
  // not by sniffing the message text (which breaks once it's translated).
  const status = useStatusMessage();
  const [rawYaml, setRawYaml] = useState("");
  const [skillsInfo, setSkillsInfo] = useState<SkillsInfo | null>(null);

  const telegram = useTelegramStatus({ status });
  const formState = useSettingsForm({
    agentTab,
    status,
    onLoaded: (settings) => {
      if (settings) setAgentTab(settings.agents[0]?.id ?? "default");
    },
    onAgentAdded: (id) => {
      setTab(`${AGENT_PREFIX}${id}`);
      setAgentTab(id);
      telegram.setTelegramStatus({ running: false, error: null });
    },
    onAgentRemoved: (_removedId, fallbackId) => {
      setAgentTab(fallbackId);
      setTab(`${AGENT_PREFIX}${fallbackId}`);
    },
  });
  const { form, setForm, loaded, loadInitial, loadForm, saveSettings, commitSettings, updateAgent, commitAgent, addAgent, removeAgent, setActiveAgent } = formState;

  const mcp = useMcpStatus({ form, setForm, status });

  // Re-scan the skills dir. Cheap (one read_dir), so it runs on load, after the
  // directory changes, and on the card's refresh button.
  const loadSkills = () => {
    invoke<SkillsInfo>("list_skills").then(setSkillsInfo).catch(() => setSkillsInfo(null));
  };

  const pickers = useSettingsPickers({
    form,
    setForm,
    saveSettings,
    commitSettings,
    status,
    skillsInfo,
    loadSkills,
  });

  // The agent shown across the agent group (falls back to the first agent, e.g.
  // after the edited one is deleted).
  const agentIdx = Math.max(0, form.agents.findIndex((a) => a.id === agentTab));
  const agent = form.agents[agentIdx] ?? form.agents[0];

  useEffect(() => {
    loadInitial();
    loadSkills();
    mcp.loadMcpStatuses();
  }, []);

  // Switch the left-rail tab. Loads the raw YAML when entering "config file",
  // and reloads settings from disk when leaving it (raw edits may have changed
  // them).
  const selectTab = async (next: TabId) => {
    if (next === tab) return;
    status.clear();
    if (next === "raw") {
      try {
        setRawYaml(await invoke<string>("get_config_raw"));
        setTab("raw");
      } catch (e: any) {
        status.fail(t("settings.rawLoadFailed", { error: e }));
      }
      return;
    }
    if (tab === "raw") {
      await loadForm();
    }
    setTab(next);
    if (next.startsWith(AGENT_PREFIX)) {
      const id = next.slice(AGENT_PREFIX.length);
      setAgentTab(id);
      telegram.loadTelegramStatus(id);
    }
  };

  /** Jump from an agent's reference control to the AI pool it points at. */
  const goToPool = (id: "ai-models" | "ai-mcp") => {
    status.clear();
    setTab(id);
  };

  const saveRaw = async () => {
    try {
      await invoke("save_config_raw", { content: rawYaml });
      status.ok(t("common.saved"));
    } catch (e: any) {
      status.fail(t("common.saveFailed", { error: e }));
    }
  };

  if (!loaded) {
    return <LoadingScreen />;
  }

  const messageLine = status.message && (
    <StatusText ok={status.message.ok} className="mt-3 text-note">{status.message.text}</StatusText>
  );

  const setLanguage = (language: string) => commitSettings({ ...form, language });

  /* ---------- Left rail / page-title helpers ---------- */

  const isAgentTab = tab.startsWith(AGENT_PREFIX);
  const activeAgent = isAgentTab ? form.agents.find((a) => a.id === tab.slice(AGENT_PREFIX.length)) : undefined;
  const allEntries = [...NAV_GROUPS.flatMap((g) => g.items), ...GENERAL_ITEMS];
  const pageTitle = isAgentTab
    ? activeAgent?.name || t("settings.agent.newName")
    : t(allEntries.find((e) => e.id === tab)?.label ?? "settings.tab.file");
  const currentGroupKey: TKey = isAgentTab
    ? "settings.group.agents"
    : tab.startsWith("ai-")
      ? "settings.group.ai"
      : tab.startsWith("pet-")
        ? "settings.group.pet"
        : "settings.group.general";

  /* ---------- Per-tab content ---------- */

  const renderTab = (): ReactNode => {
    switch (tab) {
      case "raw":
        return (
          <RawConfigPage
            value={rawYaml}
            onChange={setRawYaml}
            onBlurSave={saveRaw}
            onOpenConfigDir={pickers.handleOpenConfigDir}
          />
        );

      case "ai-models":
        return <ModelsCard settings={form} onDraft={setForm} onCommit={commitSettings} notify={status.ok} />;

      case "ai-mcp":
        return (
          <McpCard
            settings={form}
            onDraft={setForm}
            onCommit={commitSettings}
            statuses={mcp.mcpStatuses}
            onReconnect={mcp.handleReconnectMcp}
            onToggle={mcp.toggleMcpServer}
            busy={mcp.mcpBusy}
          />
        );

      case "ai-prompts":
        return <PromptsCard notify={status.ok} fail={status.fail} />;

      case "ai-tools":
        return <ToolsCard notify={status.ok} fail={status.fail} />;

      case "ai-search":
        return (
          <SettingsSection description={t("settings.search.apiKeyNote")}>
            <SettingsRow label={t("settings.search.apiKey")}>
              <SavedTextInput
                type="password"
                value={form.search_api_key}
                onChange={(e) => setForm({ ...form, search_api_key: e.target.value })}
                onCommit={() => saveSettings()}
                placeholder="tvly-..."
              />
            </SettingsRow>
          </SettingsSection>
        );

      case "ai-skills":
        return (
          <SkillsPage
            form={form}
            skillsInfo={skillsInfo}
            onDraft={setForm}
            loadSkills={loadSkills}
            commitSkillsDir={pickers.commitSkillsDir}
            onPickSkillsDir={pickers.handlePickSkillsDir}
            fail={status.fail}
          />
        );

      case "pet-visual":
        return (
          <PetVisualCard
            form={form}
            onDraft={setForm}
            commitSettings={commitSettings}
            saveSettings={saveSettings}
            exampleBusy={pickers.exampleBusy}
            onPickDirectory={pickers.pickDirectory}
            onOpenPath={pickers.handleOpenPath}
            onPickPetImageDir={pickers.handlePickPetImageDir}
            onPickLive2DCore={pickers.handlePickLive2DCore}
            onPickLive2DModel={pickers.handlePickLive2DModel}
            onDownloadExample={pickers.handleDownloadExample}
          />
        );

      case "general-language":
        return <LanguagePage value={form.language} onChange={setLanguage} />;

      default:
        return (
          <AgentPage
            form={form}
            agent={agent}
            mcpStatuses={mcp.mcpStatuses}
            telegramStatus={telegram.telegramStatus}
            telegramReconnecting={telegram.telegramReconnecting}
            updateAgent={updateAgent}
            commitAgent={commitAgent}
            saveSettings={saveSettings}
            removeAgent={removeAgent}
            setActiveAgent={setActiveAgent}
            goToPool={goToPool}
            reconnectTelegram={(agentId) => telegram.reconnectTelegram(form, agentId)}
          />
        );
    }
  };

  return (
    <div className="flex h-full min-h-0">
      {/* Left rail: grouped tabs. */}
      <NavRail
        width={settingsNav.width}
        tab={tab}
        agents={form.agents}
        activeAgentId={form.active_agent}
        onSelect={selectTab}
        onAddAgent={addAgent}
      />
      <ResizeHandle
        side="left"
        width={settingsNav.width}
        minWidth={180}
        maxWidth={360}
        onChange={settingsNav.setWidth}
        label={t("common.resizePanel")}
      />

      {/* Right content: page title + the active tab's sections. */}
      <div className="flex min-h-0 flex-1 flex-col bg-surface">
        <header className="shrink-0 border-b border-line/70 px-8 pb-4 pt-6">
          <p className="text-meta font-medium text-ink-faint">{t(currentGroupKey)}</p>
          <h1 className="mt-0.5 truncate text-heading font-semibold tracking-tight text-ink">{pageTitle}</h1>
        </header>
        <div className="min-h-0 flex-1 overflow-y-auto px-8 py-6">
          <div className="mx-auto max-w-2xl">
            {renderTab()}
            {messageLine}
          </div>
        </div>
      </div>
    </div>
  );
}